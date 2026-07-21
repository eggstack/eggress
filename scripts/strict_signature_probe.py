#!/usr/bin/env python3
"""Probe a module/symbol and emit a normalized JSON observation focused on signatures.

Usage:
    python3 scripts/strict_signature_probe.py --module pproxy.server --symbol compile_rule

Exit codes:
    0 - Observation emitted (even if symbol not found)
    1 - Harness error (bad arguments, JSON encoding failure)
"""

import argparse
import inspect
import json
import sys
import traceback


def _format_annotation(annotation):
    """Format an annotation to a string, handling None and complex types."""
    if annotation is inspect.Parameter.empty or annotation is inspect.Signature.empty:
        return None
    if annotation is None:
        return "None"
    try:
        return str(annotation)
    except Exception:
        return repr(annotation)


def _serialize_parameter(param):
    """Serialize an inspect.Parameter to a JSON-serializable dict."""
    kind_map = {
        inspect.Parameter.POSITIONAL_ONLY: "POSITIONAL_ONLY",
        inspect.Parameter.POSITIONAL_OR_KEYWORD: "POSITIONAL_OR_KEYWORD",
        inspect.Parameter.VAR_POSITIONAL: "VAR_POSITIONAL",
        inspect.Parameter.KEYWORD_ONLY: "KEYWORD_ONLY",
        inspect.Parameter.VAR_KEYWORD: "VAR_KEYWORD",
    }
    result = {
        "name": param.name,
        "kind": kind_map.get(param.kind, str(param.kind)),
        "default": _format_annotation(param.default),
        "annotation": _format_annotation(param.annotation),
    }
    return result


def probe(module_path: str, symbol_name: str) -> dict:
    """Probe a single module/symbol and return a signature-focused observation dict."""
    obs = {
        "module": module_path,
        "symbol": symbol_name,
        "parameters": [],
        "return_annotation": None,
        "is_coroutinefunction": False,
        "is_coroutine": False,
        "doc": None,
        "error": None,
    }

    try:
        mod = __import__(module_path, fromlist=[symbol_name])
    except ImportError as exc:
        obs["error"] = f"ImportError: {exc}"
        return obs
    except Exception as exc:
        obs["error"] = f"{type(exc).__name__}: {exc}"
        return obs

    try:
        obj = getattr(mod, symbol_name)
    except AttributeError:
        obs["error"] = f"AttributeError: {module_path} has no attribute {symbol_name}"
        return obs

    # Is coroutine
    try:
        obs["is_coroutinefunction"] = inspect.iscoroutinefunction(obj)
        obs["is_coroutine"] = inspect.iscoroutine(obj)
    except Exception:
        pass

    # Docstring
    try:
        obs["doc"] = inspect.getdoc(obj)
    except Exception:
        pass

    # Signature inspection
    try:
        sig = inspect.signature(obj)
        obs["return_annotation"] = _format_annotation(sig.return_annotation)
        obs["parameters"] = [_serialize_parameter(p) for p in sig.parameters.values()]
    except (ValueError, TypeError) as exc:
        obs["error"] = f"SignatureError: {exc}"
    except Exception as exc:
        obs["error"] = f"{type(exc).__name__}: {exc}"

    return obs


def main():
    parser = argparse.ArgumentParser(
        description="Probe a module/symbol and emit a signature-focused JSON observation."
    )
    parser.add_argument(
        "--module", required=True, help="Fully qualified module path (e.g. pproxy.server)"
    )
    parser.add_argument(
        "--symbol", required=True, help="Symbol name to inspect (e.g. compile_rule)"
    )
    args = parser.parse_args()

    try:
        obs = probe(args.module, args.symbol)
        json.dump(obs, sys.stdout, indent=2, default=str)
        sys.stdout.write("\n")
        sys.exit(0)
    except Exception as exc:
        error_obs = {
            "module": args.module,
            "symbol": args.symbol,
            "parameters": [],
            "return_annotation": None,
            "is_coroutinefunction": False,
            "is_coroutine": False,
            "doc": None,
            "error": f"HARNESS_ERROR: {type(exc).__name__}: {exc}",
            "traceback": traceback.format_exc(),
        }
        json.dump(error_obs, sys.stdout, indent=2, default=str)
        sys.stdout.write("\n")
        sys.exit(1)


if __name__ == "__main__":
    main()
