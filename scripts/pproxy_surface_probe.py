#!/usr/bin/env python3
"""Emit a small, deterministic inventory of an installed pproxy namespace.

The probe is intentionally dependency-free and does not execute proxy
operations. Run it once with a clean ``pproxy==2.7.9`` environment and once
with the Eggress wheel environment, then compare the resulting JSON. The
``--package-root`` option is useful for inspecting a checked-out package
without installing it; ``--python`` can point at an isolated interpreter.
"""

from __future__ import annotations

import argparse
import importlib
import inspect
import json
import pkgutil
import sys
from pathlib import Path
from typing import Any


TRACKED_SYMBOLS = {
    "pproxy": ("Connection", "DIRECT", "Rule", "Server"),
    "pproxy.proto": (
        "BaseProtocol", "Direct", "HTTP", "HTTPOnly", "H2", "H3", "SS",
        "SSR", "SSH", "Socks4", "Socks5", "Trojan", "WS", "accept",
        "get_protos", "netloc_split", "packstr", "socks_address_stream",
        "udp_accept",
    ),
    "pproxy.server": (
        "AuthTable", "ProxyBackward", "ProxyDirect", "ProxyH2", "ProxyH3",
        "ProxyQUIC", "ProxySSH", "ProxySimple", "compile_rule", "main",
        "proxies_by_uri", "proxy_by_uri", "schedule", "stream_handler",
        "datagram_handler", "test_url",
    ),
    "pproxy.cipher": ("AEADCipher", "PacketCipher", "get_cipher"),
    "pproxy.cipherpy": ("AEADCipher", "get_cipher"),
    "pproxy.plugin": (
        "BasePlugin", "Plain_Plugin", "Origin_Plugin", "Http_Simple_Plugin",
        "Tls1__2_Ticket_Auth_Plugin", "Verify_Simple_Plugin",
        "Verify_Deflate_Plugin", "get_plugin",
    ),
    "pproxy.verbose": ("all_stat", "all_stat_other", "realtime_stat", "setup"),
    "pproxy.sysproxy": ("MacSetting", "WindowsSetting", "setup"),
}


def _signature(value: Any) -> str | None:
    try:
        return str(inspect.signature(value))
    except (TypeError, ValueError):
        return None


def _symbol_observation(module_name: str, name: str, value: Any) -> dict[str, Any]:
    observation: dict[str, Any] = {
        "module": module_name,
        "name": name,
        "kind": "class" if inspect.isclass(value) else "callable" if callable(value) else "value",
        "signature": _signature(value) if callable(value) else None,
        "async": inspect.iscoroutinefunction(value),
    }
    if inspect.isclass(value):
        observation["bases"] = [base.__name__ for base in value.__bases__]
    return observation


def _module_names(package: Any) -> list[str]:
    names = [package.__name__]
    package_path = getattr(package, "__path__", ())
    names.extend(info.name for info in pkgutil.walk_packages(package_path, package.__name__ + "."))
    return sorted(set(names))


def build_inventory(package_name: str = "pproxy", package_root: Path | None = None) -> dict[str, Any]:
    if package_root is not None:
        sys.path.insert(0, str(package_root))

    package = importlib.import_module(package_name)
    module_names = _module_names(package)
    modules: list[dict[str, Any]] = []
    symbols: dict[str, dict[str, Any]] = {}

    for module_name in module_names:
        try:
            module = importlib.import_module(module_name)
            exports = sorted(name for name in dir(module) if not name.startswith("_"))
            import_error = None
        except Exception as exc:  # A missing optional dependency is inventory data.
            exports = []
            import_error = f"{type(exc).__name__}: {exc}"
            module = None
        record: dict[str, Any] = {"name": module_name, "exports": exports}
        if import_error:
            record["import_error"] = import_error
        modules.append(record)

        for symbol_name in TRACKED_SYMBOLS.get(module_name, ()):
            if module is None or not hasattr(module, symbol_name):
                symbols[f"{module_name}.{symbol_name}"] = {
                    "module": module_name,
                    "name": symbol_name,
                    "missing": True,
                }
                continue
            symbols[f"{module_name}.{symbol_name}"] = _symbol_observation(
                module_name, symbol_name, getattr(module, symbol_name)
            )

    version = getattr(package, "__version__", "unknown")
    return {
        "schema": "pproxy-surface-v1",
        "package": package_name,
        "version": str(version),
        "python": ".".join(str(part) for part in sys.version_info[:3]),
        "modules": modules,
        "top_level_exports": next(
            (module["exports"] for module in modules if module["name"] == package_name), []
        ),
        "tracked_symbols": symbols,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", default="pproxy")
    parser.add_argument("--package-root", type=Path)
    args = parser.parse_args()
    json.dump(build_inventory(args.package, args.package_root), sys.stdout, indent=2, sort_keys=True)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
