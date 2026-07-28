"""Chain topology scenario tests (CE1).

Validates that the candidate ProxySimple topology matches the frozen oracle
observations in compat/pproxy-2.7.9/observations/chain_topology.json.

These tests verify object type, endpoint, .jump type, protocol list,
and destination() output for single-hop and two-hop URI orientations.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

_OBSERVATIONS_PATH = (
    Path(__file__).resolve().parents[3]
    / "compat"
    / "pproxy-2.7.9"
    / "observations"
    / "chain_topology.json"
)


@pytest.fixture(scope="module")
def oracle_topology():
    """Load the frozen oracle topology observations."""
    if not _OBSERVATIONS_PATH.exists():
        pytest.skip(f"Oracle observations not found: {_OBSERVATIONS_PATH}")
    with open(_OBSERVATIONS_PATH) as fh:
        data = json.load(fh)
    return data["topology"]


@pytest.fixture(scope="module")
def oracle_wire_events():
    """Load the frozen oracle wire event observations."""
    if not _OBSERVATIONS_PATH.exists():
        pytest.skip(f"Oracle observations not found: {_OBSERVATIONS_PATH}")
    with open(_OBSERVATIONS_PATH) as fh:
        data = json.load(fh)
    return data["wire_events"]


def _build_candidate(uri: str):
    """Build a candidate ProxySimple from a pproxy-style URI.

    Uses proxies_by_uri (matching the oracle probe) rather than proxy_by_uri
    which does not correctly parse two-hop URIs.
    """
    from pproxy.server import proxies_by_uri

    return proxies_by_uri(uri)


class TestSingleHopTopology:
    """Verify single-hop URI topology matches oracle observations."""

    def test_http_single_hop_type(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "http://127.0.0.1:18080")
        proxy = _build_candidate("http://127.0.0.1:18080")
        assert type(proxy).__name__ == obs["type"]

    def test_http_single_hop_host(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "http://127.0.0.1:18080")
        proxy = _build_candidate("http://127.0.0.1:18080")
        assert proxy.host_name == obs["host_name"]

    def test_http_single_hop_port(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "http://127.0.0.1:18080")
        proxy = _build_candidate("http://127.0.0.1:18080")
        assert proxy.port == obs["port"]

    def test_http_single_hop_protos(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "http://127.0.0.1:18080")
        proxy = _build_candidate("http://127.0.0.1:18080")
        candidate_protos = [p.name for p in proxy.protos]
        assert candidate_protos == obs["protos"]

    def test_http_single_hop_jump_type(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "http://127.0.0.1:18080")
        proxy = _build_candidate("http://127.0.0.1:18080")
        assert type(proxy.jump).__name__ == obs["jump"]["type"]

    def test_http_single_hop_jump_direct(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "http://127.0.0.1:18080")
        proxy = _build_candidate("http://127.0.0.1:18080")
        assert proxy.jump.direct is obs["jump"]["direct"]

    def test_socks5_single_hop_type(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "socks5://127.0.0.1:11080")
        proxy = _build_candidate("socks5://127.0.0.1:11080")
        assert type(proxy).__name__ == obs["type"]

    def test_socks5_single_hop_host(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "socks5://127.0.0.1:11080")
        proxy = _build_candidate("socks5://127.0.0.1:11080")
        assert proxy.host_name == obs["host_name"]

    def test_socks5_single_hop_port(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "socks5://127.0.0.1:11080")
        proxy = _build_candidate("socks5://127.0.0.1:11080")
        assert proxy.port == obs["port"]

    def test_socks5_single_hop_protos(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "socks5://127.0.0.1:11080")
        proxy = _build_candidate("socks5://127.0.0.1:11080")
        candidate_protos = [p.name for p in proxy.protos]
        assert candidate_protos == obs["protos"]

    def test_socks5_single_hop_jump_type(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "socks5://127.0.0.1:11080")
        proxy = _build_candidate("socks5://127.0.0.1:11080")
        assert type(proxy.jump).__name__ == obs["jump"]["type"]


class TestTwoHopTopology:
    """Verify two-hop URI topology matches oracle observations."""

    def test_socks5_then_http_outer_type(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        proxy = _build_candidate("socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        assert type(proxy).__name__ == obs["type"]

    def test_socks5_then_http_outer_host(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        proxy = _build_candidate("socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        assert proxy.host_name == obs["host_name"]

    def test_socks5_then_http_outer_port(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        proxy = _build_candidate("socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        assert proxy.port == obs["port"]

    def test_socks5_then_http_inner_type(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        proxy = _build_candidate("socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        assert type(proxy.jump).__name__ == obs["jump"]["type"]

    def test_socks5_then_http_inner_host(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        proxy = _build_candidate("socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        assert proxy.jump.host_name == obs["jump"]["host_name"]

    def test_socks5_then_http_inner_port(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        proxy = _build_candidate("socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        assert proxy.jump.port == obs["jump"]["port"]

    def test_socks5_then_http_inner_protos(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        proxy = _build_candidate("socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        candidate_protos = [p.name for p in proxy.jump.protos]
        assert candidate_protos == obs["jump"]["protos"]

    def test_http_then_socks5_outer_type(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "http://127.0.0.1:18080__socks5://127.0.0.1:11080")
        proxy = _build_candidate("http://127.0.0.1:18080__socks5://127.0.0.1:11080")
        assert type(proxy).__name__ == obs["type"]

    def test_http_then_socks5_outer_host(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "http://127.0.0.1:18080__socks5://127.0.0.1:11080")
        proxy = _build_candidate("http://127.0.0.1:18080__socks5://127.0.0.1:11080")
        assert proxy.host_name == obs["host_name"]

    def test_http_then_socks5_outer_port(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "http://127.0.0.1:18080__socks5://127.0.0.1:11080")
        proxy = _build_candidate("http://127.0.0.1:18080__socks5://127.0.0.1:11080")
        assert proxy.port == obs["port"]

    def test_http_then_socks5_inner_type(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "http://127.0.0.1:18080__socks5://127.0.0.1:11080")
        proxy = _build_candidate("http://127.0.0.1:18080__socks5://127.0.0.1:11080")
        assert type(proxy.jump).__name__ == obs["jump"]["type"]

    def test_http_then_socks5_inner_host(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "http://127.0.0.1:18080__socks5://127.0.0.1:11080")
        proxy = _build_candidate("http://127.0.0.1:18080__socks5://127.0.0.1:11080")
        assert proxy.jump.host_name == obs["jump"]["host_name"]

    def test_http_then_socks5_inner_port(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "http://127.0.0.1:18080__socks5://127.0.0.1:11080")
        proxy = _build_candidate("http://127.0.0.1:18080__socks5://127.0.0.1:11080")
        assert proxy.jump.port == obs["jump"]["port"]


class TestDestinationBehavior:
    """Verify destination() output matches oracle observations."""

    def test_single_hop_destination(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "http://127.0.0.1:18080")
        proxy = _build_candidate("http://127.0.0.1:18080")
        dest = proxy.destination("example.com", 443)
        assert list(dest) == list(obs["destination"])

    def test_single_hop_jump_destination(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "http://127.0.0.1:18080")
        proxy = _build_candidate("http://127.0.0.1:18080")
        dest = proxy.jump.destination("example.com", 443)
        assert list(dest) == list(obs["jump_destination"])

    def test_two_hop_destination(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        proxy = _build_candidate("socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        dest = proxy.destination("example.com", 443)
        assert list(dest) == list(obs["destination"])

    def test_two_hop_jump_destination(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        proxy = _build_candidate("socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        dest = proxy.jump.destination("example.com", 443)
        assert list(dest) == list(obs["jump_destination"])

    def test_two_hop_jump_jump_destination(self, oracle_topology):
        obs = next(o for o in oracle_topology if o["uri"] == "socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        proxy = _build_candidate("socks5://127.0.0.1:11080__http://127.0.0.1:18080")
        dest = proxy.jump.jump.destination("example.com", 443)
        assert list(dest) == list(obs["jump_jump_destination"])
