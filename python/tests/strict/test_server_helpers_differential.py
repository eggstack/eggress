"""Paired differential tests for server helper functions.

These tests compare the pproxy oracle server internals against
the eggress candidate implementation.

Tier: 2 (paired API oracle)
Gate: --oracle-observations-dir and --candidate-observations-dir required
"""

import json
import os

import pytest

from .conftest import compare_observations


def load_observation(obs_dir, rid, side):
    """Load an observation JSON file."""
    filename = f"{rid.replace('.', '_')}_{side}.json"
    filepath = os.path.join(str(obs_dir), filename)
    if not os.path.exists(filepath):
        return {"exists": False, "error": f"Observation file not found: {filepath}"}
    with open(filepath) as fh:
        return json.load(fh)


SERVER_HELPERS = [
    ("python.pproxy.server.compile_rule", "compile_rule"),
    ("python.pproxy.server.schedule", "schedule"),
    ("python.pproxy.server.check_server_alive", "check_server_alive"),
    ("python.pproxy.server.prepare_ciphers", "prepare_ciphers"),
    ("python.pproxy.server.stream_handler", "stream_handler"),
    ("python.pproxy.server.datagram_handler", "datagram_handler"),
    ("python.pproxy.server.test_url", "test_url"),
    ("python.pproxy.server.print_server_started", "print_server_started"),
    ("python.pproxy.server.main", "main"),
]

SERVER_CONSTANTS = [
    ("python.pproxy.server.SOCKET_TIMEOUT", "SOCKET_TIMEOUT"),
    ("python.pproxy.server.UDP_LIMIT", "UDP_LIMIT"),
    ("python.pproxy.server.DUMMY", "DUMMY"),
    ("python.pproxy.server.DIRECT", "DIRECT"),
    ("python.pproxy.server.sslcontexts", "sslcontexts"),
]


@pytest.mark.differential
class TestServerHelperExistence:
    """Verify all server helpers exist in the candidate via module attributes."""

    @pytest.mark.parametrize("rid,symbol", SERVER_HELPERS)
    def test_helper_exists(self, rid, symbol, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.server", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.server", "candidate")

        o_attrs = oracle_obs.get("attributes", [])
        c_attrs = candidate_obs.get("attributes", [])
        assert symbol in o_attrs, f"Oracle server module missing {symbol}: {o_attrs[:10]}"
        assert symbol in c_attrs, f"Candidate server module missing {symbol}: {c_attrs[:10]}"


@pytest.mark.differential
class TestServerConstantExistence:
    """Verify all server constants exist in the candidate via module attributes."""

    @pytest.mark.parametrize("rid,symbol", SERVER_CONSTANTS)
    def test_constant_exists(self, rid, symbol, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.server", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.server", "candidate")

        o_attrs = oracle_obs.get("attributes", [])
        c_attrs = candidate_obs.get("attributes", [])
        assert symbol in o_attrs, f"Oracle server module missing constant {symbol}: {o_attrs[:10]}"
        assert symbol in c_attrs, f"Candidate server module missing constant {symbol}: {c_attrs[:10]}"


@pytest.mark.differential
class TestServerHelperSignatures:
    """Verify server helper signatures match the oracle."""

    @pytest.mark.parametrize("rid,symbol", [
        ("python.pproxy.server.compile_rule", "compile_rule"),
        ("python.pproxy.server.check_server_alive", "check_server_alive"),
        ("python.pproxy.server.prepare_ciphers", "prepare_ciphers"),
    ])
    def test_signature_has_params(self, rid, symbol, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        if oracle_obs.get("error") or candidate_obs.get("error"):
            pytest.skip(f"Per-class observation not available for {symbol}")

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

        if oracle_obs.get("error") or candidate_obs.get("error"):
            pytest.skip("Per-class observation not available for compile_rule")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"compile_rule mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )

    def test_compile_rule_not_coroutine(self, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.server.compile_rule", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.server.compile_rule", "candidate")

        if oracle_obs.get("error") or candidate_obs.get("error"):
            pytest.skip("Per-class observation not available for compile_rule")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"compile_rule mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )
