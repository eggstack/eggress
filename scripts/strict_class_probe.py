#!/usr/bin/env python3
"""Probe a module/class and emit a normalized JSON observation focused on class structure.

Usage:
    python3 scripts/strict_class_probe.py --module pproxy.server --class-name ProxyDirect

Exit codes:
    0 - Observation emitted (even if class not found)
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


def _get_method_info(obj, method_name):
    """Get info about a method on a class."""
    try:
        attr = getattr(obj, method_name, None)
        if attr is None:
            return None
        is_coroutine = inspect.iscoroutinefunction(attr) or inspect.iscoroutine(attr)
        sig_str = None
        try:
            sig = inspect.signature(attr)
            sig_str = str(sig)
        except (ValueError, TypeError):
            pass
        return {
            "is_coroutine": is_coroutine,
            "signature": sig_str,
        }
    except Exception:
        return None


def probe(module_path: str, class_name: str) -> dict:
    """Probe a single module/class and return a class-structure observation dict."""
    obs = {
        "module": module_path,
        "class_name": class_name,
        "bases": [],
        "mro": [],
        "methods": {},
        "class_attributes": {},
        "instance_check": None,
        "error": None,
    }

    try:
        mod = __import__(module_path, fromlist=[class_name])
    except ImportError as exc:
        obs["error"] = f"ImportError: {exc}"
        return obs
    except Exception as exc:
        obs["error"] = f"{type(exc).__name__}: {exc}"
        return obs

    try:
        cls = getattr(mod, class_name)
    except AttributeError:
        obs["error"] = f"AttributeError: {module_path} has no attribute {class_name}"
        return obs

    # Check if it's a class
    if not inspect.isclass(cls):
        obs["error"] = f"TypeError: {class_name} is not a class (it is {type(cls).__name__})"
        return obs

    # Instance check
    obs["instance_check"] = True

    # Bases
    try:
        obs["bases"] = [getattr(b, "__name__", repr(b)) for b in cls.__bases__]
    except Exception:
        pass

    # MRO
    try:
        obs["mro"] = [getattr(c, "__name__", repr(c)) for c in cls.__mro__]
    except Exception:
        pass

    # Methods (public only, skip dunder)
    try:
        for name in sorted(dir(cls)):
            if name.startswith("_"):
                continue
            attr = getattr(cls, name, None)
            if attr is None:
                continue
            if callable(attr) or inspect.isfunction(attr) or inspect.ismethod(attr):
                info = _get_method_info(cls, name)
                if info is not None:
                    obs["methods"][name] = info
    except Exception:
        pass

    # Class attributes (non-callable, non-dunder)
    try:
        for name in sorted(vars(cls)):
            if name.startswith("_"):
                continue
            try:
                val = getattr(cls, name)
                if not callable(val) and not inspect.isfunction(val):
                    obs["class_attributes"][name] = True
            except Exception:
                obs["class_attributes"][name] = True
    except Exception:
        pass

    return obs


def main():
    parser = argparse.ArgumentParser(
        description="Probe a module/class and emit a class-structure JSON observation."
    )
    parser.add_argument(
        "--module", required=True, help="Fully qualified module path (e.g. pproxy.server)"
    )
    parser.add_argument(
        "--class-name", required=True, help="Class name to inspect (e.g. ProxyDirect)"
    )
    args = parser.parse_args()

    try:
        obs = probe(args.module, args.class_name)
        json.dump(obs, sys.stdout, indent=2, default=str)
        sys.stdout.write("\n")
        sys.exit(0)
    except Exception as exc:
        error_obs = {
            "module": args.module,
            "class_name": args.class_name,
            "bases": [],
            "mro": [],
            "methods": {},
            "class_attributes": {},
            "instance_check": None,
            "error": f"HARNESS_ERROR: {type(exc).__name__}: {exc}",
            "traceback": traceback.format_exc(),
        }
        json.dump(error_obs, sys.stdout, indent=2, default=str)
        sys.stdout.write("\n")
        sys.exit(1)


if __name__ == "__main__":
    main()
