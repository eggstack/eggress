"""Shared infrastructure for strict differential tests.

Provides pytest CLI arguments for observation directories, a hardened
comparator, and helpers to load pre-generated observations.
"""

import argparse
import ast
import json
import re
import sys
from pathlib import Path
from typing import Optional

import pytest


SENTINEL = "__EGGRESS_NO_DEFAULT__"


def pytest_addoption(parser):
    parser.addoption(
        "--oracle-observations-dir",
        action="store",
        default=None,
        help="Directory containing oracle observation JSON files",
    )
    parser.addoption(
        "--candidate-observations-dir",
        action="store",
        default=None,
        help="Directory containing candidate observation JSON files",
    )


def _signatures_compatible(sig_a: str, sig_b: str) -> bool:
    """Compare two signature strings for structural compatibility."""
    def _parse_default(raw):
        if raw is None:
            return SENTINEL
        if raw == "":
            return SENTINEL
        return raw

    def _extract(sig_str):
        if not sig_str or sig_str == "(<not a callable>)":
            return None
        try:
            normalized = sig_str.strip()
            if not normalized.startswith("("):
                return None
            test_code = f"def _f{normalized}: pass"
            tree = ast.parse(test_code, mode="exec")
            func = tree.body[0]
            assert isinstance(func, (ast.FunctionDef, ast.AsyncFunctionDef))
        except (SyntaxError, AssertionError, IndexError):
            return _extract_fallback(sig_str)
        return _extract_from_ast(func)

    def _extract_fallback(sig_str):
        params = []
        inner = sig_str.strip()
        if inner.startswith("(") and inner.endswith(")"):
            inner = inner[1:-1]
        elif inner.startswith("("):
            inner = inner[1:]
        arrow_idx = inner.rfind(") -> ")
        if arrow_idx >= 0:
            inner = inner[:arrow_idx]
        elif inner.endswith(")"):
            inner = inner[:-1]
        if not inner.strip():
            return {"params": [], "vararg": None, "kwarg": None, "return_annotation": None}
        for part in inner.split(","):
            part = part.strip()
            if not part:
                continue
            name = part.lstrip("*")
            name = re.split(r"[:=]", name)[0].strip()
            if name:
                kind = "POSITIONAL_OR_KEYWORD"
                if part.startswith("**"):
                    kind = "VAR_KEYWORD"
                elif part.startswith("*"):
                    kind = "VAR_POSITIONAL"
                params.append({"name": name, "kind": kind, "default": SENTINEL, "annotation": None})
        return {"params": params, "vararg": None, "kwarg": None, "return_annotation": None}

    def _extract_from_ast(func):
        args = func.args
        params = []
        posonlyargs = getattr(args, "posonlyargs", [])
        for i, arg in enumerate(posonlyargs):
            default = None
            def_idx = i - (len(args.posonlyargs) - len(args.defaults))
            if 0 <= def_idx < len(args.defaults):
                default = ast.dump(args.defaults[def_idx])
            params.append({
                "name": arg.arg, "kind": "POSITIONAL_ONLY",
                "default": _parse_default(default),
                "annotation": ast.dump(arg.annotation) if arg.annotation else None,
            })
        regular_defaults_offset = len(args.args) - len(args.defaults)
        for i, arg in enumerate(args.args):
            default = None
            def_idx = i - regular_defaults_offset
            if 0 <= def_idx < len(args.defaults):
                default = ast.dump(args.defaults[def_idx])
            params.append({
                "name": arg.arg, "kind": "POSITIONAL_OR_KEYWORD",
                "default": _parse_default(default),
                "annotation": ast.dump(arg.annotation) if arg.annotation else None,
            })
        vararg = None
        if args.vararg:
            vararg = {
                "name": args.vararg.arg, "kind": "VAR_POSITIONAL",
                "default": SENTINEL,
                "annotation": ast.dump(args.vararg.annotation) if args.vararg.annotation else None,
            }
        for i, arg in enumerate(args.kwonlyargs):
            default = None
            if i < len(args.kw_defaults) and args.kw_defaults[i] is not None:
                default = ast.dump(args.kw_defaults[i])
            params.append({
                "name": arg.arg, "kind": "KEYWORD_ONLY",
                "default": _parse_default(default),
                "annotation": ast.dump(arg.annotation) if arg.annotation else None,
            })
        kwarg = None
        if args.kwarg:
            kwarg = {
                "name": args.kwarg.arg, "kind": "VAR_KEYWORD",
                "default": SENTINEL,
                "annotation": ast.dump(args.kwarg.annotation) if args.kwarg.annotation else None,
            }
        ret_ann = ast.dump(func.returns) if func.returns else None
        return {"params": params, "vararg": vararg, "kwarg": kwarg, "return_annotation": ret_ann}

    if not sig_a or sig_a == "(<not a callable>)":
        if not sig_b or sig_b == "(<not a callable>)":
            return True
        return False
    if not sig_b or sig_b == "(<not a callable>)":
        return False

    parsed_a = _extract(sig_a)
    parsed_b = _extract(sig_b)
    if parsed_a is None and parsed_b is None:
        return sig_a == sig_b
    if parsed_a is None or parsed_b is None:
        return False

    all_a = parsed_a["params"][:]
    all_b = parsed_b["params"][:]
    if parsed_a["vararg"]:
        all_a.append(parsed_a["vararg"])
    if parsed_b["vararg"]:
        all_b.append(parsed_b["vararg"])
    if parsed_a["kwarg"]:
        all_a.append(parsed_a["kwarg"])
    if parsed_b["kwarg"]:
        all_b.append(parsed_b["kwarg"])

    if len(all_a) != len(all_b):
        return False

    for pa, pb in zip(all_a, all_b):
        if pa["kind"] != pb["kind"]:
            return False
        if pa["name"] != pb["name"]:
            return False
        if pa["default"] != pb["default"]:
            return False
        if pa["annotation"] != pb["annotation"]:
            return False

    if parsed_a["return_annotation"] != parsed_b["return_annotation"]:
        return False

    return True


def compare_observations(
    oracle: dict,
    candidate: dict,
    known_upstream_defects: Optional[set] = None,
) -> dict:
    """Compare two observations with hardened logic.

    Principles:
    - Oracle error (unless known upstream defect) -> FAIL
    - Candidate error -> FAIL
    - Both missing (exists=False) -> FAIL (P4: Both-fail is not a match)
    - Both errors -> FAIL
    """
    comparisons = []
    if known_upstream_defects is None:
        known_upstream_defects = set()

    is_cipher = "kat_passed" in oracle or "roundtrip_passed" in oracle

    o_error = oracle.get("error")
    c_error = candidate.get("error")

    if o_error:
        is_known = o_error in known_upstream_defects
        comparisons.append({
            "dimension": "oracle_error",
            "oracle": o_error,
            "candidate": None,
            "match": is_known,
            "note": "Known upstream defect" if is_known else "Oracle probe produced an error",
        })

    if c_error:
        comparisons.append({
            "dimension": "candidate_error",
            "oracle": None,
            "candidate": c_error,
            "match": False,
            "note": "Candidate probe produced an error",
        })

    o_exists = oracle.get("exists", False)
    c_exists = candidate.get("exists", False)
    comparisons.append({
        "dimension": "exists",
        "oracle": o_exists,
        "candidate": c_exists,
        "match": o_exists == c_exists and o_exists is True,
    })

    if not o_exists and not c_exists:
        all_match = False
        mismatches = [c for c in comparisons if not c["match"]]
        return {
            "all_match": all_match,
            "total_dimensions": len(comparisons),
            "match_count": sum(1 for c in comparisons if c["match"]),
            "mismatch_count": len(mismatches),
            "comparisons": comparisons,
            "failure_reason": "both_missing" if not o_error and not c_error else "error_and_missing",
        }

    if is_cipher:
        if "kat_passed" in oracle or "kat_passed" in candidate:
            o_kat = oracle.get("kat_passed", False)
            c_kat = candidate.get("kat_passed", False)
            comparisons.append({
                "dimension": "kat_passed", "oracle": o_kat, "candidate": c_kat,
                "match": o_kat == c_kat,
            })
        if "roundtrip_passed" in oracle or "roundtrip_passed" in candidate:
            o_rt = oracle.get("roundtrip_passed", False)
            c_rt = candidate.get("roundtrip_passed", False)
            comparisons.append({
                "dimension": "roundtrip_passed", "oracle": o_rt, "candidate": c_rt,
                "match": o_rt == c_rt,
            })
        o_ct_len = oracle.get("ciphertext_len") or oracle.get("encrypt_output", {}).get("ciphertext_len")
        c_ct_len = candidate.get("ciphertext_len") or candidate.get("encrypt_output", {}).get("ciphertext_len")
        if o_ct_len is not None and c_ct_len is not None:
            comparisons.append({
                "dimension": "ciphertext_len", "oracle": o_ct_len, "candidate": c_ct_len,
                "match": o_ct_len == c_ct_len,
            })
    else:
        o_type = oracle.get("type")
        c_type = candidate.get("type")
        comparisons.append({"dimension": "type", "oracle": o_type, "candidate": c_type, "match": o_type == c_type})

        o_qualname = oracle.get("qualname", "")
        c_qualname = candidate.get("qualname", "")
        o_local = o_qualname.rsplit(".", 1)[-1] if o_qualname else ""
        c_local = c_qualname.rsplit(".", 1)[-1] if c_qualname else ""
        comparisons.append({"dimension": "qualname_local", "oracle": o_local, "candidate": c_local, "match": o_local == c_local})

        o_coro = oracle.get("is_coroutine")
        c_coro = candidate.get("is_coroutine")
        comparisons.append({"dimension": "is_coroutine", "oracle": o_coro, "candidate": c_coro, "match": o_coro == c_coro})

        o_callable = oracle.get("is_callable", False)
        c_callable = candidate.get("is_callable", False)
        comparisons.append({"dimension": "is_callable", "oracle": o_callable, "candidate": c_callable, "match": o_callable == c_callable})

        o_sig = oracle.get("signature", "")
        c_sig = candidate.get("signature", "")
        comparisons.append({"dimension": "signature", "oracle": o_sig, "candidate": c_sig, "match": _signatures_compatible(o_sig, c_sig)})

    all_match = all(c["match"] for c in comparisons)
    mismatches = [c for c in comparisons if not c["match"]]

    return {
        "all_match": all_match,
        "total_dimensions": len(comparisons),
        "match_count": sum(1 for c in comparisons if c["match"]),
        "mismatch_count": len(mismatches),
        "comparisons": comparisons,
    }


def load_observation(obs_dir: Path, rid: str, side: str) -> dict:
    """Load an observation JSON file from the given directory.

    Args:
        obs_dir: Root observation directory
        rid: Record ID (e.g. "python.pproxy")
        side: "oracle" or "candidate"
    """
    filename = f"{rid.replace('.', '_')}_{side}.json"
    filepath = obs_dir / filename
    if not filepath.exists():
        return {"exists": False, "error": f"Observation file not found: {filepath}"}
    return json.loads(filepath.read_text())


@pytest.fixture
def require_obs_dirs(request):
    """Require that observation directories are configured, skip test otherwise."""
    oracle_dir = request.config.getoption("--oracle-observations-dir")
    candidate_dir = request.config.getoption("--candidate-observations-dir")
    if not oracle_dir or not candidate_dir:
        pytest.skip("--oracle-observations-dir and --candidate-observations-dir required")
    return Path(oracle_dir), Path(candidate_dir)
