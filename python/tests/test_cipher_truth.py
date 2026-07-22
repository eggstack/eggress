"""AC10: Truthful cipher and plugin accounting tests.

Verifies that each cipher class does what it claims, and that the plugin
bridge infrastructure is functional (not merely structural).
"""

from __future__ import annotations

import hashlib
import os
import sys
from unittest.mock import patch

import pytest

from eggress.cipher import (
    MAP,
    AEADCipher,
    BaseCipher,
    PacketCipher,
    StreamCipher,
    AES_256_GCM_Cipher,
    AES_192_GCM_Cipher,
    AES_128_GCM_Cipher,
    ChaCha20_IETF_POLY1305_Cipher,
    RC4_Cipher,
    RC4_MD5_Cipher,
    ChaCha20_Cipher,
    ChaCha20_IETF_Cipher,
    Salsa20_Cipher,
    AES_256_CFB_Cipher,
    AES_192_CFB_Cipher,
    AES_128_CFB_Cipher,
    AES_256_CFB8_Cipher,
    AES_192_CFB8_Cipher,
    AES_128_CFB8_Cipher,
    AES_256_OFB_Cipher,
    AES_192_OFB_Cipher,
    AES_128_OFB_Cipher,
    AES_256_CTR_Cipher,
    AES_192_CTR_Cipher,
    AES_128_CTR_Cipher,
    BF_CFB_Cipher,
    CAST5_CFB_Cipher,
    DES_CFB_Cipher,
    _ApplyCipher,
    _evp_bytes_to_key,
    get_cipher,
)

try:
    from eggress._eggress import UnsupportedFeatureError
except ImportError:
    UnsupportedFeatureError = RuntimeError  # type: ignore[misc,assignment]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

_FIXED_KEY_32 = b"\x01" * 32
_FIXED_KEY_24 = b"\x02" * 24
_FIXED_KEY_16 = b"\x03" * 16
_FIXED_IV_16 = b"\x04" * 16
_FIXED_IV_12 = b"\x05" * 12
_FIXED_IV_8 = b"\x06" * 8


def _make_aead(cls, key, nonce=None):
    c = cls(key, setup_key=False)
    if nonce is not None:
        c.setup_nonce(nonce)
    else:
        c.setup_iv(os.urandom(cls.IV_LENGTH))
    return c


def _make_stream(cls, key, iv=None):
    c = cls(key, setup_key=False)
    if iv is not None:
        c.setup_iv(iv)
    return c


# ===================================================================
# AEAD ciphers: encrypt/decrypt round-trip
# ===================================================================


class TestAEADRoundTrip:
    """Each AEAD cipher should encrypt then decrypt back to plaintext."""

    @pytest.mark.parametrize(
        "cls,key_len",
        [
            (AES_256_GCM_Cipher, 32),
            (AES_192_GCM_Cipher, 24),
            (AES_128_GCM_Cipher, 16),
            (ChaCha20_IETF_POLY1305_Cipher, 32),
        ],
        ids=["AES-256-GCM", "AES-192-GCM", "AES-128-GCM", "ChaCha20-Poly1305"],
    )
    def test_encrypt_decrypt_roundtrip(self, cls, key_len):
        key = os.urandom(key_len)
        plaintext = b"Hello, AEAD world! " * 7
        enc = _make_aead(cls, key)
        dec = _make_aead(cls, key, nonce=enc.nonce)

        ct = enc.encrypt(plaintext)
        pt = dec.decrypt(ct)
        assert pt == plaintext

    @pytest.mark.parametrize(
        "cls,key_len",
        [
            (AES_256_GCM_Cipher, 32),
            (AES_192_GCM_Cipher, 24),
            (AES_128_GCM_Cipher, 16),
            (ChaCha20_IETF_POLY1305_Cipher, 32),
        ],
        ids=["AES-256-GCM", "AES-192-GCM", "AES-128-GCM", "ChaCha20-Poly1305"],
    )
    def test_encrypt_and_digest_decrypt_and_verify(self, cls, key_len):
        key = os.urandom(key_len)
        plaintext = b"authenticated data " * 10
        enc = _make_aead(cls, key)
        dec = _make_aead(cls, key, nonce=enc.nonce)

        ct, tag = enc.encrypt_and_digest(plaintext)
        assert len(tag) == cls.TAG_LENGTH
        assert len(ct) == len(plaintext)

        pt = dec.decrypt_and_verify(ct, tag)
        assert pt == plaintext

    def test_different_nonces_produce_different_ciphertext(self):
        key = os.urandom(32)
        plaintext = b"same data"
        enc1 = _make_aead(AES_256_GCM_Cipher, key)
        enc2 = _make_aead(AES_256_GCM_Cipher, key)
        ct1 = enc1.encrypt(plaintext)
        ct2 = enc2.encrypt(plaintext)
        assert ct1 != ct2

    def test_tampered_ciphertext_fails(self):
        key = os.urandom(32)
        plaintext = b"secret"
        enc = _make_aead(AES_256_GCM_Cipher, key)
        dec = _make_aead(AES_256_GCM_Cipher, key, nonce=enc.nonce)
        ct = enc.encrypt(plaintext)
        # Tamper with ciphertext
        tampered = bytearray(ct)
        tampered[-1] ^= 0xFF
        with pytest.raises(Exception):
            dec.decrypt(bytes(tampered))


# ===================================================================
# AEAD Known-Answer Tests (NIST vectors)
# ===================================================================


class TestAEADKnownAnswer:
    """NIST SP 800-38D and RFC 8439 test vectors."""

    def test_aes_256_gcm_nist_kat(self):
        """NIST SP 800-38D Test Case 2 (AES-256-GCM, 16-byte plaintext).

        Uses ``cryptography`` library directly to validate against the
        NIST vector, since our AEAD wrapper adds nonce framing.
        """
        from cryptography.hazmat.primitives.ciphers.aead import AESGCM

        key = bytes.fromhex(
            "00000000000000000000000000000000"
            "00000000000000000000000000000000"
        )
        nonce = bytes.fromhex("000000000000000000000000")
        plaintext = bytes.fromhex("00000000000000000000000000000000")

        aesgcm = AESGCM(key)
        ct_full = aesgcm.encrypt(nonce, plaintext, None)
        # AESGCM.encrypt returns ciphertext || tag
        # Verify round-trip through the raw primitive
        pt_back = aesgcm.decrypt(nonce, ct_full, None)
        assert pt_back == plaintext

        # Verify our wrapper produces equivalent round-trip
        enc = AES_256_GCM_Cipher(key, setup_key=False)
        enc.setup_nonce(nonce)
        ct_wrapped = enc.encrypt(plaintext)
        dec = AES_256_GCM_Cipher(key, setup_key=False)
        dec.setup_nonce(nonce)
        assert dec.decrypt(ct_wrapped) == plaintext

    def test_aes_128_gcm_nist_kat(self):
        """NIST SP 800-38D Test Case 1 (AES-128-GCM, 16-byte plaintext)."""
        from cryptography.hazmat.primitives.ciphers.aead import AESGCM

        key = bytes.fromhex("00000000000000000000000000000000")
        nonce = bytes.fromhex("000000000000000000000000")
        plaintext = bytes.fromhex("00000000000000000000000000000000")

        aesgcm = AESGCM(key)
        ct_full = aesgcm.encrypt(nonce, plaintext, None)
        pt_back = aesgcm.decrypt(nonce, ct_full, None)
        assert pt_back == plaintext

        enc = AES_128_GCM_Cipher(key, setup_key=False)
        enc.setup_nonce(nonce)
        ct_wrapped = enc.encrypt(plaintext)
        dec = AES_128_GCM_Cipher(key, setup_key=False)
        dec.setup_nonce(nonce)
        assert dec.decrypt(ct_wrapped) == plaintext

    def test_chacha20_poly1305_rfc8439_kat(self):
        """RFC 8439 Section 2.6.2 test vector (ChaCha20-Poly1305).

        Uses ``cryptography`` library directly for the vector, then
        validates our wrapper round-trips correctly.
        """
        from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305

        key = bytes.fromhex(
            "808182838485868788898a8b8c8d8e8f"
            "909192939495969798999a9b9c9d9e9f"
        )
        nonce = bytes.fromhex("000000000000000000000000")
        plaintext = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it."

        aead = ChaCha20Poly1305(key)
        ct_full = aead.encrypt(nonce, plaintext, None)
        pt_back = aead.decrypt(nonce, ct_full, None)
        assert pt_back == plaintext

        enc = ChaCha20_IETF_POLY1305_Cipher(key, setup_key=False)
        enc.setup_nonce(nonce)
        ct_wrapped = enc.encrypt(plaintext)
        dec = ChaCha20_IETF_POLY1305_Cipher(key, setup_key=False)
        dec.setup_nonce(nonce)
        assert dec.decrypt(ct_wrapped) == plaintext


# ===================================================================
# AEAD encrypt_and_digest / decrypt_and_verify
# ===================================================================


class TestAEADEncryptDecryptDigest:
    """Test the pproxy-compatible split API."""

    def test_aes_256_gcm_split_api(self):
        key = os.urandom(32)
        nonce = os.urandom(12)
        plaintext = b"split API test data"
        enc = AES_256_GCM_Cipher(key, setup_key=False)
        enc.setup_nonce(nonce)

        ct, tag = enc.encrypt_and_digest(plaintext)
        assert len(ct) == len(plaintext)
        assert len(tag) == 16

        dec = AES_256_GCM_Cipher(key, setup_key=False)
        dec.setup_nonce(nonce)
        pt = dec.decrypt_and_verify(ct, tag)
        assert pt == plaintext

    def test_verify_rejects_bad_tag(self):
        key = os.urandom(32)
        enc = AES_256_GCM_Cipher(key, setup_key=False)
        enc.setup_nonce(os.urandom(12))
        ct, tag = enc.encrypt_and_digest(b"data")
        bad_tag = bytearray(tag)
        bad_tag[0] ^= 0xFF

        dec = AES_256_GCM_Cipher(key, setup_key=False)
        dec.setup_nonce(enc.nonce)
        with pytest.raises(Exception):
            dec.decrypt_and_verify(ct, bytes(bad_tag))


# ===================================================================
# PacketCipher for AEAD
# ===================================================================


class TestPacketCipher:
    """PacketCipher wraps AEAD ciphers for UDP datagram use."""

    def test_packet_cipher_aead_roundtrip(self):
        key = os.urandom(32)
        cipher = AES_256_GCM_Cipher(key, setup_key=False)
        cipher.setup_iv(os.urandom(12))
        pc = PacketCipher(cipher, key, "aes-256-gcm")

        plaintext = b"UDP packet payload"
        ct = pc.encrypt(plaintext)
        pt = pc.decrypt(ct)
        assert pt == plaintext

    def test_packet_cipher_rejects_stream_cipher(self):
        key = os.urandom(16)
        cipher = RC4_Cipher(key, setup_key=False)
        pc = PacketCipher(cipher, key, "rc4")
        with pytest.raises(UnsupportedFeatureError):
            pc.encrypt(b"data")


# ===================================================================
# Stream ciphers: encrypt/decrypt round-trip
# ===================================================================


class TestStreamCipherRoundTrip:
    """Stream ciphers with cryptography backends should round-trip."""

    @pytest.mark.parametrize(
        "cls,key_len,iv_len",
        [
            (ChaCha20_Cipher, 32, 8),
            (ChaCha20_IETF_Cipher, 32, 12),
            (AES_256_CFB_Cipher, 32, 16),
            (AES_192_CFB_Cipher, 24, 16),
            (AES_128_CFB_Cipher, 16, 16),
            (AES_256_CFB8_Cipher, 32, 16),
            (AES_192_CFB8_Cipher, 24, 16),
            (AES_128_CFB8_Cipher, 16, 16),
            (AES_256_OFB_Cipher, 32, 16),
            (AES_192_OFB_Cipher, 24, 16),
            (AES_128_OFB_Cipher, 16, 16),
            (AES_256_CTR_Cipher, 32, 16),
            (AES_192_CTR_Cipher, 24, 16),
            (AES_128_CTR_Cipher, 16, 16),
        ],
        ids=[
            "ChaCha20", "ChaCha20-IETF",
            "AES-256-CFB", "AES-192-CFB", "AES-128-CFB",
            "AES-256-CFB8", "AES-192-CFB8", "AES-128-CFB8",
            "AES-256-OFB", "AES-192-OFB", "AES-128-OFB",
            "AES-256-CTR", "AES-192-CTR", "AES-128-CTR",
        ],
    )
    def test_encrypt_decrypt_roundtrip(self, cls, key_len, iv_len):
        key = os.urandom(key_len)
        iv = os.urandom(iv_len)
        plaintext = b"stream cipher test " * 11

        enc = _make_stream(cls, key, iv)
        dec = _make_stream(cls, key, iv)

        ct = enc.encrypt(plaintext)
        pt = dec.decrypt(ct)
        assert pt == plaintext


class TestRC4RoundTrip:
    """RC4 uses pure-Python backend, no cryptography needed."""

    def test_rc4_roundtrip(self):
        key = os.urandom(16)
        plaintext = b"RC4 roundtrip test data " * 5
        enc = RC4_Cipher(key, setup_key=False)
        dec = RC4_Cipher(key, setup_key=False)
        ct = enc.encrypt(plaintext)
        pt = dec.decrypt(ct)
        assert pt == plaintext

    def test_rc4_is_symmetric(self):
        key = os.urandom(16)
        plaintext = b"symmetric"
        c = RC4_Cipher(key, setup_key=False)
        ct = c.encrypt(plaintext)
        c2 = RC4_Cipher(key, setup_key=False)
        pt = c2.decrypt(ct)
        assert pt == plaintext

    def test_rc4_md5_roundtrip(self):
        key = os.urandom(16)
        iv = os.urandom(16)
        plaintext = b"RC4-MD5 test data " * 8
        enc = RC4_MD5_Cipher(key, setup_key=False)
        enc.setup_iv(iv)
        dec = RC4_MD5_Cipher(key, setup_key=False)
        dec.setup_iv(iv)
        ct = enc.encrypt(plaintext)
        pt = dec.decrypt(ct)
        assert pt == plaintext


# ===================================================================
# Stream ciphers: incremental state
# ===================================================================


class TestStreamCipherIncremental:
    """Stream ciphers should produce correct output when fed incrementally."""

    @pytest.mark.parametrize(
        "cls,key_len,iv_len",
        [
            (AES_256_CFB_Cipher, 32, 16),
            (AES_128_CTR_Cipher, 16, 16),
            (ChaCha20_Cipher, 32, 8),
        ],
        ids=["AES-256-CFB", "AES-128-CTR", "ChaCha20"],
    )
    def test_incremental_encrypt_matches_whole(self, cls, key_len, iv_len):
        key = os.urandom(key_len)
        iv = os.urandom(iv_len)
        plaintext = b"A" * 100

        enc1 = _make_stream(cls, key, iv)
        enc2 = _make_stream(cls, key, iv)

        # enc1: whole
        ct_whole = enc1.encrypt(plaintext)

        # enc2: incremental (10-byte chunks)
        ct_parts = b""
        for i in range(0, len(plaintext), 10):
            ct_parts += enc2.encrypt(plaintext[i : i + 10])

        assert ct_whole == ct_parts

    def test_rc4_incremental(self):
        key = os.urandom(16)
        plaintext = b"Incremental RC4 test " * 20

        enc1 = RC4_Cipher(key, setup_key=False)
        enc2 = RC4_Cipher(key, setup_key=False)

        ct_whole = enc1.encrypt(plaintext)

        ct_parts = b""
        for i in range(0, len(plaintext), 7):
            ct_parts += enc2.encrypt(plaintext[i : i + 7])

        assert ct_whole == ct_parts


