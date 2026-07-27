"""Paired differential tests for cipher classes and registries.

These tests compare the pproxy oracle cipher implementations against
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


CIPHER_CLASSES = [
    ("AES_256_GCM_Cipher", "pproxy.cipher"),
    ("AES_192_GCM_Cipher", "pproxy.cipher"),
    ("AES_128_GCM_Cipher", "pproxy.cipher"),
    ("ChaCha20_IETF_POLY1305_Cipher", "pproxy.cipher"),
    ("AES_256_CFB_Cipher", "pproxy.cipher"),
    ("AES_192_CFB_Cipher", "pproxy.cipher"),
    ("AES_128_CFB_Cipher", "pproxy.cipher"),
    ("ChaCha20_Cipher", "pproxy.cipher"),
    ("ChaCha20_IETF_Cipher", "pproxy.cipher"),
]


@pytest.mark.differential
class TestCipherClassExistence:
    """Verify cipher classes exist in the candidate via module attributes."""

    @pytest.mark.parametrize("class_name,module", CIPHER_CLASSES)
    def test_class_exists(self, class_name, module, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.cipher", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.cipher", "candidate")

        assert oracle_obs.get("error") is None, f"Oracle cipher module probe failed: {oracle_obs.get('error')}"
        assert candidate_obs.get("error") is None, f"Candidate cipher module probe failed: {candidate_obs.get('error')}"

        o_attrs = oracle_obs.get("attributes", [])
        c_attrs = candidate_obs.get("attributes", [])
        assert class_name in o_attrs, f"Oracle cipher module missing {class_name}: {o_attrs[:10]}"
        assert class_name in c_attrs, f"Candidate cipher module missing {class_name}: {c_attrs[:10]}"


@pytest.mark.differential
class TestCipherRegistry:
    """Verify cipher registries exist and are populated."""

    def test_cipher_map_exists(self, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.cipher", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.cipher", "candidate")

        o_attrs = oracle_obs.get("attributes", [])
        c_attrs = candidate_obs.get("attributes", [])
        assert "MAP" in o_attrs, f"Oracle cipher module missing MAP"
        assert "MAP" in c_attrs, f"Candidate cipher module missing MAP"

    def test_get_cipher_exists(self, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.cipher", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.cipher", "candidate")

        o_attrs = oracle_obs.get("attributes", [])
        c_attrs = candidate_obs.get("attributes", [])
        assert "get_cipher" in o_attrs, f"Oracle cipher module missing get_cipher"
        assert "get_cipher" in c_attrs, f"Candidate cipher module missing get_cipher"

    def test_packet_cipher_exists(self, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.cipher", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.cipher", "candidate")

        o_attrs = oracle_obs.get("attributes", [])
        c_attrs = candidate_obs.get("attributes", [])
        assert "PacketCipher" in o_attrs, f"Oracle cipher module missing PacketCipher"
        assert "PacketCipher" in c_attrs, f"Candidate cipher module missing PacketCipher"


@pytest.mark.differential
class TestCipherClassStructure:
    """Verify cipher class structure via module-level attributes."""

    def test_aead_classes_exist(self, require_obs_dirs):
        """Verify AEAD cipher classes are present in both oracle and candidate."""
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.cipher", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.cipher", "candidate")

        o_attrs = oracle_obs.get("attributes", [])
        c_attrs = candidate_obs.get("attributes", [])

        for cls in ["AES_256_GCM_Cipher", "ChaCha20_IETF_POLY1305_Cipher", "AEADCipher"]:
            assert cls in o_attrs, f"Oracle missing {cls}"
            assert cls in c_attrs, f"Candidate missing {cls}"

    def test_stream_classes_exist(self, require_obs_dirs):
        """Verify stream cipher classes are present in both oracle and candidate."""
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.cipher", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.cipher", "candidate")

        o_attrs = oracle_obs.get("attributes", [])
        c_attrs = candidate_obs.get("attributes", [])

        for cls in ["AES_256_CFB_Cipher", "ChaCha20_Cipher", "ChaCha20_IETF_Cipher"]:
            assert cls in o_attrs, f"Oracle missing {cls}"
            assert cls in c_attrs, f"Candidate missing {cls}"

    def test_base_classes_exist(self, require_obs_dirs):
        """Verify base cipher classes are present in both oracle and candidate."""
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.cipher", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.cipher", "candidate")

        o_attrs = oracle_obs.get("attributes", [])
        c_attrs = candidate_obs.get("attributes", [])

        for cls in ["BaseCipher", "AEADCipher", "PacketCipher"]:
            assert cls in o_attrs, f"Oracle missing {cls}"
            assert cls in c_attrs, f"Candidate missing {cls}"
