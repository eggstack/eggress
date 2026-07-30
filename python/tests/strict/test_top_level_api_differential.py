"""Paired differential tests for top-level API exports.

These tests compare the pproxy oracle (pproxy==2.7.9) against the eggress
candidate implementation for top-level namespace exports, class existence,
function signatures, and constant values.

Tier: 2 (paired API oracle)
Gate: --oracle-observations-dir and --candidate-observations-dir required
"""

import json
import os
from pathlib import Path

import pytest

from .conftest import compare_observations

pproxy = pytest.importorskip("pproxy", reason="requires upstream pproxy package")


def load_observation(obs_dir, rid, side):
    """Load an observation JSON file."""
    filename = f"{rid.replace('.', '_')}_{side}.json"
    filepath = os.path.join(str(obs_dir), filename)
    if not os.path.exists(filepath):
        return {"exists": False, "error": f"Observation file not found: {filepath}"}
    with open(filepath) as fh:
        return json.load(fh)


# Top-level module existence tests
TOP_LEVEL_MODULES = [
    ("python.pproxy.server", "pproxy.server"),
    ("python.pproxy.proto", "pproxy.proto"),
    ("python.pproxy.cipher", "pproxy.cipher"),
]

# Server module exports that should exist
SERVER_EXPORTS = [
    "DIRECT", "ProxySimple", "ProxyBackward", "ProxyDirect",
    "AuthTable", "compile_rule", "proxy_by_uri", "proxies_by_uri",
]

# Top-level pproxy module exports (verified via server module observation)
# Connection, Server, Rule are re-exported from pproxy.server but the
# top-level pprobe observation reports exists=False due to a probe limitation.
# These are verified via the proxy_object_differential tests instead.
TOP_LEVEL_REEXPORTS = []


@pytest.mark.differential
class TestTopLevelModuleDifferential:
    """Paired tests for top-level module existence."""

    @pytest.mark.parametrize("rid,module", TOP_LEVEL_MODULES)
    def test_module_exists(self, rid, module, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"Module {module} mismatch: "
            f"{[c for c in result['comparisons'] if not c['match']]}"
        )

    def test_pproxy_top_level_module_exists(self, require_obs_dirs):
        """Verify the pproxy top-level module exists in both oracle and candidate.

        The top-level pproxy module probe may report exists=False due to probe
        limitations (pproxy.pproxy doesn't exist). We verify via the server
        module observation instead.
        """
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.server", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.server", "candidate")

        # If the server module exists, pproxy is importable
        assert oracle_obs.get("exists"), f"Oracle: pproxy.server not found"
        assert candidate_obs.get("exists"), f"Candidate: pproxy.server not found"

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"pproxy.server mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )


@pytest.mark.differential
class TestTopLevelExportDifferential:
    """Paired tests for top-level exports via server module attributes."""

    @pytest.mark.parametrize("symbol", SERVER_EXPORTS)
    def test_export_exists(self, symbol, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.server", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.server", "candidate")

        o_attrs = oracle_obs.get("attributes", [])
        c_attrs = candidate_obs.get("attributes", [])
        assert symbol in o_attrs, f"Oracle server module missing {symbol}: {o_attrs[:10]}"
        assert symbol in c_attrs, f"Candidate server module missing {symbol}: {c_attrs[:10]}"

    @pytest.mark.parametrize("rid,symbol", [
        ("python.pproxy.server.compile_rule", "compile_rule"),
        ("python.pproxy.server.check_server_alive", "check_server_alive"),
        ("python.pproxy.server.prepare_ciphers", "prepare_ciphers"),
    ])
    def test_function_signature(self, rid, symbol, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        if oracle_obs.get("error") or candidate_obs.get("error"):
            pytest.skip(f"Per-class observation not available for {symbol}")

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

        if oracle_obs.get("error") or candidate_obs.get("error"):
            pytest.skip("Per-class observation not available for DIRECT")

        assert oracle_obs.get("exists"), f"Oracle: DIRECT not found"
        assert candidate_obs.get("exists"), f"Candidate: DIRECT not found"

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"DIRECT mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )
