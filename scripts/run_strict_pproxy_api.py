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
import ast
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


def _signatures_compatible(sig_a: str, sig_b: str) -> bool:
    """Compare two signature strings for structural compatibility.

    Compares: parameter count, kinds, names, defaults (sentinel-aware),
    positional-only markers, keyword-only markers, varargs, return annotation
    category, and coroutine status.
    """
    SENTINEL = "__EGGRESS_NO_DEFAULT__"

    def _parse_default(raw: Optional[str]) -> str:
        if raw is None:
            return SENTINEL
        if raw == "":
            return SENTINEL
        return raw

    def _parse_sig(sig_str: str) -> Optional[dict]:
        if not sig_str or sig_str == "(<not a callable>)":
            return None
        try:
            tree = ast.parse(f"def _f{sigs[sig_str]}: pass", mode="exec")
        except SyntaxError:
            return None
        func = tree.body[0]
        return func

    def _extract(sig_str: str) -> Optional[dict]:
        if not sig_str or sig_str == "(<not a callable>)":
            return None
        # Normalize: ensure it's parseable as a function def
        try:
            # Try to parse the signature string as a Python function def
            # Handle both "(x, y)" and "(x, y) -> int" forms
            normalized = sig_str.strip()
            if not normalized.startswith("("):
                return None
            # Create a temporary def to parse
            test_code = f"def _f{normalized}: pass"
            tree = ast.parse(test_code, mode="exec")
            func = tree.body[0]
            assert isinstance(func, ast.FunctionDef) or isinstance(func, ast.AsyncFunctionDef)
        except (SyntaxError, AssertionError, IndexError):
            # Fallback: string-based extraction for malformed signatures
            return _extract_fallback(sig_str)
        return _extract_from_ast(func)

    def _extract_fallback(sig_str: str) -> dict:
        """Fallback extraction for signatures that can't be parsed as function defs."""
        params = []
        inner = sig_str.strip()
        # Strip outer parens
        if inner.startswith("(") and inner.endswith(")"):
            inner = inner[1:-1]
        elif inner.startswith("("):
            inner = inner[1:]
        # Remove return annotation
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

    def _extract_from_ast(func) -> dict:
        """Extract structured signature from an AST FunctionDef/AsyncFunctionDef."""
        args = func.args
        params = []
        kind_map = {
            ast.arg.POSITIONAL_ONLY: "POSITIONAL_ONLY",
            ast.arg.POSITIONAL_OR_KEYWORD: "POSITIONAL_OR_KEYWORD",
            ast.arg.VAR_POSITIONAL: "VAR_POSITIONAL",
            ast.arg.KEYWORD_ONLY: "KEYWORD_ONLY",
            ast.arg.VAR_KEYWORD: "VAR_KEYWORD",
        }

        # positional-only args (before /)
        posonlyargs = getattr(args, "posonlyargs", [])
        for i, arg in enumerate(posonlyargs):
            default = None
            defaults_offset = len(args.args) + len(args.posonlyargs) - len(args.defaults)
            def_idx = i - (len(args.posonlyargs) - len(args.defaults))
            if def_idx >= 0 and def_idx < len(args.defaults):
                default = ast.dump(args.defaults[def_idx])
            params.append({
                "name": arg.arg,
                "kind": "POSITIONAL_ONLY",
                "default": _parse_default(default),
                "annotation": ast.dump(arg.annotation) if arg.annotation else None,
            })

        # regular args (POSITIONAL_OR_KEYWORD)
        regular_defaults_offset = len(args.args) - len(args.defaults)
        for i, arg in enumerate(args.args):
            default = None
            def_idx = i - regular_defaults_offset
            if def_idx >= 0 and def_idx < len(args.defaults):
                default = ast.dump(args.defaults[def_idx])
            params.append({
                "name": arg.arg,
                "kind": "POSITIONAL_OR_KEYWORD",
                "default": _parse_default(default),
                "annotation": ast.dump(arg.annotation) if arg.annotation else None,
            })

        # *args (VAR_POSITIONAL)
        vararg = None
        if args.vararg:
            vararg = {
                "name": args.vararg.arg,
                "kind": "VAR_POSITIONAL",
                "default": SENTINEL,
                "annotation": ast.dump(args.vararg.annotation) if args.vararg.annotation else None,
            }

        # keyword-only args
        for i, arg in enumerate(args.kwonlyargs):
            default = None
            if i < len(args.kw_defaults) and args.kw_defaults[i] is not None:
                default = ast.dump(args.kw_defaults[i])
            params.append({
                "name": arg.arg,
                "kind": "KEYWORD_ONLY",
                "default": _parse_default(default),
                "annotation": ast.dump(arg.annotation) if arg.annotation else None,
            })

        # **kwargs (VAR_KEYWORD)
        kwarg = None
        if args.kwarg:
            kwarg = {
                "name": args.kwarg.arg,
                "kind": "VAR_KEYWORD",
                "default": SENTINEL,
                "annotation": ast.dump(args.kwarg.annotation) if args.kwarg.annotation else None,
            }

        # return annotation
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

    # Compare parameter count (including vararg/kwarg)
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

    # Compare each parameter: kind, name, default, annotation category
    for pa, pb in zip(all_a, all_b):
        if pa["kind"] != pb["kind"]:
            return False
        if pa["name"] != pb["name"]:
            return False
        if pa["default"] != pb["default"]:
            return False
        if pa["annotation"] != pb["annotation"]:
            return False

    # Compare return annotation
    if parsed_a["return_annotation"] != parsed_b["return_annotation"]:
        return False

    return True


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
    comparator = record.get("comparator", "")

    # Cipher records use --cipher flag instead of --module/--symbol
    if comparator in ("cipher_kat", "cipher_roundtrip"):
        return (name, name)  # name is the cipher class name

    # Protocol wire records
    if comparator == "protocol_wire":
        return (module or name, name)

    # Process lifecycle records
    if comparator == "process_lifecycle":
        return ("pproxy.server", name)

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

    # Role/kind records: use module and name if available
    if kind in ("role", "lifecycle") and name:
        return (module or "pproxy", name)

    return None


