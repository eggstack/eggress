"""Paired differential tests for protocol classes.

These tests compare the pproxy oracle protocol class hierarchy against
the eggress candidate implementation.

Tier: 2 (paired API oracle)
Gate: --oracle-observations-dir and --candidate-observations-dir required
"""

import pytest


PROTOCOL_CLASSES = [
    ("python.pproxy.proto.Direct", "pproxy.proto", "Direct"),
    ("python.pproxy.proto.HTTP", "pproxy.proto", "HTTP"),
    ("python.pproxy.proto.HTTPOnly", "pproxy.proto", "HTTPOnly"),
    ("python.pproxy.proto.Socks4", "pproxy.proto", "Socks4"),
    ("python.pproxy.proto.Socks5", "pproxy.proto", "Socks5"),
    ("python.pproxy.proto.SS", "pproxy.proto", "SS"),
    ("python.pproxy.proto.Trojan", "pproxy.proto", "Trojan"),
    ("python.pproxy.proto.Echo", "pproxy.proto", "Echo"),
]


@pytest.mark.differential
class TestProtocolClassExistence:
    """Verify protocol classes exist in the candidate."""

    @pytest.mark.parametrize("rid,module,class_name", PROTOCOL_CLASSES)
    def test_class_exists(self, rid, module, class_name, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        assert oracle_obs.get("error") is None, f"Oracle failed to probe {class_name}: {oracle_obs.get('error')}"
        assert candidate_obs.get("error") is None, f"Candidate failed to probe {class_name}: {candidate_obs.get('error')}"
        assert oracle_obs.get("bases") is not None, f"Oracle: {class_name} has no bases"
        assert candidate_obs.get("bases") is not None, f"Candidate: {class_name} has no bases"


@pytest.mark.differential
class TestProtocolClassStructure:
    """Verify protocol class structure matches oracle."""

    @pytest.mark.parametrize("rid,module,class_name", [
        ("python.pproxy.proto.Direct", "pproxy.proto", "Direct"),
        ("python.pproxy.proto.HTTP", "pproxy.proto", "HTTP"),
        ("python.pproxy.proto.Socks5", "pproxy.proto", "Socks5"),
    ])
    def test_has_guess_method(self, rid, module, class_name, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        assert oracle_obs.get("error") is None
        assert candidate_obs.get("error") is None
        o_methods = oracle_obs.get("methods", {})
        c_methods = candidate_obs.get("methods", {})
        assert "guess" in o_methods, f"Oracle: {class_name} missing 'guess' method"
        assert "guess" in c_methods, f"Candidate: {class_name} missing 'guess' method"

    @pytest.mark.parametrize("rid,module,class_name", [
        ("python.pproxy.proto.Direct", "pproxy.proto", "Direct"),
        ("python.pproxy.proto.HTTP", "pproxy.proto", "HTTP"),
        ("python.pproxy.proto.Socks5", "pproxy.proto", "Socks5"),
    ])
    def test_has_accept_method(self, rid, module, class_name, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        assert oracle_obs.get("error") is None
        assert candidate_obs.get("error") is None
        o_methods = oracle_obs.get("methods", {})
        c_methods = candidate_obs.get("methods", {})
        assert "accept" in o_methods, f"Oracle: {class_name} missing 'accept' method"
        assert "accept" in c_methods, f"Candidate: {class_name} missing 'accept' method"

    @pytest.mark.parametrize("rid,module,class_name", [
        ("python.pproxy.proto.Direct", "pproxy.proto", "Direct"),
        ("python.pproxy.proto.HTTP", "pproxy.proto", "HTTP"),
        ("python.pproxy.proto.Socks5", "pproxy.proto", "Socks5"),
    ])
    def test_has_connect_method(self, rid, module, class_name, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        assert oracle_obs.get("error") is None
        assert candidate_obs.get("error") is None
        o_methods = oracle_obs.get("methods", {})
        c_methods = candidate_obs.get("methods", {})
        assert "connect" in o_methods, f"Oracle: {class_name} missing 'connect' method"
        assert "connect" in c_methods, f"Candidate: {class_name} missing 'connect' method"


@pytest.mark.differential
class TestProtocolMAPPINGS:
    """Verify the MAPPINGS registry is present and populated."""

    def test_mappings_exists(self, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.proto.MAPPINGS", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.proto.MAPPINGS", "candidate")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"MAPPINGS mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )

    def test_mappings_has_direct(self, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.proto.MAPPINGS", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.proto.MAPPINGS", "candidate")

        # Both should have MAPPINGS with 'direct' or '' key
        for label, obs in [("oracle", oracle_obs), ("candidate", candidate_obs)]:
            if obs.get("exists"):
                attrs = obs.get("attributes", [])
                assert "direct" in attrs or "" in attrs, (
                    f"{label} MAPPINGS missing direct: {attrs[:10]}"
                )
