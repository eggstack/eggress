"""Paired differential tests for top-level API exports.

These tests compare the pproxy oracle (pproxy==2.7.9) against the eggress
candidate implementation for top-level namespace exports, class existence,
function signatures, and constant values.

Tier: 2 (paired API oracle)
Gate: --oracle-observations-dir and --candidate-observations-dir required
"""

import json
from pathlib import Path

import pytest


# Top-level module existence tests
TOP_LEVEL_MODULES = [
    ("python.pproxy", "pproxy", "pproxy", "pproxy"),
    ("python.pproxy.server", "pproxy.server", "pproxy", "server"),
    ("python.pproxy.proto", "pproxy.proto", "pproxy", "proto"),
    ("python.pproxy.cipher", "pproxy.cipher", "pproxy", "cipher"),
]

# Top-level class/function/constant exports
TOP_LEVEL_EXPORTS = [
    ("python.pproxy.Connection", "pproxy", "Connection"),
    ("python.pproxy.Server", "pproxy", "Server"),
    ("python.pproxy.server.DIRECT", "pproxy.server", "DIRECT"),
    ("python.pproxy.Rule", "pproxy", "Rule"),
    ("python.pproxy.server.compile_rule", "pproxy.server", "compile_rule"),
    ("python.pproxy.server.proxy_by_uri", "pproxy.server", "proxy_by_uri"),
    ("python.pproxy.server.proxies_by_uri", "pproxy.server", "proxies_by_uri"),
    ("python.pproxy.server.AuthTable", "pproxy.server", "AuthTable"),
    ("python.pproxy.server.ProxySimple", "pproxy.server", "ProxySimple"),
    ("python.pproxy.server.ProxyBackward", "pproxy.server", "ProxyBackward"),
    ("python.pproxy.server.ProxyDirect", "pproxy.server", "ProxyDirect"),
    ("python.pproxy.server.ProxyH2", "pproxy.server", "ProxyH2"),
    ("python.pproxy.server.ProxyQUIC", "pproxy.server", "ProxyQUIC"),
    ("python.pproxy.server.ProxyH3", "pproxy.server", "ProxyH3"),
    ("python.pproxy.server.main", "pproxy.server", "main"),
    ("python.pproxy.server.check_server_alive", "pproxy.server", "check_server_alive"),
    ("python.pproxy.server.prepare_ciphers", "pproxy.server", "prepare_ciphers"),
]


@pytest.mark.differential
class TestTopLevelModuleDifferential:
    """Paired tests for top-level module existence."""

    @pytest.mark.parametrize("rid,module,symbol,name", TOP_LEVEL_MODULES)
    def test_module_exists(self, rid, module, symbol, name, require_obs_dirs):
        # rid is the observation file key, module is the pproxy module path
        del symbol, name
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"Module {module} mismatch: "
            f"{[c for c in result['comparisons'] if not c['match']]}"
        )


@pytest.mark.differential
class TestTopLevelExportDifferential:
    """Paired tests for top-level exports (classes, functions, constants)."""

    @pytest.mark.parametrize("rid,module,symbol", TOP_LEVEL_EXPORTS)
    def test_export_exists_and_type(self, rid, module, symbol, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        result = compare_observations(oracle_obs, candidate_obs)
        assert oracle_obs.get("exists"), f"Oracle: {module}.{symbol} not found: {oracle_obs.get('error')}"
        assert candidate_obs.get("exists"), f"Candidate: {module}.{symbol} not found: {candidate_obs.get('error')}"
        assert result["all_match"], (
            f"{module}.{symbol} mismatch: "
            f"{[c for c in result['comparisons'] if not c['match']]}"
        )

    @pytest.mark.parametrize("rid,module,symbol", [
        ("python.pproxy.server.compile_rule", "pproxy.server", "compile_rule"),
        ("python.pproxy.server.check_server_alive", "pproxy.server", "check_server_alive"),
        ("python.pproxy.server.prepare_ciphers", "pproxy.server", "prepare_ciphers"),
        ("python.pproxy.server.main", "pproxy.server", "main"),
    ])
    def test_function_signature(self, rid, module, symbol, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        assert oracle_obs.get("exists"), f"Oracle: {symbol} not found"
        assert candidate_obs.get("exists"), f"Candidate: {symbol} not found"
        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"{symbol} signature mismatch: "
            f"{[c for c in result['comparisons'] if not c['match']]}"
        )


@pytest.mark.differential
class TestConstantValues:
    """Paired tests for constant values."""

    def test_direct_is_proxy_direct(self, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.server.DIRECT", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.server.DIRECT", "candidate")

        assert oracle_obs.get("exists"), f"Oracle: DIRECT not found"
        assert candidate_obs.get("exists"), f"Candidate: DIRECT not found"

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"DIRECT mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )
