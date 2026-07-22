"""Paired differential tests for cipher classes and registries.

These tests compare the pproxy oracle cipher implementations against
the eggress candidate implementation.

Tier: 2 (paired API oracle)
Gate: EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1
"""

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest


REQUIRE_DIFFERENTIAL = os.environ.get("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL") == "1"
SCRIPTS_DIR = Path(__file__).resolve().parents[3] / "scripts"


def _run_api_probe(module: str, symbol: str) -> dict:
    """Run the strict_api_probe.py and return the observation."""
    cmd = [sys.executable, str(SCRIPTS_DIR / "strict_api_probe.py"), "--module", module, "--symbol", symbol]
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
    if result.returncode == 0 and result.stdout.strip():
        return json.loads(result.stdout)
    return {"module": module, "symbol": symbol, "exists": False, "error": result.stderr}


def _run_class_probe(module: str, class_name: str) -> dict:
    """Run the strict_class_probe.py and return the observation."""
    cmd = [sys.executable, str(SCRIPTS_DIR / "strict_class_probe.py"), "--module", module, "--class-name", class_name]
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
    if result.returncode == 0 and result.stdout.strip():
        return json.loads(result.stdout)
    return {"module": module, "class_name": class_name, "error": result.stderr}


CIPHER_CLASSES = [
    ("pproxy.cipher", "AES_256_GCM_Cipher"),
    ("pproxy.cipher", "AES_192_GCM_Cipher"),
    ("pproxy.cipher", "AES_128_GCM_Cipher"),
    ("pproxy.cipher", "ChaCha20_IETF_POLY1305_Cipher"),
    ("pproxy.cipher", "AES_256_CFB_Cipher"),
    ("pproxy.cipher", "AES_192_CFB_Cipher"),
    ("pproxy.cipher", "AES_128_CFB_Cipher"),
    ("pproxy.cipher", "ChaCha20_Cipher"),
    ("pproxy.cipher", "ChaCha20_IETF_Cipher"),
]


@pytest.mark.differential
class TestCipherClassExistence:
    """Verify cipher classes exist in the candidate."""

    @pytest.mark.parametrize("module,class_name", CIPHER_CLASSES)
    def test_class_exists(self, module, class_name):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_class_probe(module, class_name)
        assert obs.get("error") is None, f"Failed to probe {class_name}: {obs.get('error')}"
        assert obs.get("bases") is not None, f"{class_name} has no bases"


@pytest.mark.differential
class TestCipherRegistry:
    """Verify cipher registries exist and are populated."""

    def test_cipher_map_exists(self):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_api_probe("pproxy.cipher", "MAP")
        assert obs.get("exists") is True, f"MAP not found: {obs.get('error')}"

    def test_get_cipher_exists(self):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_api_probe("pproxy.cipher", "get_cipher")
        assert obs.get("exists") is True, f"get_cipher not found: {obs.get('error')}"
        assert obs.get("type") == "function", f"get_cipher type: {obs.get('type')}"

    def test_packet_cipher_exists(self):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_class_probe("pproxy.cipher", "PacketCipher")
        assert obs.get("error") is None, f"PacketCipher not found: {obs.get('error')}"


@pytest.mark.differential
class TestCipherClassStructure:
    """Verify cipher class structure matches oracle."""

    @pytest.mark.parametrize("module,class_name", [
        ("pproxy.cipher", "AES_256_GCM_Cipher"),
        ("pproxy.cipher", "ChaCha20_IETF_POLY1305_Cipher"),
    ])
    def test_has_encrypt_method(self, module, class_name):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_class_probe(module, class_name)
        assert obs.get("error") is None
        methods = obs.get("methods", {})
        assert "encrypt" in methods, f"{class_name} missing 'encrypt' method"

    @pytest.mark.parametrize("module,class_name", [
        ("pproxy.cipher", "AES_256_GCM_Cipher"),
        ("pproxy.cipher", "ChaCha20_IETF_POLY1305_Cipher"),
    ])
    def test_has_decrypt_method(self, module, class_name):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        obs = _run_class_probe(module, class_name)
        assert obs.get("error") is None
        methods = obs.get("methods", {})
        assert "decrypt" in methods, f"{class_name} missing 'decrypt' method"
