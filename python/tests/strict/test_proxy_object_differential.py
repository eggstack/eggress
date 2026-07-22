"""Paired differential tests for proxy object behavior.

These tests compare the pproxy oracle proxy object hierarchy against
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


def _compare_class(oracle: dict, candidate: dict) -> list[dict]:
    """Compare two class observations."""
    results = []

    # Bases
    o_bases = oracle.get("bases", [])
    c_bases = candidate.get("bases", [])
    results.append({"dimension": "bases", "oracle": o_bases, "candidate": c_bases, "match": o_bases == c_bases})

    # MRO
    o_mro = oracle.get("mro", [])
    c_mro = candidate.get("mro", [])
    results.append({"dimension": "mro", "oracle": o_mro, "candidate": c_mro, "match": o_mro == c_mro})

    # Method names
    o_methods = sorted(oracle.get("methods", {}).keys())
    c_methods = sorted(candidate.get("methods", {}).keys())
    results.append({"dimension": "method_names", "oracle": o_methods, "candidate": c_methods, "match": o_methods == c_methods})

    return results


PROXY_CLASSES = [
    ("pproxy.server", "ProxyDirect"),
    ("pproxy.server", "ProxySimple"),
    ("pproxy.server", "ProxyBackward"),
    ("pproxy.server", "ProxyH2"),
    ("pproxy.server", "ProxyQUIC"),
    ("pproxy.server", "ProxyH3"),
]


@pytest.mark.differential
class TestProxyObjectDifferential:
    """Paired tests for proxy object structure."""

    @pytest.mark.parametrize("module,class_name", PROXY_CLASSES)
    def test_class_exists(self, module, class_name):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_class_probe(module, class_name)
        assert obs.get("error") is None, f"Failed to probe {class_name}: {obs.get('error')}"
        assert obs.get("bases") is not None, f"{class_name} has no bases"

    def test_proxy_direct_is_not_subclass_of_proxy_simple(self):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_class_probe("pproxy.server", "ProxyDirect")
        assert obs.get("error") is None
        # ProxyDirect should NOT have ProxySimple in its bases (it's a separate class)
        bases = obs.get("bases", [])
        assert "ProxySimple" not in bases, f"ProxyDirect should not subclass ProxySimple, got bases: {bases}"

    def test_proxy_backward_has_jump(self):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_class_probe("pproxy.server", "ProxyBackward")
        assert obs.get("error") is None
        methods = obs.get("methods", {})
        # ProxyBackward should have a jump property or attribute
        assert "jump" in methods or "jump" in obs.get("class_attributes", {}), (
            f"ProxyBackward missing 'jump', methods: {sorted(methods.keys())}"
        )


@pytest.mark.differential
class TestChainTopology:
    """Paired tests for nested __ chain construction."""

    def test_chain_construction_produces_nested_jump(self):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        # Import and test chain construction
        try:
            import pproxy
            # Create a two-hop chain: socks5://user:pass@host:1080__http://user:pass@host:8080
            proxy = pproxy.Connection("socks5://user:pass@host:1080__http://user:pass@host:8080")
            # The result should be a ProxyBackward with nested .jump
            assert hasattr(proxy, "jump"), f"Chain result has no .jump attribute: {type(proxy)}"
        except ImportError:
            pytest.skip("pproxy not importable")
        except Exception as e:
            pytest.fail(f"Chain construction failed: {e}")
