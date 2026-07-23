"""Paired differential tests for cipher classes and registries.

These tests compare the pproxy oracle cipher implementations against
the eggress candidate implementation.

Tier: 2 (paired API oracle)
Gate: --oracle-observations-dir and --candidate-observations-dir required
"""

import pytest


CIPHER_CLASSES = [
    ("python.pproxy.cipher.AES_256_GCM_Cipher", "pproxy.cipher", "AES_256_GCM_Cipher"),
    ("python.pproxy.cipher.AES_192_GCM_Cipher", "pproxy.cipher", "AES_192_GCM_Cipher"),
    ("python.pproxy.cipher.AES_128_GCM_Cipher", "pproxy.cipher", "AES_128_GCM_Cipher"),
    ("python.pproxy.cipher.ChaCha20_IETF_POLY1305_Cipher", "pproxy.cipher", "ChaCha20_IETF_POLY1305_Cipher"),
    ("python.pproxy.cipher.AES_256_CFB_Cipher", "pproxy.cipher", "AES_256_CFB_Cipher"),
    ("python.pproxy.cipher.AES_192_CFB_Cipher", "pproxy.cipher", "AES_192_CFB_Cipher"),
    ("python.pproxy.cipher.AES_128_CFB_Cipher", "pproxy.cipher", "AES_128_CFB_Cipher"),
    ("python.pproxy.cipher.ChaCha20_Cipher", "pproxy.cipher", "ChaCha20_Cipher"),
    ("python.pproxy.cipher.ChaCha20_IETF_Cipher", "pproxy.cipher", "ChaCha20_IETF_Cipher"),
]


@pytest.mark.differential
class TestCipherClassExistence:
    """Verify cipher classes exist in the candidate."""

    @pytest.mark.parametrize("rid,module,class_name", CIPHER_CLASSES)
    def test_class_exists(self, rid, module, class_name, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        assert oracle_obs.get("error") is None, f"Oracle failed to probe {class_name}: {oracle_obs.get('error')}"
        assert candidate_obs.get("error") is None, f"Candidate failed to probe {class_name}: {candidate_obs.get('error')}"
        assert oracle_obs.get("bases") is not None, f"Oracle: {class_name} has no bases"
        assert candidate_obs.get("bases") is not None, f"Candidate: {class_name} has no bases"


@pytest.mark.differential
class TestCipherRegistry:
    """Verify cipher registries exist and are populated."""

    def test_cipher_map_exists(self, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.cipher.MAP", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.cipher.MAP", "candidate")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"MAP mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )

    def test_get_cipher_exists(self, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.cipher.get_cipher", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.cipher.get_cipher", "candidate")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"get_cipher mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )

    def test_packet_cipher_exists(self, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, "python.pproxy.cipher.PacketCipher", "oracle")
        candidate_obs = load_observation(candidate_dir, "python.pproxy.cipher.PacketCipher", "candidate")

        assert oracle_obs.get("error") is None, f"Oracle: PacketCipher not found: {oracle_obs.get('error')}"
        assert candidate_obs.get("error") is None, f"Candidate: PacketCipher not found: {candidate_obs.get('error')}"


@pytest.mark.differential
class TestCipherClassStructure:
    """Verify cipher class structure matches oracle."""

    @pytest.mark.parametrize("rid,module,class_name", [
        ("python.pproxy.cipher.AES_256_GCM_Cipher", "pproxy.cipher", "AES_256_GCM_Cipher"),
        ("python.pproxy.cipher.ChaCha20_IETF_POLY1305_Cipher", "pproxy.cipher", "ChaCha20_IETF_POLY1305_Cipher"),
    ])
    def test_has_encrypt_method(self, rid, module, class_name, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        assert oracle_obs.get("error") is None
        assert candidate_obs.get("error") is None
        o_methods = oracle_obs.get("methods", {})
        c_methods = candidate_obs.get("methods", {})
        assert "encrypt" in o_methods, f"Oracle: {class_name} missing 'encrypt' method"
        assert "encrypt" in c_methods, f"Candidate: {class_name} missing 'encrypt' method"

    @pytest.mark.parametrize("rid,module,class_name", [
        ("python.pproxy.cipher.AES_256_GCM_Cipher", "pproxy.cipher", "AES_256_GCM_Cipher"),
        ("python.pproxy.cipher.ChaCha20_IETF_POLY1305_Cipher", "pproxy.cipher", "ChaCha20_IETF_POLY1305_Cipher"),
    ])
    def test_has_decrypt_method(self, rid, module, class_name, require_obs_dirs):
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        assert oracle_obs.get("error") is None
        assert candidate_obs.get("error") is None
        o_methods = oracle_obs.get("methods", {})
        c_methods = candidate_obs.get("methods", {})
        assert "decrypt" in o_methods, f"Oracle: {class_name} missing 'decrypt' method"
        assert "decrypt" in c_methods, f"Candidate: {class_name} missing 'decrypt' method"
