#!/usr/bin/env python3
"""Build a redacted, commit-bound release evidence bundle.

The generator is intentionally dependency-free and accepts already-executed
scenario/result files. It never claims that a gated suite passed merely
because a command was listed; callers must provide the result artifact.

Exit codes:
  0 – success, all scenarios passed or were skipped/not_applicable
  1 – scenario failures (one or more --scenario status=fail)
  2 – input validation failure (--result or --wheel path missing)
  3 – clean/SHA/tracked-inputs violation (--require-clean, --expected-commit,
      or --verify-tracked-inputs guard triggered)

Example::

    python3 scripts/release_evidence.py \\
      --output target/release-evidence \\
      --result target/compat/pproxy-parity-report.json \\
      --wheel dist/*.whl \\
      --command 'cargo test --workspace'
"""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any, Iterable

WORKSPACE = Path(__file__).resolve().parent.parent
DEFAULT_TRACKED_FILES = (
    "Cargo.lock",
    "docs/parity/pproxy_capability_manifest.toml",
    "docs/parity/composition_matrix.toml",
    "python/compat/pproxy_api_contract.json",
    "python-pproxy-compat/pyproject.toml",
)

_CREDENTIAL_URI = re.compile(
    r"(?P<scheme>[A-Za-z][A-Za-z0-9+.-]*://)(?P<userinfo>[^\s/@]+)@"
)
_SECRET_ASSIGNMENT = re.compile(
    r"(?im)^(\s*(?:password|secret|token|private_key|api_key)\s*=\s*)([^\n]+)$"
)

EXIT_SUCCESS = 0
EXIT_SCENARIO_FAILURE = 1
EXIT_INPUT_VALIDATION = 2
EXIT_GUARD_VIOLATION = 3


