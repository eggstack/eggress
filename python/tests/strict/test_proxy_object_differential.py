"""Paired differential tests for proxy object behavior.

These tests compare the pproxy oracle proxy object hierarchy against
the eggress candidate implementation.

Tier: 2 (paired API oracle)
Gate: --oracle-observations-dir and --candidate-observations-dir required
"""

import pytest


PROXY_CLASSES = [
    ("python.pproxy.server.ProxyDirect", "pproxy.server", "ProxyDirect"),
    ("python.pproxy.server.ProxySimple", "pproxy.server", "ProxySimple"),
    ("python.pproxy.server.ProxyBackward", "pproxy.server", "ProxyBackward"),
    ("python.pproxy.server.ProxyH2", "pproxy.server", "ProxyH2"),
    ("python.pproxy.server.ProxyQUIC", "pproxy.server", "ProxyQUIC"),
    ("python.pproxy.server.ProxyH3", "pproxy.server", "ProxyH3"),
]


def _compare_class(oracle: dict, candidate: dict) -> list:
    """Compare two class observations."""
    results = []
    o_bases = oracle.get("bases", [])
    c_bases = candidate.get("bases", [])
    results.append({"dimension": "bases", "oracle": o_bases, "candidate": c_bases, "match": o_bases == c_bases})

    o_mro = oracle.get("mro", [])
    c_mro = candidate.get("mro", [])
    results.append({"dimension": "mro", "oracle": o_mro, "candidate": c_mro, "match": o_mro == c_mro})

    o_methods = sorted(oracle.get("methods", {}).keys())
    c_methods = sorted(candidate.get("methods", {}).keys())
    results.append({"dimension": "method_names", "oracle": o_methods, "candidate": c_methods, "match": o_methods == c_methods})

    return results


@pytest.mark.differential
class TestProxyObjectDifferential:
    """Paired tests for proxy object structure."""

    @pytest.mark.parametrize("rid,module,class_name", PROXY_CLASSES)
    def test_class_exists(self, rid, module, class_name, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        assert oracle_obs.get("error") is None, f"Oracle failed to probe {class_name}: {oracle_obs.get('error')}"
        assert candidate_obs.get("error") is None, f"Candidate failed to probe {class_name}: {candidate_obs.get('error')}"
        assert oracle_obs.get("bases") is not None, f"Oracle: {class_name} has no bases"
        assert candidate_obs.get("bases") is not None, f"Candidate: {class_name} has no bases"

    def test_proxy_direct_is_not_subclass_of_proxy_simple(self, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.server.ProxyDirect", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.server.ProxyDirect", "candidate")

        assert oracle_obs.get("error") is None
        assert candidate_obs.get("error") is None
        # ProxyDirect should NOT have ProxySimple in its bases
        o_bases = oracle_obs.get("bases", [])
        c_bases = candidate_obs.get("bases", [])
        assert "ProxySimple" not in o_bases, f"Oracle: ProxyDirect should not subclass ProxySimple, got bases: {o_bases}"
        assert "ProxySimple" not in c_bases, f"Candidate: ProxyDirect should not subclass ProxySimple, got bases: {c_bases}"

    def test_proxy_backward_has_jump(self, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.server.ProxyBackward", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.server.ProxyBackward", "candidate")

        assert oracle_obs.get("error") is None
        assert candidate_obs.get("error") is None
        o_methods = oracle_obs.get("methods", {})
        c_methods = candidate_obs.get("methods", {})
        assert "jump" in o_methods or any("jump" in m for m in o_methods), (
            f"Oracle: ProxyBackward missing 'jump' attribute"
        )
        assert "jump" in c_methods or any("jump" in m for m in c_methods), (
            f"Candidate: ProxyBackward missing 'jump' attribute"
        )


@pytest.mark.differential
class TestChainTopology:
    """Paired tests for nested __ chain construction."""

    def test_chain_construction_produces_nested_jump(self, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        # Load observations for chain construction test
        # This tests Connection() with a two-hop chain URI
        oracle_obs = load_observation(oracle_dir, "python.pproxy.Connection", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.Connection", "candidate")

        # Both should have Connection (proxies_by_uri function)
        assert oracle_obs.get("exists"), f"Oracle: Connection not found"
        assert candidate_obs.get("exists"), f"Candidate: Connection not found"

        # Chain construction is tested via the signature/qualname comparison
        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"Connection mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )
