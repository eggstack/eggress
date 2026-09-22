#!/usr/bin/env python3
"""Graph-derived resumable crates.io publisher for the eggress workspace.

Manual, operator-driven release helper. The publish order is derived from
``cargo metadata`` (never from a hand-maintained tier list) and the normal
path never passes ``--no-verify`` or ``--allow-dirty`` to Cargo.

Usage:
    scripts/publish-crates.py --list
    scripts/publish-crates.py --dry-run
    scripts/publish-crates.py --execute [--tag vX.Y.Z]

Default invocation without ``--execute`` never mutates crates.io.

Background: ``cargo publish --workspace`` remains nightly-only on the
pinned stable toolchain (Cargo 1.89), so the normal path publishes one
crate at a time in metadata-derived topological order with resume support.
``--dry-run`` still validates the whole publishable workspace up front via
``cargo package --workspace --exclude eggress-bench`` (normal verification).

Resume model: before publishing each crate/version, the exact version is
queried on crates.io. Already-published versions are skipped; unpublished
crates wait for their internal prerequisites to become visible. If there is
any reason to believe an already-published same version came from the wrong
commit, stop and roll forward to a new patch version -- crates.io versions
are immutable and are never overwritten.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CRATES_IO_API = "https://crates.io/api/v1/crates"
USER_AGENT = "eggress-release/1.0"

# Bounded retry budget for registry throttling / transient failures.
MAX_PUBLISH_RETRIES = 5
VISIBILITY_ATTEMPTS = 12
VISIBILITY_INTERVAL_SECONDS = 10.0
BACKOFF_BASE_SECONDS = 30.0
BACKOFF_CAP_SECONDS = 300.0


class PublishError(RuntimeError):
    pass


def repo_root() -> Path:
    return REPO_ROOT


def run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, cwd=str(REPO_ROOT), **kwargs)


def load_metadata() -> dict:
    proc = run(
        ["cargo", "metadata", "--format-version", "1"],
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise PublishError(f"cargo metadata failed: {proc.stderr.strip()}")
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        raise PublishError(f"cargo metadata produced invalid JSON: {exc}") from exc


def workspace_version(metadata: dict) -> str:
    for pkg in metadata.get("packages", []):
        if pkg.get("name") == "eggress-bench":
            return pkg.get("version", "")
    # Fall back to the workspace root manifest.
    try:
        import tomllib
    except ModuleNotFoundError:  # Python 3.10 and older without tomli
        import tomli as tomllib  # type: ignore[no-redef]

    root = tomllib.loads((REPO_ROOT / "Cargo.toml").read_text())
    return root["workspace"]["package"]["version"]


def discover_packages(metadata: dict) -> dict[str, dict]:
    """Return publishable workspace packages keyed by name.

    Excludes ``publish = false`` packages (notably the ``eggress-bench``
    root package) and non-workspace packages.
    """
    workspace_members = set(metadata.get("workspace_members", []))
    packages: dict[str, dict] = {}
    for pkg in metadata.get("packages", []):
        name = pkg.get("name", "")
        if not name.startswith("eggress"):
            continue
        pkg_id = pkg.get("id", "")
        if workspace_members and pkg_id not in workspace_members:
            continue
        if pkg.get("publish") is False:
            continue
        # `publish` may also be a restricted list; treat an empty list as
        # unpublished. Anything else is publishable.
        publish = pkg.get("publish")
        if isinstance(publish, list) and not publish:
            continue
        packages[name] = {
            "version": pkg.get("version", ""),
            "manifest_path": pkg.get("manifest_path", ""),
            "id": pkg_id,
        }
    return packages


def internal_edges(metadata: dict, packages: dict[str, dict]) -> dict[str, set[str]]:
    """Map each publishable crate to its internal publishable prerequisites.

    Includes optional normal/build edges (Cargo resolves those against the
    index at package time even when a minimal build leaves the feature off).
    Excludes dev-dependency edges, which Cargo omits from the published
    dependency graph.
    """
    names = set(packages)
    edges: dict[str, set[str]] = {name: set() for name in names}
    by_id = {p.get("id", ""): p.get("name", "") for p in metadata.get("packages", [])}
    for pkg in metadata.get("packages", []):
        name = pkg.get("name", "")
        if name not in names:
            continue
        for dep in pkg.get("dependencies", []):
            dep_name = dep.get("name", "")
            if dep_name not in names or dep_name == name:
                continue
            kind = dep.get("kind")
            if kind == "dev":
                continue
            # `kind` is None for normal deps, "build" for build deps.
            if kind not in (None, "build"):
                continue
            # Only edges Cargo must resolve from the registry constrain
            # order. A path-only edge without a registry requirement (for
            # example a wildcard dev-style req that leaked into normal deps)
            # must fail closed rather than silently reorder.
            req = dep.get("req", "")
            if not req or req == "*":
                raise PublishError(
                    f"{name} has an unpinned internal dependency on {dep_name} "
                    f"(req={req!r}); expected an exact =version pin"
                )
            edges[name].add(dep_name)
    # Silence unused-variable lint for the lookup table kept for debugging.
    _ = by_id
    return edges


def topological_order(edges: dict[str, set[str]]) -> list[str]:
    """Deterministic Kahn topological sort; fails closed on cycles."""
    remaining = {name: set(deps) for name, deps in edges.items()}
    ordered: list[str] = []
    while remaining:
        ready = sorted(n for n, deps in remaining.items() if not deps)
        if not ready:
            cycle = sorted(remaining)
            raise PublishError(
                "dependency cycle or unresolved internal edge among: "
                + ", ".join(cycle)
            )
        for name in ready:
            ordered.append(name)
            del remaining[name]
        for deps in remaining.values():
            deps.difference_update(ready)
    return ordered


def validate_versions(
    metadata: dict, packages: dict[str, dict], expected: str
) -> None:
    mismatched = sorted(
        f"{name}={info['version']}"
        for name, info in packages.items()
        if info["version"] != expected
    )
    if mismatched:
        raise PublishError(
            f"package versions differ from workspace {expected}: "
            + ", ".join(mismatched)
        )
    for pkg in metadata.get("packages", []):
        name = pkg.get("name", "")
        if name not in packages:
            continue
        for dep in pkg.get("dependencies", []):
            dep_name = dep.get("name", "")
            if dep_name not in packages:
                continue
            if dep.get("kind") == "dev":
                continue
            if dep.get("kind") not in (None, "build"):
                continue
            req = dep.get("req", "")
            if req != f"={expected}":
                raise PublishError(
                    f"{name} pins {dep_name} to {req!r}, expected '={expected}'"
                )


def check_preflight() -> None:
    proc = run(
        ["./scripts/release-preflight.sh", "--check-versions-only"],
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise PublishError(
            "release preflight failed:\n" + (proc.stdout + proc.stderr).strip()
        )


def check_clean_tree() -> None:
    proc = run(["git", "status", "--porcelain"], capture_output=True, text=True)
    if proc.returncode != 0:
        raise PublishError("git status failed; cannot verify a clean tree")
    if proc.stdout.strip():
        raise PublishError(
            "working tree is not clean; commit or stash changes before releasing"
        )


def query_registry(
    crate: str,
    version: str,
    opener=None,
    timeout: float = 15.0,
) -> str:
    """Return 'present', 'missing', or 'transient' for a crate/version."""
    url = f"{CRATES_IO_API}/{crate}/{version}"
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    open_url = opener or urllib.request.urlopen
    try:
        with open_url(request, timeout=timeout) as resp:
            payload = resp.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            return "missing"
        if exc.code in (429, 500, 502, 503, 504):
            return "transient"
        return "transient"
    except (urllib.error.URLError, TimeoutError, OSError):
        return "transient"
    try:
        data = json.loads(payload)
    except json.JSONDecodeError:
        return "transient"
    if isinstance(data, dict) and data.get("version", {}).get("num") == version:
        return "present"
    # The version endpoint returns {"version": {...}} on success.
    if isinstance(data, dict) and "version" in data:
        return "present"
    return "transient"


def wait_for_visibility(
    crate: str,
    version: str,
    query=None,
    sleep=None,
    attempts: int = VISIBILITY_ATTEMPTS,
    interval: float = VISIBILITY_INTERVAL_SECONDS,
) -> bool:
    ask = query or query_registry
    doze = sleep or time.sleep
    for _ in range(attempts):
        if ask(crate, version) == "present":
            return True
        doze(interval)
    return ask(crate, version) == "present"


def parse_retry_after(output: str) -> float | None:
    match = re.search(r"[Rr]etry-[Aa]fter[:\s]+(\d+)", output)
    if match:
        try:
            return float(match.group(1))
        except ValueError:
            return None
    return None


def publish_command(crate: str) -> list[str]:
    cmd = ["cargo", "publish", "-p", crate, "--locked"]
    # Fail closed: these flags must never appear in the normal release path.
    assert "--no-verify" not in cmd and "--allow-dirty" not in cmd
    return cmd


def publish_one(
    crate: str,
    runner=None,
    sleep=None,
    query=None,
    version: str | None = None,
) -> str:
    """Publish one crate; return 'published', 'skipped-present', or raise."""
    do = runner or (lambda cmd: run(cmd, capture_output=True, text=True))
    doze = sleep or time.sleep
    ask = query or query_registry
    crate_version = version if version is not None else current_version(crate)

    packages_state = ask(crate, crate_version)
    if packages_state == "present":
        return "skipped-present"

    last_output = ""
    for attempt in range(1, MAX_PUBLISH_RETRIES + 1):
        # Re-check visibility first: a prior attempt may have succeeded
        # despite a transport error.
        if attempt > 1 and ask(crate, crate_version) == "present":
            return "published"
        proc = do(publish_command(crate))
        output = (getattr(proc, "stdout", "") or "") + (
            getattr(proc, "stderr", "") or ""
        )
        last_output = output
        if getattr(proc, "returncode", 1) == 0:
            if wait_for_visibility(
                crate, crate_version, query=ask, sleep=doze
            ):
                return "published"
            raise PublishError(
                f"{crate} publish succeeded but version did not become visible"
            )
        throttled = (
            "429" in output
            or "rate limit" in output.lower()
            or "try again" in output.lower()
        )
        server_error = any(
            code in output for code in ("500", "502", "503", "504")
        )
        manifest_error = (
            "error: failed to verify" in output.lower()
            or "package validation" in output.lower()
        )
        if manifest_error and not throttled:
            raise PublishError(f"{crate} package verification failed:\n{output}")
        if (throttled or server_error) and attempt < MAX_PUBLISH_RETRIES:
            delay = parse_retry_after(output)
            if delay is None:
                delay = min(BACKOFF_BASE_SECONDS * (2 ** (attempt - 1)), BACKOFF_CAP_SECONDS)
            doze(delay)
            continue
        raise PublishError(
            f"{crate} publish failed (attempt {attempt}):\n{output.strip()}"
        )
    raise PublishError(f"{crate} publish failed:\n{last_output.strip()}")


_VERSION_CACHE: dict[str, str] = {}


def current_version(crate: str) -> str:
    if crate in _VERSION_CACHE:
        return _VERSION_CACHE[crate]
    metadata = load_metadata()
    for pkg in metadata.get("packages", []):
        if pkg.get("name") == crate:
            _VERSION_CACHE[crate] = pkg.get("version", "")
            return _VERSION_CACHE[crate]
    raise PublishError(f"unknown workspace crate: {crate}")


def compute_plan() -> tuple[str, list[str], dict[str, set[str]], dict[str, dict]]:
    metadata = load_metadata()
    version = workspace_version(metadata)
    if not version:
        raise PublishError("could not determine workspace version")
    packages = discover_packages(metadata)
    if not packages:
        raise PublishError("no publishable eggress crates discovered")
    validate_versions(metadata, packages, version)
    edges = internal_edges(metadata, packages)
    order = topological_order(edges)
    return version, order, edges, packages


def cmd_list() -> int:
    version, order, _edges, _packages = compute_plan()
    print(f"workspace version: {version}")
    print(f"publishable crates: {len(order)}")
    for name in order:
        print(name)
    return 0


def cmd_dry_run(skip_package_verify: bool = False) -> int:
    check_preflight()
    # Dry-run intentionally does not require a clean tree: it is the
    # verification used *before* the release commit is finalized. The
    # execute path enforces cleanliness.
    version, order, edges, _packages = compute_plan()
    print(f"workspace version: {version}")
    print("computed publish order:")
    for name in order:
        prereqs = sorted(edges[name])
        suffix = f" (after {', '.join(prereqs)})" if prereqs else ""
        print(f"  {name}{suffix}")
    if not skip_package_verify:
        print("running workspace-wide package verification ...")
        proc = run(
            ["cargo", "package", "--workspace", "--exclude", "eggress-bench", "--locked"],
            capture_output=False,
        )
        if proc.returncode != 0:
            raise PublishError("workspace package verification failed")
    print("registry state (no uploads):")
    failed = False
    for name in order:
        state = query_registry(name, version)
        print(f"  {name} {version}: {state}")
        if state == "transient":
            print(
                f"warning: registry unavailable for {name}; "
                "rerun dry-run before executing",
                file=sys.stderr,
            )
            failed = True
    if failed:
        return 1
    print("dry-run OK: no uploads performed")
    return 0


def cmd_execute(tag: str | None = None) -> int:
    check_preflight()
    check_clean_tree()
    if tag is not None:
        proc = run(
            ["./scripts/release-preflight.sh", "--tag", tag],
            capture_output=True,
            text=True,
            check=False,
        )
        if proc.returncode != 0:
            raise PublishError(
                "tag preflight failed:\n" + (proc.stdout + proc.stderr).strip()
            )
    version, order, _edges, _packages = compute_plan()
    print(f"publishing workspace version {version} ({len(order)} crates)")
    for name in order:
        state = query_registry(name, version)
        if state == "present":
            print(f"skip {name} {version}: already published (resume)")
            continue
        if state == "transient":
            raise PublishError(
                f"registry unavailable for {name} {version}; "
                "rerun --execute later"
            )
        print(f"publishing {name} {version} ...")
        outcome = publish_one(name, version=version)
        print(f"{name}: {outcome}")
    print("publish run complete")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Graph-derived resumable crates.io publisher (manual)."
    )
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--list", action="store_true", help="print publish order")
    group.add_argument("--dry-run", action="store_true", help="verify without upload")
    group.add_argument("--execute", action="store_true", help="publish to crates.io")
    parser.add_argument("--tag", default=None, help="expected release tag (vX.Y.Z)")
    parser.add_argument(
        "--skip-package-verify",
        action="store_true",
        help="skip workspace package verification in dry-run (testing only)",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        if args.execute:
            return cmd_execute(tag=args.tag)
        if args.dry_run:
            return cmd_dry_run(skip_package_verify=args.skip_package_verify)
        return cmd_list()
    except PublishError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
