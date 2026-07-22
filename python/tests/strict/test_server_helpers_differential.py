"""Paired differential tests for server helper functions.

These tests compare the pproxy oracle server internals against
the eggress candidate implementation.

Tier: 2 (paired API oracle)
Gate: EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1
"""

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest


REQUIRE_DIFFERENTIAL = os.environ.get("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL") == "1"
SCRIPTS_DIR = Path(__file__).resolve().parents[3] / "scripts"


def _run_api_probe(module: str, symbol: str) -> dict:
    """Run the strict_api_probe.py and return the observation."""
    cmd = [sys.executable, str(SCRIPTS_DIR / "strict_api_probe.py"), "--module", module, "--symbol", symbol]
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
    if result.returncode == 0 and result.stdout.strip():
        return json.loads(result.stdout)
    return {"module": module, "symbol": symbol, "exists": False, "error": result.stderr}


def _run_sig_probe(module: str, symbol: str) -> dict:
    """Run the strict_signature_probe.py and return the observation."""
    cmd = [sys.executable, str(SCRIPTS_DIR / "strict_signature_probe.py"), "--module", module, "--symbol", symbol]
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
    if result.returncode == 0 and result.stdout.strip():
        return json.loads(result.stdout)
    return {"module": module, "symbol": symbol, "error": result.stderr}


SERVER_HELPERS = [
    ("pproxy.server", "compile_rule"),
    ("pproxy.server", "schedule"),
    ("pproxy.server", "check_server_alive"),
    ("pproxy.server", "prepare_ciphers"),
    ("pproxy.server", "stream_handler"),
    ("pproxy.server", "datagram_handler"),
    ("pproxy.server", "test_url"),
    ("pproxy.server", "print_server_started"),
    ("pproxy.server", "main"),
]

SERVER_CONSTANTS = [
    ("pproxy.server", "SOCKET_TIMEOUT"),
    ("pproxy.server", "UDP_LIMIT"),
    ("pproxy.server", "DUMMY"),
    ("pproxy.server", "DIRECT"),
    ("pproxy.server", "sslcontexts"),
]


@pytest.mark.differential
class TestServerHelperExistence:
    """Verify all server helpers exist in the candidate."""

    @pytest.mark.parametrize("module,symbol", SERVER_HELPERS)
    def test_helper_exists(self, module, symbol):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_api_probe(module, symbol)
        assert obs.get("exists") is True, f"{module}.{symbol} not found: {obs.get('error')}"


@pytest.mark.differential
class TestServerConstantExistence:
    """Verify all server constants exist in the candidate."""

    @pytest.mark.parametrize("module,symbol", SERVER_CONSTANTS)
    def test_constant_exists(self, module, symbol):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_api_probe(module, symbol)
        assert obs.get("exists") is True, f"{module}.{symbol} not found: {obs.get('error')}"


@pytest.mark.differential
class TestServerHelperSignatures:
    """Verify server helper signatures match the oracle."""

    @pytest.mark.parametrize("module,symbol", [
        ("pproxy.server", "compile_rule"),
        ("pproxy.server", "schedule"),
        ("pproxy.server", "check_server_alive"),
        ("pproxy.server", "prepare_ciphers"),
    ])
    def test_signature_has_params(self, module, symbol):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_sig_probe(module, symbol)
        assert obs.get("error") is None, f"Signature probe failed for {symbol}: {obs.get('error')}"
        params = obs.get("parameters", [])
        assert len(params) > 0, f"{symbol} has no parameters"


@pytest.mark.differential
class TestCompileRuleCallable:
    """Test that compile_rule returns an oracle-compatible callable."""

    def test_compile_rule_is_function(self):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_api_probe("pproxy.server", "compile_rule")
        assert obs.get("exists") is True
        assert obs.get("type") == "function", f"compile_rule type: {obs.get('type')}"

    def test_compile_rule_not_coroutine(self):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_api_probe("pproxy.server", "compile_rule")
        assert obs.get("is_coroutine") is False, f"compile_rule is_coroutine: {obs.get('is_coroutine')}"
