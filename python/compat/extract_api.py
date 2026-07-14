#!/usr/bin/env python3
"""Extract pproxy's public API contract into a JSON schema.

Records every public symbol across pproxy, pproxy.proto, pproxy.cipher,
and pproxy.server with full introspection: signatures, class hierarchies,
async classifications, properties, constants, aliases, and docstrings.

Usage:
    python3.11 python/compat/extract_api.py
"""
from __future__ import annotations

import importlib
import importlib.metadata
import inspect
import json
import sys
import textwrap
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

PINNED_VERSION = "2.7.9"
OUTPUT_PATH = Path(__file__).resolve().parent / "pproxy_api_contract.json"

SUBMODULES = ["pproxy", "pproxy.proto", "pproxy.cipher", "pproxy.server"]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _safe_signature(obj: Any) -> str | None:
    """Return inspect.signature as a string, or None on failure."""
    try:
        return str(inspect.signature(obj))
    except (ValueError, TypeError):
        return None


def _safe_doc(obj: Any) -> str | None:
    """Return a stripped docstring or None."""
    doc = inspect.getdoc(obj)
    if doc:
        return textwrap.dedent(doc).strip()
    return None


def _is_coroutine(obj: Any) -> bool:
    return inspect.iscoroutinefunction(obj)


def _is_generator(obj: Any) -> bool:
    return inspect.isgeneratorfunction(obj)


def _is_async_generator(obj: Any) -> bool:
    return inspect.isasyncgenfunction(obj)


def _callable_classification(obj: Any) -> dict[str, bool]:
    """Return coroutine/generator/async-generator flags for a callable."""
    return {
        "is_coroutine": _is_coroutine(obj),
        "is_generator": _is_generator(obj),
        "is_async_generator": _is_async_generator(obj),
    }


def _safe_repr(obj: Any) -> str:
    """Best-effort repr that never raises."""
    try:
        r = repr(obj)
        if len(r) > 512:
            r = r[:509] + "..."
        return r
    except Exception:
        return "<unrepresentable>"


def _safe_json_value(obj: Any) -> Any:
    """Try to serialize obj as a JSON-safe value. Falls back to repr string."""
    if obj is None or isinstance(obj, (bool, int, float, str)):
        return obj
    if isinstance(obj, bytes):
        return obj.hex()
    try:
        json.dumps(obj)
        return obj
    except (TypeError, ValueError, OverflowError):
        return _safe_repr(obj)


def _public_names(module: Any) -> list[str]:
    """Return sorted public names from a module (no underscore prefixes)."""
    return sorted(n for n in dir(module) if not n.startswith("_"))


def _class_bases(cls: type) -> list[str]:
    """Qualified names of direct bases (excluding object)."""
    return [b.__qualname__ for b in cls.__bases__ if b is not object]


def _class_mro(cls: type) -> list[str]:
    """Qualified names of the full MRO (excluding object)."""
    return [b.__qualname__ for b in cls.__mro__ if b is not object]


def _extract_methods(cls: type) -> dict[str, dict]:
    """Extract public methods, their signatures, and coroutine classification."""
    methods: dict[str, dict] = {}
    for name, obj in inspect.getmembers(cls):
        if name.startswith("_"):
            continue
        if inspect.isfunction(obj) or inspect.ismethod(obj):
            entry: dict[str, Any] = {}
            sig = _safe_signature(obj)
            if sig is not None:
                entry["signature"] = sig
            entry.update(_callable_classification(obj))
            methods[name] = entry
    return methods


def _extract_properties(cls: type) -> list[str]:
    """Return names of public properties/descriptors on a class."""
    props: list[str] = []
    for name in dir(cls):
        if name.startswith("_"):
            continue
        try:
            attr = inspect.getattr_static(cls, name)
        except AttributeError:
            continue
        if isinstance(attr, property):
            props.append(name)
    return sorted(props)


# ---------------------------------------------------------------------------
# Per-symbol extractor
# ---------------------------------------------------------------------------

def extract_symbol(name: str, obj: Any, module_name: str) -> dict[str, Any]:
    """Extract a full description of a single public symbol."""
    entry: dict[str, Any] = {
        "module": module_name,
        "qualname": getattr(obj, "__qualname__", name),
        "docstring": _safe_doc(obj),
    }

    # --- Modules ---
    if inspect.ismodule(obj):
        entry["kind"] = "module"
        return entry

    # --- Classes ---
    if inspect.isclass(obj):
        entry["kind"] = "class"
        entry["bases"] = _class_bases(obj)
        entry["mro"] = _class_mro(obj)
        entry["signature"] = _safe_signature(obj)
        entry.update(_callable_classification(obj))
        entry["methods"] = _extract_methods(obj)
        entry["properties"] = _extract_properties(obj)
        return entry

    # --- Functions / callables ---
    if callable(obj) and not inspect.ismodule(obj):
        entry["kind"] = "function"
        entry["signature"] = _safe_signature(obj)
        entry.update(_callable_classification(obj))
        return entry

    # --- Constants / plain values ---
    entry["kind"] = "constant"
    entry["value"] = _safe_json_value(obj)
    return entry


