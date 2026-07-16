#!/usr/bin/env python3
"""Build a redacted, commit-bound release evidence bundle.

The generator is intentionally dependency-free and accepts already-executed
scenario/result files. It never claims that a gated suite passed merely
because a command was listed; callers must provide the result artifact.

Example::

    python3 scripts/release_evidence.py \
      --output target/release-evidence \
      --result target/compat/pproxy-parity-report.json \
      --wheel dist/*.whl \
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


def _metadata(reference: str, commands: list[str]) -> dict[str, Any]:
    dirty = subprocess.run(
        ["git", "diff", "--quiet"], cwd=WORKSPACE, check=False
    ).returncode != 0
    tracked_hashes: dict[str, str] = {}
    for relative in DEFAULT_TRACKED_FILES:
        path = WORKSPACE / relative
        if path.is_file():
            tracked_hashes[relative] = _sha256(path)
    return {
        "schema_version": "1.0",
        "eggress_commit": _run("git", "rev-parse", "HEAD"),
        "dirty_state": dirty,
        "reference": reference,
        "os": platform.system(),
        "release": platform.release(),
        "architecture": platform.machine(),
        "target_triple": _run("rustc", "-vV").split("host: ")[-1].splitlines()[0]
        if "host: " in _run("rustc", "-vV")
        else "unavailable",
        "python": platform.python_version(),
        "python_implementation": platform.python_implementation(),
        "rust": _run("rustc", "--version"),
        "cargo": _run("cargo", "--version"),
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
    args = parser.parse_args()

    root = args.output if args.output.is_absolute() else WORKSPACE / args.output
    root.mkdir(parents=True, exist_ok=True)

    commands = list(args.command)
    (root / "metadata.json").write_text(
        json.dumps(_metadata(args.reference, commands), indent=2, sort_keys=True) + "\n",
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
        if not source.is_file():
            continue
        destination = root / "inputs" / source.name
        _copy_redacted(source, destination)

    wheel_hashes = {}
    for path in args.wheel:
        resolved = path if path.is_absolute() else WORKSPACE / path
        if resolved.is_file():
            wheel_hashes[resolved.name] = _sha256(resolved)
    (root / "wheel-hashes.json").write_text(
        json.dumps(wheel_hashes, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )

    pass_count = sum(item["status"] == "pass" for item in args.scenario)
    fail_count = sum(item["status"] == "fail" for item in args.scenario)
    skip_count = sum(item["status"] in {"skip", "not_applicable"} for item in args.scenario)
    summary = (
        "# Release evidence summary\n\n"
        f"- Eggress commit: `{_run('git', 'rev-parse', 'HEAD')}`\n"
        f"- Reference: `{args.reference}`\n"
        f"- Scenarios: {pass_count} pass, {fail_count} fail, {skip_count} skipped/not applicable\n"
        f"- Result artifacts: {len(args.result)}\n"
        f"- Wheel artifacts: {len(wheel_hashes)}\n"
    )
    (root / "summary.md").write_text(summary, encoding="utf-8")
    _write_hash_manifest(root)
    print(root)
    return 1 if fail_count else 0


if __name__ == "__main__":
    raise SystemExit(main())