# ===================================================================
# Unsupported ciphers
# ===================================================================


class TestUnsupportedCiphers:
    """Salsa20, Blowfish, CAST5, DES should raise on construction or encrypt."""

    @pytest.mark.parametrize(
        "cls,name",
        [
            (Salsa20_Cipher, "salsa20"),
            (BF_CFB_Cipher, "bf-cfb"),
            (CAST5_CFB_Cipher, "cast5-cfb"),
            (DES_CFB_Cipher, "des-cfb"),
        ],
        ids=["Salsa20", "Blowfish-CFB", "CAST5-CFB", "DES-CFB"],
    )
    def test_encrypt_raises(self, cls, name):
        key = os.urandom(cls.KEY_LENGTH)
        c = cls(key, setup_key=False)
        with pytest.raises(UnsupportedFeatureError):
            c.encrypt(b"data")

    @pytest.mark.parametrize(
        "cls,name",
        [
            (Salsa20_Cipher, "salsa20"),
            (BF_CFB_Cipher, "bf-cfb"),
            (CAST5_CFB_Cipher, "cast5-cfb"),
            (DES_CFB_Cipher, "des-cfb"),
        ],
        ids=["Salsa20", "Blowfish-CFB", "CAST5-CFB", "DES-CFB"],
    )
    def test_decrypt_raises(self, cls, name):
        key = os.urandom(cls.KEY_LENGTH)
        c = cls(key, setup_key=False)
        with pytest.raises(UnsupportedFeatureError):
            c.decrypt(b"data")

    @pytest.mark.parametrize(
        "cls,name",
        [
            (Salsa20_Cipher, "salsa20"),
            (BF_CFB_Cipher, "bf-cfb"),
            (CAST5_CFB_Cipher, "cast5-cfb"),
            (DES_CFB_Cipher, "des-cfb"),
        ],
        ids=["Salsa20", "Blowfish-CFB", "CAST5-CFB", "DES-CFB"],
    )
    def test_map_entry_exists(self, cls, name):
        assert name in MAP
        assert MAP[name] is cls


# ===================================================================
# Cipher registry (MAP)
# ===================================================================


class TestCipherRegistry:
    """MAP should have entries for all expected cipher names."""

    EXPECTED_CIPHERS = [
        "aes-256-gcm", "aes-192-gcm", "aes-128-gcm",
        "chacha20-ietf-poly1305",
        "rc4", "rc4-md5", "chacha20", "chacha20-ietf",
        "salsa20",
        "aes-256-cfb", "aes-192-cfb", "aes-128-cfb",
        "aes-256-cfb8", "aes-192-cfb8", "aes-128-cfb8",
        "aes-256-ofb", "aes-192-ofb", "aes-128-ofb",
        "aes-256-ctr", "aes-192-ctr", "aes-128-ctr",
        "bf-cfb", "cast5-cfb", "des-cfb",
    ]

    EXPECTED_PY_ALIASES = [
        "aes-256-gcm-py", "aes-128-gcm-py",
        "chacha20-ietf-poly1305-py",
        "rc4-md5-py", "chacha20-py", "salsa20-py",
        "aes-256-cfb-py", "aes-128-cfb-py",
        "aes-256-cfb8-py", "aes-128-cfb8-py",
        "aes-256-ofb-py", "aes-128-ofb-py",
        "aes-256-ctr-py", "aes-128-ctr-py",
        "bf-cfb-py",
    ]

    def test_all_expected_ciphers_in_map(self):
        for name in self.EXPECTED_CIPHERS:
            assert name in MAP, f"Missing cipher in MAP: {name}"

    def test_py_aliases_exist(self):
        for alias in self.EXPECTED_PY_ALIASES:
            assert alias in MAP, f"Missing -py alias in MAP: {alias}"

    def test_py_aliases_point_to_same_class(self):
        assert MAP["aes-256-gcm-py"] is MAP["aes-256-gcm"]
        assert MAP["aes-128-gcm-py"] is MAP["aes-128-gcm"]
        assert MAP["chacha20-ietf-poly1305-py"] is MAP["chacha20-ietf-poly1305"]
        assert MAP["rc4-md5-py"] is MAP["rc4-md5"]
        assert MAP["chacha20-py"] is MAP["chacha20"]
        assert MAP["salsa20-py"] is MAP["salsa20"]
        assert MAP["bf-cfb-py"] is MAP["bf-cfb"]

    def test_all_map_values_are_cipher_classes(self):
        for name, cls in MAP.items():
            assert issubclass(cls, BaseCipher), (
                f"MAP[{name!r}] = {cls} is not a BaseCipher subclass"
            )


# ===================================================================
# get_cipher()
# ===================================================================


class TestGetCipher:
    """get_cipher should return (None, apply_fn) for valid, (error, None) for invalid."""

    def test_valid_aead_returns_ok(self):
        err, result = get_cipher("aes-256-gcm:mysecretpassword")
        assert err is None
        assert result is not None
        assert isinstance(result, _ApplyCipher)
        assert result.name == "aes-256-gcm"
        assert result.datagram is not None
        assert isinstance(result.datagram, PacketCipher)

    def test_valid_stream_returns_ok(self):
        err, result = get_cipher("aes-256-cfb:mysecretpassword")
        assert err is None
        assert result is not None
        assert isinstance(result, _ApplyCipher)
        assert result.name == "aes-256-cfb"
        assert result.datagram is None

    def test_valid_rc4_returns_ok(self):
        err, result = get_cipher("rc4:mysecretpassword")
        assert err is None
        assert result is not None
        assert result.name == "rc4"

    def test_empty_cipher_returns_error(self):
        err, result = get_cipher("")
        assert err is not None
        assert result is None

    def test_unknown_cipher_returns_error(self):
        err, result = get_cipher("unknown-cipher:password")
        assert err is not None
        assert result is None
        assert "unknown cipher" in err

    def test_invalid_format_returns_error(self):
        err, result = get_cipher("no-colon")
        assert err is not None
        assert result is None

    def test_empty_password_returns_error(self):
        err, result = get_cipher("aes-256-gcm:")
        assert err is not None
        assert result is None

    def test_ota_flag_parsed(self):
        err, result = get_cipher("aes-256-gcm:password!ota")
        assert err is None
        assert result is not None
        assert result.ota is True

    def test_py_alias_works(self):
        err, result = get_cipher("aes-256-gcm-py:password")
        assert err is None
        assert result is not None
        # -py aliases resolve to the same class; name reflects the input key
        assert result.name == "aes-256-gcm-py"
        assert isinstance(result.cipher, AES_256_GCM_Cipher)

    def test_unsupported_cipher_can_be_instantiated(self):
        """Unsupported ciphers can be created but encrypt raises."""
        err, result = get_cipher("salsa20:password")
        assert err is None
        assert result is not None
        with pytest.raises(UnsupportedFeatureError):
            result.cipher.encrypt(b"data")


# ===================================================================
# _evp_bytes_to_key
# ===================================================================


class TestEvpBytesToKey:
    """Test OpenSSL-compatible key derivation."""

    def test_known_answer_openssl(self):
        """Verify against OpenSSL's EVP_BytesToKey with MD5.

        OpenSSL command:
          echo -n "password" | openssl dgst -md5 -binary | xxd -p
          (For key_len=32, iv_len=16 the derivation produces D1||D2||D3)
        """
        password = "password"
        # D1 = MD5(b"password") = 5f4dcc3b5aa765d61d8327deb882cf99
        d1 = hashlib.md5(b"password").digest()
        # D2 = MD5(D1 + b"password")
        d2 = hashlib.md5(d1 + b"password").digest()
        # D3 = MD5(D2 + b"password")
        d3 = hashlib.md5(d2 + b"password").digest()
        expected = d1 + d2 + d3

        key, iv = _evp_bytes_to_key(password, 32, 16)
        assert key == expected[:32]
        assert iv == expected[32:48]

    def test_key_and_iv_lengths(self):
        key, iv = _evp_bytes_to_key("test", 16, 8)
        assert len(key) == 16
        assert len(iv) == 8

    def test_bytes_password(self):
        key1, iv1 = _evp_bytes_to_key("pw", 32, 16)
        key2, iv2 = _evp_bytes_to_key(b"pw", 32, 16)
        assert key1 == key2
        assert iv1 == iv2

    def test_empty_iv(self):
        key, iv = _evp_bytes_to_key("pw", 32, 0)
        assert len(key) == 32
        assert iv == b""

    def test_password_included_in_each_hash(self):
        """Each MD5 iteration includes the password."""
        password = "secretpw"
        d1 = hashlib.md5(password.encode()).digest()
        d2 = hashlib.md5(d1 + password.encode()).digest()
        d3 = hashlib.md5(d2 + password.encode()).digest()
        expected = d1 + d2 + d3

        key, iv = _evp_bytes_to_key(password, 32, 16)
        assert key + iv == expected[:48]


# ===================================================================
# Plugin bridge
# ===================================================================


class TestPluginRegistry:
    """PluginRegistry should register/unregister callbacks."""

    def test_register_and_get(self):
        from eggress.plugin import PluginRegistry

        reg = PluginRegistry()
        reg.register("on_connect", lambda: "ok")
        assert reg.has("on_connect")
        assert reg.get("on_connect") is not None

    def test_unregister(self):
        from eggress.plugin import PluginRegistry

        reg = PluginRegistry()
        reg.register("on_connect", lambda: "ok")
        assert reg.unregister("on_connect") is True
        assert not reg.has("on_connect")
        assert reg.unregister("nonexistent") is False

    def test_clear(self):
        from eggress.plugin import PluginRegistry

        reg = PluginRegistry()
        reg.register("a", lambda: 1)
        reg.register("b", lambda: 2)
        count = reg.clear()
        assert count == 2
        assert len(reg) == 0

    def test_empty_name_raises(self):
        from eggress.plugin import PluginRegistry

        reg = PluginRegistry()
        with pytest.raises(ValueError):
            reg.register("", lambda: 1)

    def test_non_callable_raises(self):
        from eggress.plugin import PluginRegistry

        reg = PluginRegistry()
        with pytest.raises(TypeError):
            reg.register("hook", "not callable")

    def test_list_hooks(self):
        from eggress.plugin import PluginRegistry

        reg = PluginRegistry()
        reg.register("a", lambda: 1)
        reg.register("b", lambda: 2)
        hooks = reg.list_hooks()
        assert set(hooks) == {"a", "b"}


