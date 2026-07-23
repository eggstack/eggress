"""Paired differential tests for server helper functions.

These tests compare the pproxy oracle server internals against
the eggress candidate implementation.

Tier: 2 (paired API oracle)
Gate: --oracle-observations-dir and --candidate-observations-dir required
"""

import pytest


SERVER_HELPERS = [
    ("python.pproxy.server.compile_rule", "pproxy.server", "compile_rule"),
    ("python.pproxy.server.schedule", "pproxy.server", "schedule"),
    ("python.pproxy.server.check_server_alive", "pproxy.server", "check_server_alive"),
    ("python.pproxy.server.prepare_ciphers", "pproxy.server", "prepare_ciphers"),
    ("python.pproxy.server.stream_handler", "pproxy.server", "stream_handler"),
    ("python.pproxy.server.datagram_handler", "pproxy.server", "datagram_handler"),
    ("python.pproxy.server.test_url", "pproxy.server", "test_url"),
    ("python.pproxy.server.print_server_started", "pproxy.server", "print_server_started"),
    ("python.pproxy.server.main", "pproxy.server", "main"),
]

SERVER_CONSTANTS = [
    ("python.pproxy.server.SOCKET_TIMEOUT", "pproxy.server", "SOCKET_TIMEOUT"),
    ("python.pproxy.server.UDP_LIMIT", "pproxy.server", "UDP_LIMIT"),
    ("python.pproxy.server.DUMMY", "pproxy.server", "DUMMY"),
    ("python.pproxy.server.DIRECT", "pproxy.server", "DIRECT"),
    ("python.pproxy.server.sslcontexts", "pproxy.server", "sslcontexts"),
]


@pytest.mark.differential
class TestServerHelperExistence:
    """Verify all server helpers exist in the candidate."""

    @pytest.mark.parametrize("rid,module,symbol", SERVER_HELPERS)
    def test_helper_exists(self, rid, module, symbol, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"{module}.{symbol} mismatch: "
            f"{[c for c in result['comparisons'] if not c['match']]}"
        )


@pytest.mark.differential
class TestServerConstantExistence:
    """Verify all server constants exist in the candidate."""

    @pytest.mark.parametrize("rid,module,symbol", SERVER_CONSTANTS)
    def test_constant_exists(self, rid, module, symbol, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"{module}.{symbol} mismatch: "
            f"{[c for c in result['comparisons'] if not c['match']]}"
        )


@pytest.mark.differential
class TestServerHelperSignatures:
    """Verify server helper signatures match the oracle."""

    @pytest.mark.parametrize("rid,module,symbol", [
        ("python.pproxy.server.compile_rule", "pproxy.server", "compile_rule"),
        ("python.pproxy.server.schedule", "pproxy.server", "schedule"),
        ("python.pproxy.server.check_server_alive", "pproxy.server", "check_server_alive"),
        ("python.pproxy.server.prepare_ciphers", "pproxy.server", "prepare_ciphers"),
    ])
    def test_signature_has_params(self, rid, module, symbol, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        # Both should have signature info
        o_sig = oracle_obs.get("signature")
        c_sig = candidate_obs.get("signature")
        assert o_sig is not None, f"Oracle: {symbol} has no signature"
        assert c_sig is not None, f"Candidate: {symbol} has no signature"

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"{symbol} signature mismatch: "
            f"{[c for c in result['comparisons'] if not c['match']]}"
        )


@pytest.mark.differential
class TestCompileRuleCallable:
    """Test that compile_rule returns an oracle-compatible callable."""

    def test_compile_rule_is_function(self, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.server.compile_rule", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.server.compile_rule", "candidate")

        assert oracle_obs.get("exists"), f"Oracle: compile_rule not found"
        assert candidate_obs.get("exists"), f"Candidate: compile_rule not found"

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"compile_rule mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )

    def test_compile_rule_not_coroutine(self, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.server.compile_rule", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.server.compile_rule", "candidate")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"compile_rule mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )
