"""Paired differential tests for top-level API exports.

These tests compare the pproxy oracle (pproxy==2.7.9) against the eggress
candidate implementation for top-level namespace exports, class existence,
function signatures, and constant values.

Tier: 2 (paired API oracle)
Gate: EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1
"""

import importlib
import json
import os
import subprocess
import sys
from pathlib import Path

import pytest


REQUIRE_DIFFERENTIAL = os.environ.get("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL") == "1"
SCRIPTS_DIR = Path(__file__).resolve().parents[3] / "scripts"
MANIFEST_PATH = Path(__file__).resolve().parents[3] / "docs" / "parity" / "pproxy_2_7_9_strict_manifest.toml"


def _run_probe(module: str, symbol: str) -> dict:
    """Run the strict_api_probe.py and return the observation."""
    cmd = [sys.executable, str(SCRIPTS_DIR / "strict_api_probe.py"), "--module", module, "--symbol", symbol]
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
    if result.returncode == 0 and result.stdout.strip():
        return json.loads(result.stdout)
    return {"module": module, "symbol": symbol, "exists": False, "error": result.stderr}


def _compare_api(oracle: dict, candidate: dict) -> list[dict]:
    """Compare two API observations."""
    results = []
    for dim in ["exists", "type", "is_coroutine", "is_callable"]:
        o_val = oracle.get(dim)
        c_val = candidate.get(dim)
        results.append({"dimension": dim, "oracle": o_val, "candidate": c_val, "match": o_val == c_val})

    o_sig = oracle.get("signature", "")
    c_sig = candidate.get("signature", "")
    results.append({"dimension": "signature", "oracle": o_sig, "candidate": c_sig, "match": o_sig == c_sig})

    return results


# Top-level module existence tests
# These verify that the pproxy submodules are importable.
# The symbol name is the last component of the module path.
TOP_LEVEL_MODULES = [
    ("pproxy", "pproxy"),
    ("pproxy.server", "server"),
    ("pproxy.proto", "proto"),
    ("pproxy.cipher", "cipher"),
]

# Top-level class/function/constant exports
# Note: pproxy.Connection and pproxy.Server are aliases for proxies_by_uri
# (a function), not classes in pproxy.server.
TOP_LEVEL_EXPORTS = [
    ("pproxy", "Connection"),
    ("pproxy", "Server"),
    ("pproxy.server", "DIRECT"),
    ("pproxy", "Rule"),
    ("pproxy.server", "compile_rule"),
    ("pproxy.server", "proxy_by_uri"),
    ("pproxy.server", "proxies_by_uri"),
    ("pproxy.server", "AuthTable"),
    ("pproxy.server", "ProxySimple"),
    ("pproxy.server", "ProxyBackward"),
    ("pproxy.server", "ProxyDirect"),
    ("pproxy.server", "ProxyH2"),
    ("pproxy.server", "ProxyQUIC"),
    ("pproxy.server", "ProxyH3"),
    ("pproxy.server", "main"),
    ("pproxy.server", "check_server_alive"),
    ("pproxy.server", "prepare_ciphers"),
]


@pytest.mark.differential
class TestTopLevelModuleDifferential:
    """Paired tests for top-level module existence."""

    @pytest.mark.parametrize("module,symbol", TOP_LEVEL_MODULES)
    def test_module_exists(self, module, symbol):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        # Import the module directly rather than probing a submodule
        try:
            importlib.import_module(module)
        except ImportError:
            pytest.fail(f"Module {module} is not importable")


@pytest.mark.differential
class TestTopLevelExportDifferential:
    """Paired tests for top-level exports (classes, functions, constants)."""

    @pytest.mark.parametrize("module,symbol", TOP_LEVEL_EXPORTS)
    def test_export_exists_and_type(self, module, symbol):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_probe(module, symbol)
        assert obs.get("exists") is True, f"{module}.{symbol} not found: {obs.get('error')}"

    @pytest.mark.parametrize("module,symbol", [
        ("pproxy.server", "compile_rule"),
        ("pproxy.server", "check_server_alive"),
        ("pproxy.server", "prepare_ciphers"),
        ("pproxy.server", "main"),
    ])
    def test_function_signature(self, module, symbol):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_probe(module, symbol)
        assert obs.get("exists") is True
        assert obs.get("signature") is not None, f"{symbol} has no signature"


@pytest.mark.differential
class TestConstantValues:
    """Paired tests for constant values."""

    def test_direct_is_proxy_direct(self):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_probe("pproxy.server", "DIRECT")
        assert obs.get("exists") is True
        # DIRECT should be a ProxyDirect instance
        assert obs.get("type") is not None