def probe_in_venv(
    venv_python: str,
    probe_script: str,
    module: str,
    symbol: str,
    extra_args: Optional[list[str]] = None,
) -> dict:
    """Run a probe script in a venv and return the observation."""
    cmd = [venv_python, str(SCRIPTS_DIR / probe_script)]
    if extra_args:
        cmd.extend(extra_args)
    else:
        cmd.extend(["--module", module, "--symbol", symbol])
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


def compare_observations(
    oracle: dict,
    candidate: dict,
    closure_required: bool = False,
    known_upstream_defects: Optional[set] = None,
) -> dict:
    """Compare two observations and return a comparison report.

    Principles:
    - Oracle error (unless known upstream defect) → FAIL
    - Candidate error → FAIL
    - Both missing (exists=False) → FAIL (P4: Both-fail is not a match)
    - Both errors → FAIL
    - closure_required: skipped/missing records cause failure
    """
    comparisons = []
    if known_upstream_defects is None:
        known_upstream_defects = set()

    # Detect if this is a cipher observation
    is_cipher = "kat_passed" in oracle or "roundtrip_passed" in oracle

    # --- Error checking (fail-closed) ---
    o_error = oracle.get("error")
    c_error = candidate.get("error")
    o_import_error = oracle.get("import_error")
    c_import_error = candidate.get("import_error")

    # Import errors are treated as errors
    if o_import_error:
        if not o_error:
            o_error = o_import_error
    if c_import_error:
        if not c_error:
            c_error = c_import_error

    # Oracle error → FAIL unless explicitly a known upstream defect
    if o_error:
        is_known = o_error in known_upstream_defects
        comparisons.append({
            "dimension": "oracle_error",
            "oracle": o_error,
            "candidate": None,
            "match": is_known,
            "note": "Known upstream defect" if is_known else "Oracle probe produced an error",
        })

    # Candidate error → always FAIL
    if c_error:
        comparisons.append({
            "dimension": "candidate_error",
            "oracle": None,
            "candidate": c_error,
            "match": False,
            "note": "Candidate probe produced an error",
        })

    # --- Existence ---
    o_exists = oracle.get("exists", False)
    c_exists = candidate.get("exists", False)
    comparisons.append({
        "dimension": "exists",
        "oracle": o_exists,
        "candidate": c_exists,
        "match": o_exists == c_exists and o_exists is True,
    })

    # Both-fail is not a match (P4)
    if not o_exists and not c_exists:
        # Even if we recorded an error comparison above, ensure existence
        # comparison reports mismatch
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
        # Cipher observations: compare kat_passed, roundtrip_passed, ciphertext_len
        if "kat_passed" in oracle or "kat_passed" in candidate:
            o_kat = oracle.get("kat_passed", False)
            c_kat = candidate.get("kat_passed", False)
            comparisons.append({
                "dimension": "kat_passed",
                "oracle": o_kat,
                "candidate": c_kat,
                "match": o_kat == c_kat,
            })
        if "roundtrip_passed" in oracle or "roundtrip_passed" in candidate:
            o_rt = oracle.get("roundtrip_passed", False)
            c_rt = candidate.get("roundtrip_passed", False)
            comparisons.append({
                "dimension": "roundtrip_passed",
                "oracle": o_rt,
                "candidate": c_rt,
                "match": o_rt == c_rt,
            })
        o_ct_len = oracle.get("ciphertext_len") or oracle.get("encrypt_output", {}).get("ciphertext_len")
        c_ct_len = candidate.get("ciphertext_len") or candidate.get("encrypt_output", {}).get("ciphertext_len")
        if o_ct_len is not None and c_ct_len is not None:
            comparisons.append({
                "dimension": "ciphertext_len",
                "oracle": o_ct_len,
                "candidate": c_ct_len,
                "match": o_ct_len == c_ct_len,
            })
    else:
        # API observations: compare type, qualname, is_coroutine, is_callable, signature

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

        # Signature - compare with structural comparator
        o_sig = oracle.get("signature", "")
        c_sig = candidate.get("signature", "")
        comparisons.append({
            "dimension": "signature",
            "oracle": o_sig,
            "candidate": c_sig,
            "match": _signatures_compatible(o_sig, c_sig),
        })

    all_match = all(c["match"] for c in comparisons)
    mismatches = [c for c in comparisons if not c["match"]]

    result = {
        "all_match": all_match,
        "total_dimensions": len(comparisons),
        "match_count": sum(1 for c in comparisons if c["match"]),
        "mismatch_count": len(mismatches),
        "comparisons": comparisons,
    }

    # closure_required enforcement: skipped/missing records cause failure
    if closure_required and not all_match:
        result["closure_enforced"] = True

    return result


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


