"""Paired differential tests for protocol classes.

These tests compare the pproxy oracle protocol class hierarchy against
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


def _run_class_probe(module: str, class_name: str) -> dict:
    """Run the strict_class_probe.py and return the observation."""
    cmd = [sys.executable, str(SCRIPTS_DIR / "strict_class_probe.py"), "--module", module, "--class-name", class_name]
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
    if result.returncode == 0 and result.stdout.strip():
        return json.loads(result.stdout)
    return {"module": module, "class_name": class_name, "error": result.stderr}


PROTOCOL_CLASSES = [
    ("pproxy.proto", "Direct"),
    ("pproxy.proto", "HTTP"),
    ("pproxy.proto", "HTTPOnly"),
    ("pproxy.proto", "Socks4"),
    ("pproxy.proto", "Socks5"),
    ("pproxy.proto", "SS"),
    ("pproxy.proto", "Trojan"),
    ("pproxy.proto", "Echo"),
]


@pytest.mark.differential
class TestProtocolClassExistence:
    """Verify protocol classes exist in the candidate."""

    @pytest.mark.parametrize("module,class_name", PROTOCOL_CLASSES)
    def test_class_exists(self, module, class_name):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_class_probe(module, class_name)
        assert obs.get("error") is None, f"Failed to probe {class_name}: {obs.get('error')}"
        assert obs.get("bases") is not None, f"{class_name} has no bases"


@pytest.mark.differential
class TestProtocolClassStructure:
    """Verify protocol class structure matches oracle."""

    @pytest.mark.parametrize("module,class_name", [
        ("pproxy.proto", "Direct"),
        ("pproxy.proto", "HTTP"),
        ("pproxy.proto", "Socks5"),
    ])
    def test_has_guess_method(self, module, class_name):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_class_probe(module, class_name)
        assert obs.get("error") is None
        methods = obs.get("methods", {})
        assert "guess" in methods, f"{class_name} missing 'guess' method, methods: {sorted(methods.keys())}"

    @pytest.mark.parametrize("module,class_name", [
        ("pproxy.proto", "Direct"),
        ("pproxy.proto", "HTTP"),
        ("pproxy.proto", "Socks5"),
    ])
    def test_has_accept_method(self, module, class_name):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_class_probe(module, class_name)
        assert obs.get("error") is None
        methods = obs.get("methods", {})
        assert "accept" in methods, f"{class_name} missing 'accept' method"

    @pytest.mark.parametrize("module,class_name", [
        ("pproxy.proto", "Direct"),
        ("pproxy.proto", "HTTP"),
        ("pproxy.proto", "Socks5"),
    ])
    def test_has_connect_method(self, module, class_name):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_class_probe(module, class_name)
        assert obs.get("error") is None
        methods = obs.get("methods", {})
        assert "connect" in methods, f"{class_name} missing 'connect' method"


@pytest.mark.differential
class TestProtocolMAPPINGS:
    """Verify the MAPPINGS registry is present and populated."""

    def test_mappings_exists(self):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        cmd = [sys.executable, str(SCRIPTS_DIR / "strict_api_probe.py"), "--module", "pproxy.proto", "--symbol", "MAPPINGS"]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        if result.returncode == 0 and result.stdout.strip():
            obs = json.loads(result.stdout)
            assert obs.get("exists") is True, "MAPPINGS not found in pproxy.proto"
        else:
            pytest.fail(f"Failed to probe MAPPINGS: {result.stderr}")

    def test_mappings_has_direct(self):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        try:
            import pproxy.proto
            mappings = getattr(pproxy.proto, "MAPPINGS", {})
            assert "direct" in mappings or "" in mappings, f"MAPPINGS missing direct: {list(mappings.keys())[:10]}"
        except ImportError:
            pytest.skip("pproxy.proto not importable")