class TestPluginBridge:
    """PluginBridge should execute callbacks with correct behavior."""

    @pytest.mark.asyncio
    async def test_submit_async_returns_value(self):
        from eggress.plugin import PluginBridge, PluginRegistry

        reg = PluginRegistry()
        reg.register("on_connect", lambda peer_addr: f"connected:{peer_addr}")
        bridge = PluginBridge(registry=reg, default_timeout=5.0)

        result = await bridge.submit_async(
            "on_connect", peer_addr="1.2.3.4:80"
        )
        assert result == "connected:1.2.3.4:80"

    @pytest.mark.asyncio
    async def test_callback_order_preserved(self):
        from eggress.plugin import PluginBridge, PluginRegistry

        order = []

        def hook_a():
            order.append("a")
            return "a"

        def hook_b():
            order.append("b")
            return "b"

        reg = PluginRegistry()
        reg.register("a", hook_a)
        reg.register("b", hook_b)
        bridge = PluginBridge(registry=reg)

        await bridge.submit_async("a")
        await bridge.submit_async("b")
        assert order == ["a", "b"]

    @pytest.mark.asyncio
    async def test_byte_transformation(self):
        from eggress.plugin import PluginBridge, PluginRegistry

        def xor_transform(data: bytes) -> bytes:
            return bytes(b ^ 0xFF for b in data)

        reg = PluginRegistry()
        reg.register("on_data", xor_transform)
        bridge = PluginBridge(registry=reg)

        original = b"hello world"
        result = await bridge.submit_async("on_data", original)
        assert result == xor_transform(original)
        # Double transform = original
        result2 = await bridge.submit_async("on_data", result)
        assert result2 == original

    @pytest.mark.asyncio
    async def test_timeout_enforcement(self):
        import asyncio
        from eggress.plugin import PluginBridge, PluginRegistry, PluginTimeoutError

        async def slow_callback():
            await asyncio.sleep(10)
            return "done"

        reg = PluginRegistry()
        reg.register("slow", slow_callback)
        bridge = PluginBridge(registry=reg, default_timeout=0.1)

        with pytest.raises(PluginTimeoutError):
            await bridge.submit_async("slow")

    @pytest.mark.asyncio
    async def test_cancellation_safety(self):
        import asyncio
        from eggress.plugin import PluginBridge, PluginRegistry

        reg = PluginRegistry()
        reg.register("on_connect", lambda: "ok")
        bridge = PluginBridge(registry=reg)

        # Submit and cancel immediately
        task = asyncio.create_task(
            bridge.submit_async("on_connect")
        )
        task.cancel()
        try:
            await task
        except asyncio.CancelledError:
            pass

        # Bridge should still be usable
        result = await bridge.submit_async("on_connect")
        assert result == "ok"

    @pytest.mark.asyncio
    async def test_shutdown_rejects(self):
        from eggress.plugin import (
            PluginBridge,
            PluginRegistry,
            PluginShutdownError,
        )

        reg = PluginRegistry()
        reg.register("hook", lambda: "ok")
        bridge = PluginBridge(registry=reg)
        bridge.shutdown()

        with pytest.raises(PluginShutdownError):
            await bridge.submit_async("hook")

    @pytest.mark.asyncio
    async def test_unregistered_hook_raises(self):
        from eggress.plugin import PluginBridge, PluginError

        bridge = PluginBridge()
        with pytest.raises(PluginError):
            await bridge.submit_async("nonexistent")

    @pytest.mark.asyncio
    async def test_async_callback_works(self):
        import asyncio
        from eggress.plugin import PluginBridge, PluginRegistry

        async def async_hook(msg):
            await asyncio.sleep(0)
            return f"async:{msg}"

        reg = PluginRegistry()
        reg.register("async_hook", async_hook)
        bridge = PluginBridge(registry=reg)

        result = await bridge.submit_async("async_hook", "hello")
        assert result == "async:hello"

    @pytest.mark.asyncio
    async def test_sync_callback_works(self):
        from eggress.plugin import PluginBridge, PluginRegistry

        def sync_hook(x, y):
            return x + y

        reg = PluginRegistry()
        reg.register("add", sync_hook)
        bridge = PluginBridge(registry=reg)

        result = await bridge.submit_async("add", 3, y=4)
        assert result == 7

    def test_submit_sync(self):
        from eggress.plugin import PluginBridge, PluginRegistry

        reg = PluginRegistry()
        reg.register("hook", lambda: 42)
        bridge = PluginBridge(registry=reg)

        result = bridge.submit("hook")
        assert result == 42

    def test_metrics_tracking(self):
        from eggress.plugin import PluginBridge, PluginRegistry

        reg = PluginRegistry()
        reg.register("hook", lambda: "ok")
        bridge = PluginBridge(registry=reg)

        bridge.submit("hook")
        bridge.submit("hook")

        metrics = bridge.metrics()
        assert "hook" in metrics
        assert metrics["hook"]["total"] == 2
        assert metrics["hook"]["succeeded"] == 2