def verify_venv(python_path: str, is_oracle: bool = True) -> dict:
    """Verify a venv and extract metadata.

    Runs a small Python snippet in the venv to extract:
    - interpreter path
    - sys.prefix
    - pproxy.__file__ (oracle) or eggress.__file__ (candidate)
    - installed distribution name and version
    - candidate commit SHA (if available)

    Fails if:
    - Oracle imports eggress (compatibility package)
    - Candidate imports upstream pproxy (for candidate venv)
    """
    verification = {
        "interpreter": python_path,
        "is_oracle": is_oracle,
        "sys_prefix": None,
        "package_file": None,
        "distribution_name": None,
        "distribution_version": None,
        "commit_sha": None,
        "error": None,
    }

    snippet = """
import json, sys, os

result = {
    "sys_prefix": sys.prefix,
    "python_version": sys.version,
    "package_file": None,
    "distribution_name": None,
    "distribution_version": None,
    "commit_sha": None,
    "import_check": None,
}

IS_ORACLE = {is_oracle}

# Check imports
try:
    import pproxy
    result["pproxy_file"] = getattr(pproxy, "__file__", None)
    if IS_ORACLE:
        try:
            import eggress
            result["import_check"] = "FAIL: oracle imports eggress"
        except ImportError:
            result["import_check"] = "OK"
    else:
        result["import_check"] = "candidate imports pproxy (expected for compat)"
except ImportError:
    if IS_ORACLE:
        result["import_check"] = "FAIL: oracle cannot import pproxy"
    else:
        result["pproxy_file"] = None

try:
    import eggress
    result["package_file"] = getattr(eggress, "__file__", None)
except ImportError:
    if not IS_ORACLE:
        result["import_check"] = "FAIL: candidate cannot import eggress"

# Try to get distribution info
try:
    from importlib.metadata import version, packages_distributions
    try:
        ver = version("pproxy" if IS_ORACLE else "eggress")
        result["distribution_name"] = "pproxy" if IS_ORACLE else "eggress"
        result["distribution_version"] = ver
    except Exception:
        pass
except ImportError:
    pass

# Try to get commit SHA from package metadata
try:
    import importlib.metadata
    dist = importlib.metadata.distribution("eggress" if not IS_ORACLE else "pproxy")
    result["commit_sha"] = dist.metadata.get("Vcs-url", None)
    if not result["commit_sha"]:
        # Try Cargo.toml info
        for k, v in dist.metadata.items():
            if "commit" in k.lower() or "sha" in k.lower():
                result["commit_sha"] = v
                break
except Exception:
    pass

print(json.dumps(result, default=str))
""".format(is_oracle="True" if is_oracle else "False")

    try:
        result = subprocess.run(
            [python_path, "-c", snippet],
            capture_output=True,
            text=True,
            timeout=15,
            cwd=str(Path.cwd()),
        )
        if result.returncode == 0 and result.stdout.strip():
            data = json.loads(result.stdout)
            verification["sys_prefix"] = data.get("sys_prefix")
            verification["package_file"] = data.get("package_file")
            verification["distribution_name"] = data.get("distribution_name")
            verification["distribution_version"] = data.get("distribution_version")
            verification["commit_sha"] = data.get("commit_sha")
            verification["python_version"] = data.get("python_version")
            verification["import_check"] = data.get("import_check")

            if data.get("import_check", "").startswith("FAIL"):
                verification["error"] = data["import_check"]
        else:
            verification["error"] = f"Verification script failed: {result.stderr.strip()}"
    except subprocess.TimeoutExpired:
        verification["error"] = "Verification script timed out"
    except json.JSONDecodeError as e:
        verification["error"] = f"Invalid JSON from verification: {e}"

    return verification


def run_paired_comparison(
    records: list[dict],
    oracle_python: str,
    candidate_python: str,
    probe_type: str = "api",
    output_dir: Optional[Path] = None,
    closure_required: bool = False,
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
        "cipher_kat": "strict_cipher_kat_probe.py",
        "cipher_roundtrip": "strict_cipher_roundtrip_probe.py",
        "protocol_wire": "strict_protocol_wire_probe.py",
        "process_lifecycle": "strict_process_lifecycle_probe.py",
        "failure_class": "strict_api_probe.py",  # Fallback to api probe
    }

    for i, record in enumerate(records, 1):
        rid = record.get("id", "?")
        comparator = record.get("comparator", "module_existence")
        args = extract_probe_args(record)
        if not args:
            results.append({
                "id": rid,
                "status": "skipped",
                "reason": "Cannot extract module/symbol from record",
            })
            continue

        module, symbol = args

        # Select probe based on comparator
        if comparator in ("cipher_kat", "cipher_roundtrip"):
            probe_script = probe_map.get(comparator, "strict_api_probe.py")
            extra_args = ["--cipher", symbol]
        elif comparator == "protocol_wire":
            probe_script = probe_map.get(comparator, "strict_api_probe.py")
            extra_args = ["--module", module, "--symbol", symbol, "--test", "address_encode"]
        elif comparator == "process_lifecycle":
            probe_script = probe_map.get(comparator, "strict_api_probe.py")
            # Determine appropriate test based on record kind
            kind = record.get("kind", "")
            if kind == "class":
                test_name = "proxy_object"
            elif kind == "function":
                test_name = "server_create"
            else:
                test_name = "constants"
            extra_args = ["--module", module, "--symbol", symbol, "--test", test_name]
        else:
            probe_script = probe_map.get(probe_type, "strict_api_probe.py")
            extra_args = None

        print(f"  [{i}/{total}] {rid} ({module}.{symbol}) [{comparator}]", file=sys.stderr)

        # Run oracle probe
        oracle_obs = probe_in_venv(oracle_python, probe_script, module, symbol, extra_args)

        # Run candidate probe
        candidate_obs = probe_in_venv(candidate_python, probe_script, module, symbol, extra_args)

        # Compare
        comparison = compare_observations(oracle_obs, candidate_obs, closure_required=closure_required)

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
    parser.add_argument("--closure-required", action="store_true",
                        help="Enforce closure: skipped/missing records cause failure")
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

    # Verify venvs and extract metadata
    print("Verifying oracle venv...", file=sys.stderr)
    oracle_verification = verify_venv(oracle_python, is_oracle=True)
    if oracle_verification.get("error"):
        print(f"Error: oracle venv verification failed: {oracle_verification['error']}", file=sys.stderr)
        sys.exit(2)

    print("Verifying candidate venv...", file=sys.stderr)
    candidate_verification = verify_venv(candidate_python, is_oracle=False)
    if candidate_verification.get("error"):
        print(f"Error: candidate venv verification failed: {candidate_verification['error']}", file=sys.stderr)
        sys.exit(2)

    print(f"  Oracle: {oracle_verification.get('distribution_name')} "
          f"{oracle_verification.get('distribution_version')} "
          f"({oracle_verification.get('python_version', '').split()[0]})",
          file=sys.stderr)
    print(f"  Candidate: {candidate_verification.get('distribution_name')} "
          f"{candidate_verification.get('distribution_version')} "
          f"({candidate_verification.get('python_version', '').split()[0]})",
          file=sys.stderr)

    # Run comparison
    output_dir = Path(args.output_dir) if args.output_dir else Path("target/strict/paired_observations")
    print(f"\nRunning paired comparison for {len(testable)} records...", file=sys.stderr)

    report = run_paired_comparison(
        testable, oracle_python, candidate_python,
        output_dir=output_dir,
        closure_required=args.closure_required,
    )

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
    report["oracle_verification"] = oracle_verification
    report["candidate_verification"] = candidate_verification
    report["closure_required"] = args.closure_required
    report_file = output_dir / "paired_api_report.json"
    output_dir.mkdir(parents=True, exist_ok=True)
    report_file.write_text(json.dumps(report, indent=2, default=str))
    print(f"\nReport written to: {report_file}", file=sys.stderr)

    sys.exit(0 if report["failed"] == 0 else 1)


if __name__ == "__main__":
    main()
