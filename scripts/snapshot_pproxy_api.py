#!/usr/bin/env python3
"""Snapshot pproxy's public API into a JSON fixture for oracle tests.

Inspects module exports, protocol classes, cipher classes, scheduling
algorithms, CLI arguments, and class hierarchy. Writes the result to
tests/compat/fixtures/pproxy_api_snapshot.json.

Usage:
    python3 scripts/snapshot_pproxy_api.py
"""
from __future__ import annotations

import importlib
import inspect
import json
import sys
from pathlib import Path

FIXTURE_PATH = Path(__file__).resolve().parent.parent / "tests" / "compat" / "fixtures" / "pproxy_api_snapshot.json"


def _public_names(module) -> list[str]:
    return sorted(name for name in dir(module) if not name.startswith("_"))


def _class_info(cls) -> dict:
    bases = [b.__qualname__ for b in cls.__mro__ if b is not object]
    methods = sorted(
        name for name, _ in inspect.getmembers(cls, predicate=inspect.isfunction)
        if not name.startswith("_")
    )
    return {"qualname": cls.__qualname__, "module": cls.__module__, "bases": bases, "methods": methods}


def _extract_protocol_classes(proto_module) -> dict[str, dict]:
    result: dict[str, dict] = {}
    for name in _public_names(proto_module):
        obj = getattr(proto_module, name)
        if inspect.isclass(obj) and not name.startswith("_"):
            result[name] = _class_info(obj)
    return result


def _extract_cipher_info(cipher_module) -> dict:
    info: dict[str, list[str]] = {}
    for name in _public_names(cipher_module):
        obj = getattr(cipher_module, name)
        if inspect.isclass(obj):
            info.setdefault("classes", []).append(name)
        elif callable(obj) and not inspect.ismodule(obj):
            info.setdefault("functions", []).append(name)
    return info


def _extract_scheduling(server_module) -> list[str]:
    found: list[str] = []
    for name in _public_names(server_module):
        obj = getattr(server_module, name)
        if callable(obj) and not inspect.isclass(obj) and not inspect.ismodule(obj):
            low = name.lower()
            if any(k in low for k in ("schedul", "rr", "round", "lc", "least", "first")):
                found.append(name)
    return sorted(set(found))


def build_snapshot() -> dict:
    pproxy = importlib.import_module("pproxy")
    proto = importlib.import_module("pproxy.proto")
    cipher = importlib.import_module("pproxy.cipher")
    server = importlib.import_module("pproxy.server")

    # Top-level exports
    module_exports = _public_names(pproxy)

    # Sub-module exports
    submodules: dict[str, list[str]] = {}
    for name in ("proto", "server", "cipher"):
        try:
            mod = importlib.import_module(f"pproxy.{name}")
            submodules[name] = _public_names(mod)
        except ImportError:
            pass

    # Protocol classes from pproxy.proto
    protocols = _extract_protocol_classes(proto)

    # Cipher classes / functions from pproxy.cipher
    ciphers = _extract_cipher_info(cipher)

    # Scheduling hints from pproxy.server
    scheduling = _extract_scheduling(server)

    # Version detection
    version = getattr(pproxy, "__version__", None) or "unknown"
    if callable(version):
        version = str(version())

    return {
        "pproxy_version": version,
        "python_version": f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}",
        "module_exports": module_exports,
        "submodules": submodules,
        "protocols": protocols,
        "cipher_info": ciphers,
        "scheduling_algorithms": scheduling,
    }


if __name__ == "__main__":
    snapshot = build_snapshot()
    FIXTURE_PATH.parent.mkdir(parents=True, exist_ok=True)
    FIXTURE_PATH.write_text(json.dumps(snapshot, indent=2) + "\n")
    print(f"Snapshot written to {FIXTURE_PATH}")
    print(f"  pproxy version : {snapshot['pproxy_version']}")
    print(f"  python version : {snapshot['python_version']}")
    print(f"  module exports : {len(snapshot['module_exports'])} names")
    print(f"  protocols      : {len(snapshot['protocols'])} classes")
    print(f"  cipher classes : {len(snapshot['cipher_info'].get('classes', []))} classes")
    print(f"  cipher funcs   : {len(snapshot['cipher_info'].get('functions', []))} functions")
    print(f"  scheduling     : {len(snapshot['scheduling_algorithms'])} names")
