#!/usr/bin/env python3
"""Paired oracle/candidate API comparison runner.

Reads the strict manifest, extracts records with module/symbol pairs,
runs probes in isolated oracle and candidate venvs, and compares results.

Usage:
    # Full paired run
    python3 scripts/run_strict_pproxy_api.py --oracle-venv .venv-oracle --candidate-venv .venv-candidate

    # Dry run (list records, no execution)
    python3 scripts/run_strict_pproxy_api.py --dry-run

    # Filter by category
    python3 scripts/run_strict_pproxy_api.py --category python_namespace

    # Filter by implementation_state
    python3 scripts/run_strict_pproxy_api.py --implementation-state structural

Exit codes:
    0 - All comparisons passed (or dry run)
    1 - At least one mismatch
    2 - Harness error
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import venv
from pathlib import Path
from typing import Optional


MANIFEST_PATH = Path("docs/parity/pproxy_2_7_9_strict_manifest.toml")
SCRIPTS_DIR = Path("scripts")


def parse_manifest(path: Path) -> list[dict]:
    """Parse the strict manifest TOML into a list of record dicts."""
    content = path.read_text()
    raw_records = content.split("[[record]]")[1:]
    records = []
    for raw in raw_records:
        data = {}
        for line in raw.strip().split("\n"):
            line = line.strip()
            if line.startswith("#") or not line:
                continue
            m = re.match(r'(\w+)\s*=\s*"([^"]*)"', line)
            if m:
                data[m.group(1)] = m.group(2)
            m_list = re.match(r'(\w+)\s*=\s*\[([^\]]*)\]', line)
            if m_list:
                vals = re.findall(r'"([^"]*)"', m_list.group(2))
                data[m_list.group(1)] = vals
        if "id" in data:
            records.append(data)
    return records


def extract_probe_args(record: dict) -> Optional[tuple[str, str]]:
    """Extract (module, symbol) from a manifest record for probing."""
    module = record.get("module", "")
    name = record.get("name", "")
    kind = record.get("kind", "")

    if kind == "module":
        # For module existence checks, probe the parent module for the submodule
        if module:
            return (module, name)
        else:
            # Top-level module (e.g., pproxy)
            return (name, name)

    if kind in ("class", "function", "constant"):
        if module and name:
            return (module, name)

    return None


def probe_in_venv(
    venv_python: str,
    probe_script: str,
    module: str,
    symbol: str,
) -> dict:
    """Run a probe script in a venv and return the observation."""
    cmd = [venv_python, str(SCRIPTS_DIR / probe_script), "--module", module, "--symbol", symbol]
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=30,
            cwd=str(Path.cwd()),
        )
        if result.returncode == 0 and result.stdout.strip():
            return json.loads(result.stdout)
        else:
            return {
                "module": module,
                "symbol": symbol,
                "exists": False,
                "error": f"Probe failed: {result.stderr.strip() or 'no output'}",
            }
    except subprocess.TimeoutExpired:
        return {
            "module": module,
            "symbol": symbol,
            "exists": False,
            "error": "Probe timed out",
        }
    except json.JSONDecodeError as e:
        return {
            "module": module,
            "symbol": symbol,
            "exists": False,
            "error": f"Invalid JSON from probe: {e}",
        }


def compare_observations(oracle: dict, candidate: dict) -> dict:
    """Compare two observations and return a comparison report."""
    comparisons = []

    # Existence
    o_exists = oracle.get("exists", False)
    c_exists = candidate.get("exists", False)
    comparisons.append({
        "dimension": "exists",
        "oracle": o_exists,
        "candidate": c_exists,
        "match": o_exists == c_exists,
    })

    if not o_exists and not c_exists:
        return {
            "all_match": True,
            "total_dimensions": 1,
            "match_count": 1,
            "mismatch_count": 0,
            "comparisons": comparisons,
        }

    # Type
    o_type = oracle.get("type")
    c_type = candidate.get("type")
    comparisons.append({
        "dimension": "type",
        "oracle": o_type,
        "candidate": c_type,
        "match": o_type == c_type,
    })

    # Qualname
    o_qualname = oracle.get("qualname", "")
    c_qualname = candidate.get("qualname", "")
    o_local = o_qualname.rsplit(".", 1)[-1] if o_qualname else ""
    c_local = c_qualname.rsplit(".", 1)[-1] if c_qualname else ""
    comparisons.append({
        "dimension": "qualname_local",
        "oracle": o_local,
        "candidate": c_local,
        "match": o_local == c_local,
    })

    # Is coroutine
    o_coro = oracle.get("is_coroutine")
    c_coro = candidate.get("is_coroutine")
    comparisons.append({
        "dimension": "is_coroutine",
        "oracle": o_coro,
        "candidate": c_coro,
        "match": o_coro == c_coro,
    })

    # Is callable
    o_callable = oracle.get("is_callable", False)
    c_callable = candidate.get("is_callable", False)
    comparisons.append({
        "dimension": "is_callable",
        "oracle": o_callable,
        "candidate": c_callable,
        "match": o_callable == c_callable,
    })

    # Signature
    o_sig = oracle.get("signature", "")
    c_sig = candidate.get("signature", "")
    comparisons.append({
        "dimension": "signature",
        "oracle": o_sig,
        "candidate": c_sig,
        "match": o_sig == c_sig,
    })

    all_match = all(c["match"] for c in comparisons)
    mismatches = [c for c in comparisons if not c["match"]]

    return {
        "all_match": all_match,
        "total_dimensions": len(comparisons),
        "match_count": sum(1 for c in comparisons if c["match"]),
        "mismatch_count": len(mismatches),
        "comparisons": comparisons,
    }


def setup_venv(venv_dir: Path, install_pproxy: bool = False, install_eggress: bool = False) -> str:
    """Create a clean venv and optionally install packages. Returns python path."""
    if venv_dir.exists():
        import shutil
        shutil.rmtree(venv_dir)

    venv.create(venv_dir, with_pip=True)
    python = str(venv_dir / "bin" / "python")

    # Upgrade pip
    subprocess.run([python, "-m", "pip", "install", "--upgrade", "pip"], capture_output=True)

    if install_pproxy:
        subprocess.run(
            [python, "-m", "pip", "install", "pproxy==2.7.9"],
            capture_output=True,
        )

    if install_eggress:
        # Build and install the eggress wheel + compat wheel
        subprocess.run(
            [python, "-m", "pip", "install", "maturin"],
            capture_output=True,
        )
        subprocess.run(
            ["maturin", "build", "--release", "--out", "target/wheels"],
            capture_output=True,
        )
        # Find and install the eggress wheel
        wheels = list(Path("target/wheels").glob("eggress-*.whl"))
        if wheels:
            subprocess.run(
                [python, "-m", "pip", "install", str(wheels[0])],
                capture_output=True,
            )
        # Install compat wheel
        subprocess.run(
            [python, "-m", "pip", "wheel", "--no-deps", "--wheel-dir", "target/wheels", "./python-pproxy-compat"],
            capture_output=True,
        )
        compat_wheels = list(Path("target/wheels").glob("eggress_pproxy_compat-*.whl"))
        if compat_wheels:
            subprocess.run(
                [python, "-m", "pip", "install", str(compat_wheels[0])],
                capture_output=True,
            )

    return python


def run_paired_comparison(
    records: list[dict],
    oracle_python: str,
    candidate_python: str,
    probe_type: str = "api",
    output_dir: Optional[Path] = None,
) -> dict:
    """Run paired comparison for a list of records."""
    results = []
    total = len(records)
    passed = 0
    failed = 0
    errors = 0

    probe_map = {
        "api": "strict_api_probe.py",
        "signature": "strict_signature_probe.py",
        "class": "strict_class_probe.py",
    }
    probe_script = probe_map.get(probe_type, "strict_api_probe.py")

    for i, record in enumerate(records, 1):
        rid = record.get("id", "?")
        args = extract_probe_args(record)
        if not args:
            results.append({
                "id": rid,
                "status": "skipped",
                "reason": "Cannot extract module/symbol from record",
            })
            continue

        module, symbol = args
        print(f"  [{i}/{total}] {rid} ({module}.{symbol})", file=sys.stderr)

        # Run oracle probe
        oracle_obs = probe_in_venv(oracle_python, probe_script, module, symbol)

        # Run candidate probe
        candidate_obs = probe_in_venv(candidate_python, probe_script, module, symbol)

        # Compare
        comparison = compare_observations(oracle_obs, candidate_obs)

        # Save observations if output dir specified
        if output_dir:
            output_dir.mkdir(parents=True, exist_ok=True)
            oracle_file = output_dir / f"{rid.replace('.', '_')}_oracle.json"
            candidate_file = output_dir / f"{rid.replace('.', '_')}_candidate.json"
            comparison_file = output_dir / f"{rid.replace('.', '_')}_comparison.json"
            oracle_file.write_text(json.dumps(oracle_obs, indent=2, default=str))
            candidate_file.write_text(json.dumps(candidate_obs, indent=2, default=str))
            comparison_file.write_text(json.dumps(comparison, indent=2, default=str))

        result = {
            "id": rid,
            "module": module,
            "symbol": symbol,
            "all_match": comparison["all_match"],
            "mismatch_count": comparison["mismatch_count"],
            "comparisons": comparison["comparisons"],
            "oracle_exists": oracle_obs.get("exists", False),
            "candidate_exists": candidate_obs.get("exists", False),
        }

        if oracle_obs.get("error"):
            result["oracle_error"] = oracle_obs["error"]
        if candidate_obs.get("error"):
            result["candidate_error"] = candidate_obs["error"]

        if comparison["all_match"]:
            result["status"] = "pass"
            passed += 1
        else:
            result["status"] = "fail"
            failed += 1

        results.append(result)

    return {
        "total": total,
        "passed": passed,
        "failed": failed,
        "skipped": total - passed - failed,
        "results": results,
    }


def main():
    parser = argparse.ArgumentParser(description="Paired oracle/candidate API comparison runner")
    parser.add_argument("--manifest", default=str(MANIFEST_PATH), help="Path to strict manifest")
    parser.add_argument("--oracle-venv", help="Path to oracle venv (pproxy 2.7.9)")
    parser.add_argument("--candidate-venv", help="Path to candidate venv (eggress + compat)")
    parser.add_argument("--category", help="Filter by category")
    parser.add_argument("--implementation-state", help="Filter by implementation_state")
    parser.add_argument("--status", help="Filter by status")
    parser.add_argument("--dry-run", action="store_true", help="List records without executing")
    parser.add_argument("--output-dir", help="Directory for observation JSON files")
    parser.add_argument("--create-venvs", action="store_true", help="Create clean venvs automatically")
    args = parser.parse_args()

    # Parse manifest
    manifest_path = Path(args.manifest)
    if not manifest_path.exists():
        print(f"Manifest not found: {manifest_path}", file=sys.stderr)
        sys.exit(2)

    records = parse_manifest(manifest_path)
    print(f"Parsed {len(records)} records from manifest", file=sys.stderr)

    # Apply filters
    if args.category:
        records = [r for r in records if r.get("category") == args.category]
        print(f"After category filter ({args.category}): {len(records)} records", file=sys.stderr)

    if args.implementation_state:
        records = [r for r in records if r.get("implementation_state") == args.implementation_state]
        print(f"After implementation_state filter ({args.implementation_state}): {len(records)} records", file=sys.stderr)

    if args.status:
        records = [r for r in records if r.get("status") == args.status]
        print(f"After status filter ({args.status}): {len(records)} records", file=sys.stderr)

    # Only test records that have extractable module/symbol
    testable = [r for r in records if extract_probe_args(r) is not None]
    print(f"Testable records: {len(testable)}", file=sys.stderr)

    if args.dry_run:
        print("\nDry run — records that would be tested:")
        for r in testable:
            args_tuple = extract_probe_args(r)
            module, symbol = args_tuple
            print(f"  {r['id']}  ->  {module}.{symbol}  ({r.get('comparator', '?')})")
        return

    # Setup venvs
    if args.create_venvs:
        oracle_dir = Path(".venv-oracle-api")
        candidate_dir = Path(".venv-candidate-api")
        print("Setting up oracle venv...", file=sys.stderr)
        oracle_python = setup_venv(oracle_dir, install_pproxy=True)
        print("Setting up candidate venv...", file=sys.stderr)
        candidate_python = setup_venv(candidate_dir, install_eggress=True)
    else:
        if not args.oracle_venv or not args.candidate_venv:
            print("Error: --oracle-venv and --candidate-venv required (or use --create-venvs)", file=sys.stderr)
            sys.exit(2)
        oracle_python = str(Path(args.oracle_venv) / "bin" / "python")
        candidate_python = str(Path(args.candidate_venv) / "bin" / "python")

    # Verify venvs exist
    for label, python_path in [("oracle", oracle_python), ("candidate", candidate_python)]:
        if not Path(python_path).exists():
            print(f"Error: {label} python not found: {python_path}", file=sys.stderr)
            sys.exit(2)

    # Run comparison
    output_dir = Path(args.output_dir) if args.output_dir else Path("target/strict/paired_observations")
    print(f"\nRunning paired comparison for {len(testable)} records...", file=sys.stderr)

    report = run_paired_comparison(testable, oracle_python, candidate_python, output_dir=output_dir)

    # Print summary
    print(f"\n{'='*60}")
    print(f"Paired API Comparison Results")
    print(f"{'='*60}")
    print(f"Total:   {report['total']}")
    print(f"Passed:  {report['passed']}")
    print(f"Failed:  {report['failed']}")
    print(f"Skipped: {report['skipped']}")
    print(f"{'='*60}")

    if report["failed"] > 0:
        print("\nFailed records:")
        for r in report["results"]:
            if r["status"] == "fail":
                print(f"  {r['id']}:")
                for comp in r["comparisons"]:
                    if not comp["match"]:
                        print(f"    {comp['dimension']}: oracle={comp['oracle']}, candidate={comp['candidate']}")

    # Write report
    report_file = output_dir / "paired_api_report.json"
    output_dir.mkdir(parents=True, exist_ok=True)
    report_file.write_text(json.dumps(report, indent=2, default=str))
    print(f"\nReport written to: {report_file}", file=sys.stderr)

    sys.exit(0 if report["failed"] == 0 else 1)


if __name__ == "__main__":
    main()