# ---------------------------------------------------------------------------
# Module-level extractor
# ---------------------------------------------------------------------------

def extract_module(module_name: str) -> tuple[dict[str, Any], dict[str, dict], dict[str, str], dict[str, Any]]:
    """Extract all public symbols from a single module.

    Returns:
        (module_record, symbols_dict, aliases_dict, constants_dict)
    """
    module = importlib.import_module(module_name)
    exports: list[str] = _public_names(module)

    # Discover submodules if this is the top-level package
    submodules: list[str] = []
    if module_name == "pproxy":
        for attr_name in exports:
            attr = getattr(module, attr_name, None)
            if inspect.ismodule(attr) and attr.__name__.startswith("pproxy."):
                submodules.append(attr.__name__)

    module_record: dict[str, Any] = {
        "exports": exports,
        "submodules": sorted(submodules),
    }

    symbols: dict[str, dict] = {}
    aliases: dict[str, str] = {}
    constants: dict[str, Any] = {}

    for name in exports:
        obj = getattr(module, name, None)
        if obj is None:
            continue

        qualified = f"{module_name}.{name}"
        info = extract_symbol(name, obj, module_name)

        # Detect identity aliases: if an object's module differs from where
        # we found it, record the canonical location.
        if hasattr(obj, "__module__") and obj.__module__ and obj.__module__ != module_name:
            # Check identity-based alias
            try:
                canonical_module = importlib.import_module(obj.__module__)
                canonical_obj = getattr(canonical_module, name, None)
                if canonical_obj is obj:
                    aliases[qualified] = f"{obj.__module__}.{name}"
            except ImportError:
                pass

        # Detect function aliases (e.g. pproxy.Connection == pproxy.server.proxies_by_uri)
        if info["kind"] == "function" and hasattr(obj, "__qualname__"):
            # Try to find where the function is actually defined
            source_file = None
            try:
                source_file = inspect.getfile(obj)
            except (TypeError, OSError):
                pass
            if source_file:
                # Check if it's defined in a different submodule
                for sub in SUBMODULES:
                    if sub == module_name:
                        continue
                    if sub in source_file.replace("/", ".").replace("-", "_"):
                        # Verify identity
                        try:
                            sub_mod = importlib.import_module(sub)
                            sub_obj = getattr(sub_mod, name, None)
                            if sub_obj is not obj:
                                # Check all public names in sub module
                                for sub_name in _public_names(sub_mod):
                                    sub_candidate = getattr(sub_mod, sub_name, None)
                                    if sub_candidate is obj:
                                        aliases[qualified] = f"{sub}.{sub_name}"
                                        break
                        except ImportError:
                            pass
                        break

        # Separate constants from symbols
        if info["kind"] == "constant":
            constants[qualified] = info.get("value")
        else:
            symbols[qualified] = info

    return module_record, symbols, aliases, constants


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def build_contract() -> dict[str, Any]:
    """Build the complete API contract."""
    all_symbols: dict[str, dict] = {}
    all_aliases: dict[str, str] = {}
    all_constants: dict[str, Any] = {}
    modules: dict[str, dict] = {}

    for mod_name in SUBMODULES:
        try:
            mod_record, syms, als, consts = extract_module(mod_name)
            modules[mod_name] = mod_record
            all_symbols.update(syms)
            all_aliases.update(als)
            all_constants.update(consts)
        except ImportError as exc:
            modules[mod_name] = {"error": str(exc), "exports": [], "submodules": []}
            print(f"WARNING: Could not import {mod_name}: {exc}", file=sys.stderr)

    # Resolve version
    try:
        version = importlib.metadata.version("pproxy")
    except importlib.metadata.PackageNotFoundError:
        version = "unknown"

    python_version = f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}"

    contract = {
        "schema_version": "1.0.0",
        "pproxy_version": version,
        "python_version": python_version,
        "extracted_at": datetime.now(timezone.utc).isoformat(),
        "modules": modules,
        "symbols": all_symbols,
        "aliases": all_aliases,
        "constants": all_constants,
    }
    return contract


def main() -> None:
    contract = build_contract()

    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT_PATH.write_text(json.dumps(contract, indent=2, sort_keys=False) + "\n")

    total_symbols = len(contract["symbols"])
    total_constants = len(contract["constants"])
    total_aliases = len(contract["aliases"])

    print(f"Contract written to {OUTPUT_PATH}")
    print(f"  pproxy version   : {contract['pproxy_version']}")
    print(f"  python version   : {contract['python_version']}")
    print(f"  modules covered  : {len(contract['modules'])}")
    print(f"  symbols extracted: {total_symbols}")
    print(f"  constants        : {total_constants}")
    print(f"  aliases          : {total_aliases}")

    # Per-module breakdown
    for mod_name, mod_info in contract["modules"].items():
        export_count = len(mod_info.get("exports", []))
        sub_count = len(mod_info.get("submodules", []))
        print(f"  {mod_name:20s}: {export_count} exports, {sub_count} submodules")


if __name__ == "__main__":
    main()
