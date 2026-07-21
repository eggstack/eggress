#!/usr/bin/env python3
"""Probe a module/symbol and emit a normalized JSON observation.

Usage:
    python3 scripts/strict_api_probe.py --module pproxy.server --symbol compile_rule
    python3 scripts/strict_api_probe.py --module pproxy.proto --symbol MAPPINGS

Exit codes:
    0 - Observation emitted (even if symbol not found)
    1 - Harness error (bad arguments, JSON encoding failure)
"""

import argparse
import inspect
import json
import sys
import traceback


def probe(module_path: str, symbol_name: str) -> dict:
    """Probe a single module/symbol and return an observation dict."""
    obs = {
        "module": module_path,
        "symbol": symbol_name,
        "exists": False,
        "type": None,
        "qualname": None,
        "signature": None,
        "is_coroutine": None,
        "is_callable": False,
        "attributes": [],
        "doc_first_line": None,
        "module_file": None,
        "error": None,
        "import_error": None,
    }

    try:
        mod = __import__(module_path, fromlist=[symbol_name])
    except ImportError as exc:
        obs["import_error"] = f"{type(exc).__name__}: {exc}"
        return obs
    except Exception as exc:
        obs["error"] = f"{type(exc).__name__}: {exc}"
        obs["error_stage"] = "import"
        return obs

    obs["module_file"] = getattr(mod, "__file__", None)

    try:
        obj = getattr(mod, symbol_name)
    except AttributeError:
        obs["exists"] = False
        return obs

    obs["exists"] = True

    # Determine type
    if inspect.isclass(obj):
        obs["type"] = "class"
    elif inspect.isfunction(obj):
        obs["type"] = "function"
    elif inspect.ismethod(obj):
        obs["type"] = "method"
    elif inspect.ismodule(obj):
        obs["type"] = "module"
    elif inspect.isbuiltin(obj):
        obs["type"] = "builtin"
    elif isinstance(obj, property):
        obs["type"] = "property"
    elif isinstance(obj, (int, float, str, bytes, bool, list, dict, tuple, set, frozenset)):
        obs["type"] = f"constant:{type(obj).__name__}"
    else:
        obs["type"] = type(obj).__name__

    # Qualname
    try:
        obs["qualname"] = getattr(obj, "__qualname__", None)
    except Exception:
        pass

    # Is coroutine
    try:
        obs["is_coroutine"] = inspect.iscoroutinefunction(obj) or inspect.iscoroutine(obj)
    except Exception:
        pass

    # Is callable
    try:
        obs["is_callable"] = callable(obj)
    except Exception:
        pass

    # Signature
    try:
        sig = inspect.signature(obj)
        obs["signature"] = str(sig)
    except (ValueError, TypeError):
        pass

    # Attributes (public only)
    try:
        obs["attributes"] = sorted(
            a for a in dir(obj) if not a.startswith("_")
        )
    except Exception:
        pass

    # Doc first line
    try:
        doc = inspect.getdoc(obj)
        if doc:
            obs["doc_first_line"] = doc.split("\n", 1)[0].strip()
    except Exception:
        pass

    return obs


def main():
    parser = argparse.ArgumentParser(
        description="Probe a module/symbol and emit a normalized JSON observation."
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
            "exists": False,
            "type": None,
            "qualname": None,
            "signature": None,
            "is_coroutine": None,
            "is_callable": False,
            "attributes": [],
            "doc_first_line": None,
            "module_file": None,
            "error": f"HARNESS_ERROR: {type(exc).__name__}: {exc}",
            "import_error": None,
            "traceback": traceback.format_exc(),
        }
        json.dump(error_obs, sys.stdout, indent=2, default=str)
        sys.stdout.write("\n")
        sys.exit(1)


if __name__ == "__main__":
    main()
