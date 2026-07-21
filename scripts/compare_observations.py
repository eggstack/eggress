#!/usr/bin/env python3
"""Compare two JSON observation files (oracle and candidate) and emit a report.

Usage:
    python3 scripts/compare_observations.py --oracle obs_oracle.json --candidate obs_candidate.json

Exit codes:
    0 - All compared dimensions match
    1 - At least one mismatch found
    2 - Harness error
"""

import argparse
import json
import sys
import traceback


def _compare_api_obs(oracle: dict, candidate: dict) -> list:
    """Compare two strict_api_probe observations. Returns list of comparison results."""
    results = []
    symbol_key = f"{oracle.get('module', '?')}.{oracle.get('symbol', '?')}"

    # Existence
    o_exists = oracle.get("exists", False)
    c_exists = candidate.get("exists", False)
    results.append({
        "dimension": "exists",
        "oracle": o_exists,
        "candidate": c_exists,
        "match": o_exists == c_exists,
    })

    if not o_exists and not c_exists:
        return results

    # Type
    o_type = oracle.get("type")
    c_type = candidate.get("type")
    results.append({
        "dimension": "type",
        "oracle": o_type,
        "candidate": c_type,
        "match": o_type == c_type,
    })

    # Qualname - compare just the last component (local name)
    o_qualname = oracle.get("qualname", "")
    c_qualname = candidate.get("qualname", "")
    o_local = o_qualname.rsplit(".", 1)[-1] if o_qualname else ""
    c_local = c_qualname.rsplit(".", 1)[-1] if c_qualname else ""
    results.append({
        "dimension": "qualname_local",
        "oracle": o_local,
        "candidate": c_local,
        "match": o_local == c_local,
    })

    # Is coroutine
    o_coro = oracle.get("is_coroutine")
    c_coro = candidate.get("is_coroutine")
    results.append({
        "dimension": "is_coroutine",
        "oracle": o_coro,
        "candidate": c_coro,
        "match": o_coro == c_coro,
    })

    # Is callable
    o_callable = oracle.get("is_callable", False)
    c_callable = candidate.get("is_callable", False)
    results.append({
        "dimension": "is_callable",
        "oracle": o_callable,
        "candidate": c_callable,
        "match": o_callable == c_callable,
    })

    # Signature - compare parameter names and kinds only (not defaults)
    o_sig = oracle.get("signature", "")
    c_sig = candidate.get("signature", "")
    results.append({
        "dimension": "signature",
        "oracle": o_sig,
        "candidate": c_sig,
        "match": o_sig == c_sig,
    })

    return results


def _compare_signature_obs(oracle: dict, candidate: dict) -> list:
    """Compare two strict_signature_probe observations. Returns list of comparison results."""
    results = []
    symbol_key = f"{oracle.get('module', '?')}.{oracle.get('symbol', '?')}"

    # Parameter names and kinds (not defaults)
    o_params = oracle.get("parameters", [])
    c_params = candidate.get("parameters", [])

    o_param_names = [p.get("name") for p in o_params]
    c_param_names = [p.get("name") for p in c_params]
    results.append({
        "dimension": "parameter_names",
        "oracle": o_param_names,
        "candidate": c_param_names,
        "match": o_param_names == c_param_names,
    })

    o_param_kinds = [p.get("kind") for p in o_params]
    c_param_kinds = [p.get("kind") for p in c_params]
    results.append({
        "dimension": "parameter_kinds",
        "oracle": o_param_kinds,
        "candidate": c_param_kinds,
        "match": o_param_kinds == c_param_kinds,
    })

    # Parameter count
    results.append({
        "dimension": "parameter_count",
        "oracle": len(o_params),
        "candidate": len(c_params),
        "match": len(o_params) == len(c_params),
    })

    # Return annotation
    o_ret = oracle.get("return_annotation")
    c_ret = candidate.get("return_annotation")
    results.append({
        "dimension": "return_annotation",
        "oracle": o_ret,
        "candidate": c_ret,
        "match": o_ret == c_ret,
    })

    # Is coroutinefunction
    o_coro = oracle.get("is_coroutinefunction", False)
    c_coro = candidate.get("is_coroutinefunction", False)
    results.append({
        "dimension": "is_coroutinefunction",
        "oracle": o_coro,
        "candidate": c_coro,
        "match": o_coro == c_coro,
    })

    return results


def _compare_class_obs(oracle: dict, candidate: dict) -> list:
    """Compare two strict_class_probe observations. Returns list of comparison results."""
    results = []

    # Bases
    o_bases = oracle.get("bases", [])
    c_bases = candidate.get("bases", [])
    results.append({
        "dimension": "bases",
        "oracle": o_bases,
        "candidate": c_bases,
        "match": o_bases == c_bases,
    })

    # MRO (compare names only)
    o_mro = oracle.get("mro", [])
    c_mro = candidate.get("mro", [])
    results.append({
        "dimension": "mro",
        "oracle": o_mro,
        "candidate": c_mro,
        "match": o_mro == c_mro,
    })

    # Methods - compare names and signatures
    o_methods = oracle.get("methods", {})
    c_methods = candidate.get("methods", {})

    o_method_names = sorted(o_methods.keys())
    c_method_names = sorted(c_methods.keys())
    results.append({
        "dimension": "method_names",
        "oracle": o_method_names,
        "candidate": c_method_names,
        "match": o_method_names == c_method_names,
    })

    # Compare shared method signatures
    shared = set(o_method_names) & set(c_method_names)
    method_mismatches = []
    for name in sorted(shared):
        o_info = o_methods[name]
        c_info = c_methods[name]
        o_coro = o_info.get("is_coroutine", False)
        c_coro = c_info.get("is_coroutine", False)
        o_sig = o_info.get("signature")
        c_sig = c_info.get("signature")
        if o_coro != c_coro or o_sig != c_sig:
            method_mismatches.append(name)
    results.append({
        "dimension": "method_signature_mismatches",
        "oracle": "none",
        "candidate": method_mismatches,
        "match": len(method_mismatches) == 0,
    })

    # Class attributes
    o_attrs = set(oracle.get("class_attributes", {}).keys())
    c_attrs = set(candidate.get("class_attributes", {}).keys())
    results.append({
        "dimension": "class_attributes",
        "oracle": sorted(o_attrs),
        "candidate": sorted(c_attrs),
        "match": o_attrs == c_attrs,
    })

    return results


def compare(oracle_file: str, candidate_file: str) -> dict:
    """Compare two observation files and return a report dict."""
    with open(oracle_file, "r") as f:
        oracle = json.load(f)
    with open(candidate_file, "r") as f:
        candidate = json.load(f)

    # Detect observation type by checking for known keys
    if "parameters" in oracle or "is_coroutinefunction" in oracle:
        comparisons = _compare_signature_obs(oracle, candidate)
        obs_type = "signature"
    elif "bases" in oracle or "mro" in oracle or "methods" in oracle:
        comparisons = _compare_class_obs(oracle, candidate)
        obs_type = "class"
    else:
        comparisons = _compare_api_obs(oracle, candidate)
        obs_type = "api"

    all_match = all(c["match"] for c in comparisons)
    mismatches = [c for c in comparisons if not c["match"]]

    return {
        "obs_type": obs_type,
        "oracle_file": oracle_file,
        "candidate_file": candidate_file,
        "oracle_module": oracle.get("module"),
        "oracle_symbol": oracle.get("symbol") or oracle.get("class_name"),
        "candidate_module": candidate.get("module"),
        "candidate_symbol": candidate.get("symbol") or candidate.get("class_name"),
        "all_match": all_match,
        "total_dimensions": len(comparisons),
        "match_count": sum(1 for c in comparisons if c["match"]),
        "mismatch_count": len(mismatches),
        "comparisons": comparisons,
    }


def main():
    parser = argparse.ArgumentParser(
        description="Compare two JSON observation files and emit a comparison report."
    )
    parser.add_argument(
        "--oracle", required=True, help="Path to oracle observation JSON file"
    )
    parser.add_argument(
        "--candidate", required=True, help="Path to candidate observation JSON file"
    )
    args = parser.parse_args()

    try:
        report = compare(args.oracle, args.candidate)
        json.dump(report, sys.stdout, indent=2, default=str)
        sys.stdout.write("\n")
        sys.exit(0 if report["all_match"] else 1)
    except Exception as exc:
        error_report = {
            "error": f"HARNESS_ERROR: {type(exc).__name__}: {exc}",
            "traceback": traceback.format_exc(),
            "all_match": False,
        }
        json.dump(error_report, sys.stdout, indent=2, default=str)
        sys.stdout.write("\n")
        sys.exit(2)


if __name__ == "__main__":
    main()
