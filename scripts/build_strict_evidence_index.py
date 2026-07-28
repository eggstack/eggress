#!/usr/bin/env python3
"""Build and validate the strict differential evidence index.

Reads the producer mapping TOML and validates that:
1. Every strict test file has required observation producers
2. Every observation RID has at least one producer
3. Every producer has a valid probe script
4. No observation is produced by an unknown script

Usage:
    python3 scripts/build_strict_evidence_index.py --validate
    python3 scripts/build_strict_evidence_index.py --list-missing
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

try:
    import tomllib
except ImportError:
    try:
        import tomli as tomllib
    except ImportError:
        print("ERROR: tomllib/tomli not available", file=sys.stderr)
        sys.exit(1)


KNOWN_PROBES = {
    "strict_api_probe.py",
    "strict_signature_probe.py",
    "strict_class_probe.py",
    "strict_cipher_kat_probe.py",
    "strict_cipher_roundtrip_probe.py",
    "strict_protocol_wire_probe.py",
    "strict_process_lifecycle_probe.py",
    "strict_cipher_interop_probe.py",
    "strict_handler_relay_probe.py",
    "strict_stream_adapter_probe.py",
    "strict_server_internals_probe.py",
    "strict_runtime_failure_cleanup_probe.py",
    "strict_plugin_lifecycle_probe.py",
}

STRICT_TEST_DIR = Path("python/tests/strict")
MAPPING_FILE = Path("docs/parity/strict_evidence_producer_mapping.toml")
EVIDENCE_DIR = Path("target/closure-audit/evidence")


def load_mapping(path: Path) -> dict:
    """Load the producer mapping TOML."""
    with open(path, "rb") as fh:
        return tomllib.load(fh)


def discover_strict_tests() -> list[str]:
    """Discover all test files in the strict test directory."""
    tests = []
    if STRICT_TEST_DIR.exists():
        for f in sorted(STRICT_TEST_DIR.glob("test_*.py")):
            tests.append(str(f))
    return tests


def validate_mapping(mapping: dict) -> list[str]:
    """Validate the producer mapping. Returns list of errors."""
    errors = []
    producers = mapping.get("producer", [])

    if not producers:
        errors.append("No producers defined in mapping")
        return errors

    known_rids = set()
    known_tests = set()

    for prod in producers:
        rid = prod.get("rid", "")
        probe = prod.get("probe", "")
        test_files = prod.get("test_files", [])

        if not rid:
            errors.append("Producer missing 'rid'")
            continue
        known_rids.add(rid)

        if probe not in KNOWN_PROBES:
            errors.append(f"RID '{rid}' uses unknown probe '{probe}'")

        if not test_files:
            errors.append(f"RID '{rid}' has no test_files")

        for tf in test_files:
            known_tests.add(tf)
            if not Path(tf).exists():
                errors.append(f"RID '{rid}' references missing test file: {tf}")

    # Check for unlisted strict tests
    actual_tests = set(discover_strict_tests())
    unlisted = actual_tests - known_tests
    if unlisted:
        for t in sorted(unlisted):
            errors.append(f"Strict test file not in mapping: {t}")

    return errors


def check_observations(mapping: dict, obs_dir: Path) -> list[str]:
    """Check which observations exist for a given directory."""
    missing = []
    producers = mapping.get("producer", [])

    for prod in producers:
        rid = prod.get("rid", "")
        filename = f"{rid.replace('.', '_')}_oracle.json"
        filepath = obs_dir / filename
        if not filepath.exists():
            missing.append(rid)

    return missing


def build_index(mapping: dict, output: Path) -> dict:
    """Build the evidence index from the mapping."""
    index = {
        "version": 1,
        "producers": [],
    }

    for prod in mapping.get("producer", []):
        index["producers"].append({
            "rid": prod["rid"],
            "probe": prod["probe"],
            "module": prod.get("module", ""),
            "symbol": prod.get("symbol", ""),
            "test_files": prod.get("test_files", []),
        })

    output.parent.mkdir(parents=True, exist_ok=True)
    with open(output, "w") as fh:
        json.dump(index, fh, indent=2)
        fh.write("\n")

    return index


def main():
    parser = argparse.ArgumentParser(description="Build strict evidence index")
    parser.add_argument("--validate", action="store_true",
                        help="Validate the producer mapping")
    parser.add_argument("--list-missing", action="store_true",
                        help="List missing observation files")
    parser.add_argument("--build", action="store_true",
                        help="Build evidence index JSON")
    parser.add_argument("--obs-dir", type=Path,
                        default=Path("target/closure-audit/paired_observations/oracle"),
                        help="Observation directory to check")
    parser.add_argument("--output", type=Path,
                        default=EVIDENCE_DIR / "strict_evidence_index.json",
                        help="Output index file path")
    args = parser.parse_args()

    if not any([args.validate, args.list_missing, args.build]):
        args.validate = True
        args.list_missing = True
        args.build = True

    mapping = load_mapping(MAPPING_FILE)
    errors = []

    if args.validate:
        errors = validate_mapping(mapping)
        if errors:
            print("VALIDATION ERRORS:")
            for e in errors:
                print(f"  - {e}")
        else:
            print("Producer mapping: VALID")

    if args.list_missing:
        missing = check_observations(mapping, args.obs_dir)
        if missing:
            print(f"\nMissing observations in {args.obs_dir}:")
            for rid in sorted(missing):
                print(f"  - {rid}")
        else:
            print(f"\nAll observations present in {args.obs_dir}")

    if args.build:
        index = build_index(mapping, args.output)
        print(f"\nEvidence index built: {args.output}")
        print(f"  Producers: {len(index['producers'])}")

    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