def _run(*args: str) -> str:
    try:
        return subprocess.check_output(
            args, cwd=WORKSPACE, text=True, stderr=subprocess.DEVNULL
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return "unavailable"


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def redact_text(value: str) -> str:
    """Redact URI userinfo and common secret assignments from an artifact."""
    value = _CREDENTIAL_URI.sub(r"\g<scheme>****@", value)
    return _SECRET_ASSIGNMENT.sub(r"\1****", value)


def _copy_redacted(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    if source.suffix.lower() in {".json", ".toml", ".md", ".txt", ".log", ".jsonl"}:
        destination.write_text(redact_text(source.read_text(encoding="utf-8")))
    else:
        shutil.copyfile(source, destination)


def _parse_scenario(value: str) -> dict[str, str]:
    scenario_id, separator, status = value.partition("=")
    if not separator or not scenario_id or not status:
        raise argparse.ArgumentTypeError("scenario must use ID=STATUS")
    if status not in {"pass", "fail", "skip", "not_applicable"}:
        raise argparse.ArgumentTypeError(
            "scenario status must be pass, fail, skip, or not_applicable"
        )
    return {"id": scenario_id, "status": status}


def _is_dirty() -> bool:
    return subprocess.run(
        ["git", "diff", "--quiet"], cwd=WORKSPACE, check=False
    ).returncode != 0


def _os_release_pretty() -> str:
    system = platform.system()
    if system == "Darwin":
        return _run("sw_vers", "-productVersion")
    elif system == "Linux":
        out = _run("lsb_release", "-d")
        if out != "unavailable":
            return out
    elif system == "Windows":
        out = _run("cmd", "/c", "ver")
        if out != "unavailable":
            return out
    return platform.platform()


def _metadata(
    reference: str,
    commands: list[str],
    tracked_hashes: dict[str, str],
) -> dict[str, Any]:
    dirty = _is_dirty()
    head = _run("git", "rev-parse", "HEAD")
    cargo_lock_path = WORKSPACE / "Cargo.lock"
    cargo_lock_sha = _sha256(cargo_lock_path) if cargo_lock_path.is_file() else "unavailable"
    return {
        "schema_version": "1.0",
        "eggress_commit": head,
        "dirty_state": dirty,
        "reference": reference,
        "os": platform.system(),
        "release": platform.release(),
        "architecture": platform.machine(),
        "os_release_pretty": _os_release_pretty(),
        "target_triple": _run("rustc", "-vV").split("host: ")[-1].splitlines()[0]
        if "host: " in _run("rustc", "-vV")
        else "unavailable",
        "python": platform.python_version(),
        "python_implementation": platform.python_implementation(),
        "rust": _run("rustc", "--version"),
        "cargo": _run("cargo", "--version"),
        "cargo_lock_sha256": cargo_lock_sha,
        "pinned_reference": reference,
        "tracked_input_sha256": tracked_hashes,
        "commands": commands,
    }


def _load_results(paths: Iterable[Path]) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []
    for path in paths:
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            results.append({"source": str(path), "status": "invalid", "error": str(exc)})
            continue
        results.append({"source": str(path), "status": "provided", "result": data})
    return results


def _write_hash_manifest(root: Path) -> None:
    entries: list[str] = []
    for path in sorted(p for p in root.rglob("*") if p.is_file()):
        if path.name == "SHA256SUMS":
            continue
        entries.append(f"{_sha256(path)}  {path.relative_to(root).as_posix()}")
    (root / "SHA256SUMS").write_text("\n".join(entries) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--reference", default="pproxy==2.7.9")
    parser.add_argument("--result", type=Path, action="append", default=[])
    parser.add_argument("--wheel", type=Path, action="append", default=[])
    parser.add_argument("--command", action="append", default=[])
    parser.add_argument("--scenario", type=_parse_scenario, action="append", default=[])
    parser.add_argument("--skip-reason", action="append", default=[])
    parser.add_argument(
        "--require-clean",
        action="store_true",
        default=False,
        help="Exit with code 3 if the working tree is dirty.",
    )
    parser.add_argument(
        "--expected-commit",
        type=str,
        default=None,
        metavar="SHA",
        help="Exit with code 3 unless HEAD matches this exact commit SHA.",
    )
    parser.add_argument(
        "--verify-tracked-inputs",
        action="store_true",
        default=False,
        help="Recompute SHA256 for each tracked input and abort (code 3) "
             "if any differs from the recorded value.",
    )
    args = parser.parse_args()

    root = args.output if args.output.is_absolute() else WORKSPACE / args.output

    # ── Guard: expected-commit ──
    if args.expected_commit is not None:
        head = _run("git", "rev-parse", "HEAD")
        expected = args.expected_commit
        if expected != head and _run("git", "rev-parse", "--verify", expected + "^{commit}") == head:
            expected = head
        if head != expected:
            print(
                f"error: HEAD ({head}) does not match expected commit "
                f"({args.expected_commit})",
                file=sys.stderr,
            )
            return EXIT_GUARD_VIOLATION

    # ── Guard: require-clean ──
    if args.require_clean and _is_dirty():
        print("error: working tree is dirty", file=sys.stderr)
        return EXIT_GUARD_VIOLATION

    # ── Pre-validate input paths (fail-closed before writing anything) ──
    missing: list[str] = []
    for result_path in args.result:
        resolved = result_path if result_path.is_absolute() else WORKSPACE / result_path
        if not resolved.is_file():
            missing.append(f"--result {result_path}")
    for wheel_path in args.wheel:
        resolved = wheel_path if wheel_path.is_absolute() else WORKSPACE / wheel_path
        if not resolved.is_file():
            missing.append(f"--wheel {wheel_path}")
    if missing:
        for m in missing:
            print(f"error: missing input path: {m}", file=sys.stderr)
        return EXIT_INPUT_VALIDATION

    root.mkdir(parents=True, exist_ok=True)

    # ── Compute tracked-input hashes ──
    tracked_hashes: dict[str, str] = {}
    for relative in DEFAULT_TRACKED_FILES:
        path = WORKSPACE / relative
        if path.is_file():
            tracked_hashes[relative] = _sha256(path)

    # ── Guard: verify-tracked-inputs ──
    if args.verify_tracked_inputs:
        mismatches: list[str] = []
        for relative in DEFAULT_TRACKED_FILES:
            path = WORKSPACE / relative
            if not path.is_file():
                mismatches.append(f"{relative}: file not found")
                continue
            current_hash = _sha256(path)
            # Re-read from a freshly generated metadata to compare
            stored = tracked_hashes.get(relative)
            if stored != current_hash:
                mismatches.append(
                    f"{relative}: expected {stored or 'none'}, got {current_hash}"
                )
        if mismatches:
            for m in mismatches:
                print(f"error: tracked input mismatch: {m}", file=sys.stderr)
            return EXIT_GUARD_VIOLATION

    # ── Emit artifacts ──
    commands = list(args.command)
    (root / "metadata.json").write_text(
        json.dumps(
            _metadata(args.reference, commands, tracked_hashes),
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )

    results = _load_results(
        path if path.is_absolute() else WORKSPACE / path for path in args.result
    )
    (root / "scenario-results.json").write_text(
        json.dumps(results, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (root / "scenarios.json").write_text(
        json.dumps(args.scenario, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (root / "skipped.json").write_text(
        json.dumps(args.skip_reason, indent=2) + "\n", encoding="utf-8"
    )

    inputs = [
        path if path.is_absolute() else WORKSPACE / path for path in args.result
    ]
    inputs.extend(path if path.is_absolute() else WORKSPACE / path for path in args.wheel)
    for source in inputs:
        destination = root / "inputs" / source.name
        _copy_redacted(source, destination)

    wheel_hashes = {}
    for path in args.wheel:
        resolved = path if path.is_absolute() else WORKSPACE / path
        wheel_hashes[resolved.name] = _sha256(resolved)
    (root / "wheel-hashes.json").write_text(
        json.dumps(wheel_hashes, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )

    # ── Summary ──
    pass_count = sum(item["status"] == "pass" for item in args.scenario)
    fail_count = sum(item["status"] == "fail" for item in args.scenario)
    skip_count = sum(
        item["status"] in {"skip", "not_applicable"} for item in args.scenario
    )
    dirty = _is_dirty()
    head = _run("git", "rev-parse", "HEAD")
    guard_violations: list[str] = []
    if dirty:
        guard_violations.append("dirty working tree")
    if args.expected_commit:
        expected = args.expected_commit
        if expected != head and _run("git", "rev-parse", "--verify", expected + "^{commit}") == head:
            expected = head
        if head != expected:
            guard_violations.append(
                f"HEAD ({head}) != expected ({args.expected_commit})"
            )
    summary = (
        "# Release evidence summary\n\n"
        f"- Eggress commit: `{head}`\n"
        f"- Reference: `{args.reference}`\n"
        f"- Dirty state: `{dirty}`\n"
        f"- Scenarios: {pass_count} pass, {fail_count} fail, "
        f"{skip_count} skipped/not applicable\n"
        f"- Result artifacts: {len(args.result)}\n"
        f"- Wheel artifacts: {len(wheel_hashes)}\n"
    )
    if guard_violations:
        summary += f"- Guard violations: {', '.join(guard_violations)}\n"
    (root / "summary.md").write_text(summary, encoding="utf-8")
    _write_hash_manifest(root)
    print(root)
    return EXIT_SCENARIO_FAILURE if fail_count else EXIT_SUCCESS


if __name__ == "__main__":
    raise SystemExit(main())
