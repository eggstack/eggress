"""Paired differential tests for protocol classes.

These tests compare the pproxy oracle protocol class hierarchy against
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


PROTOCOL_CLASSES = [
    "Direct", "HTTP", "HTTPOnly", "Socks4", "Socks5", "SS", "Trojan", "Echo",
]


@pytest.mark.differential
class TestProtocolClassExistence:
    """Verify protocol classes exist in the candidate via module attributes."""

    @pytest.mark.parametrize("class_name", PROTOCOL_CLASSES)
    def test_class_exists(self, class_name, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.proto", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.proto", "candidate")

        assert oracle_obs.get("error") is None, f"Oracle proto module probe failed: {oracle_obs.get('error')}"
        assert candidate_obs.get("error") is None, f"Candidate proto module probe failed: {candidate_obs.get('error')}"

        o_attrs = oracle_obs.get("attributes", [])
        c_attrs = candidate_obs.get("attributes", [])
        assert class_name in o_attrs, f"Oracle proto module missing {class_name}: {o_attrs[:10]}"
        assert class_name in c_attrs, f"Candidate proto module missing {class_name}: {c_attrs[:10]}"


@pytest.mark.differential
class TestProtocolModuleStructure:
    """Verify protocol module structure via module-level attributes."""

    def test_all_expected_classes_exist(self, require_obs_dirs):
        """Verify all expected protocol classes are present in both oracle and candidate."""
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.proto", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.proto", "candidate")

        o_attrs = oracle_obs.get("attributes", [])
        c_attrs = candidate_obs.get("attributes", [])

        for cls in PROTOCOL_CLASSES:
            assert cls in o_attrs, f"Oracle missing {cls}"
            assert cls in c_attrs, f"Candidate missing {cls}"

    def test_registry_exists(self, require_obs_dirs):
        """Verify MAPPINGS registry exists in both oracle and candidate."""
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.proto", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.proto", "candidate")

        o_attrs = oracle_obs.get("attributes", [])
        c_attrs = candidate_obs.get("attributes", [])
        assert "MAPPINGS" in o_attrs, f"Oracle proto module missing MAPPINGS"
        assert "MAPPINGS" in c_attrs, f"Candidate proto module missing MAPPINGS"

    def test_module_comparison(self, require_obs_dirs):
        """Compare proto module structure between oracle and candidate."""
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.proto", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.proto", "candidate")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"Proto module mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )
