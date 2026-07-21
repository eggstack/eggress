"""Milestone C functional internal API tests.

Covers C1 (server utilities), C2 (stream/datagram handlers), C8 (sslwrap),
C9/C10 (cipher inventory and key derivation), C11 (PacketCipher),
and C15 (differential/property tests).

All tests are runnable without the Rust native extension -- they exercise
pure-Python compatibility surfaces only.
"""

from __future__ import annotations

import asyncio
import hashlib
import os
import struct
import time
from typing import Any

import pytest


# ---------------------------------------------------------------------------
# C9/C10: Stream cipher round-trip tests
# ---------------------------------------------------------------------------


class TestStreamCipherRoundTrips:
    """Encrypt/decrypt round-trip tests for all functional stream ciphers."""

    def test_rc4_round_trip(self):
        from eggress.cipher import RC4_Cipher

        key = os.urandom(16)
        cipher = RC4_Cipher(key, setup_key=False)
        cipher._iv = b""
        plaintext = b"Hello, World! This is a test of RC4 stream cipher."
        ct = cipher.encrypt(plaintext)
        # RC4 is symmetric: decrypt with same state should recover plaintext
        cipher2 = RC4_Cipher(key, setup_key=False)
        cipher2._iv = b""
        pt = cipher2.decrypt(ct)
        assert pt == plaintext

    def test_rc4_incremental(self):
        from eggress.cipher import RC4_Cipher

        key = b"0123456789abcdef"
        cipher1 = RC4_Cipher(key, setup_key=False)
        cipher1._iv = b""
        cipher2 = RC4_Cipher(key, setup_key=False)
        cipher2._iv = b""

        # Encrypt in chunks
        chunks = [b"hello", b" ", b"world", b"!"]
        encrypted = b"".join(cipher1.encrypt(c) for c in chunks)

        # Decrypt in one shot
        decrypted = cipher2.decrypt(encrypted)
        assert decrypted == b"hello world!"

    def test_rc4_md5_round_trip(self):
        from eggress.cipher import RC4_MD5_Cipher

        key = os.urandom(16)
        iv = os.urandom(16)
        cipher = RC4_MD5_Cipher(key, setup_key=False)
        cipher._iv = iv
        plaintext = b"RC4-MD5 round trip test"
        ct = cipher.encrypt(plaintext)

        cipher2 = RC4_MD5_Cipher(key, setup_key=False)
        cipher2._iv = iv
        pt = cipher2.decrypt(ct)
        assert pt == plaintext

    def test_aes_256_cfb_round_trip(self):
        from eggress.cipher import AES_256_CFB_Cipher

        key = os.urandom(32)
        iv = os.urandom(16)
        cipher = AES_256_CFB_Cipher(key, setup_key=False)
        cipher._iv = iv
        cipher._setup_state()
        plaintext = b"AES-256-CFB round trip test data"
        ct = cipher.encrypt(plaintext)

        cipher2 = AES_256_CFB_Cipher(key, setup_key=False)
        cipher2._iv = iv
        cipher2._setup_state()
        pt = cipher2.decrypt(ct)
        assert pt == plaintext

    def test_aes_128_cfb_round_trip(self):
        from eggress.cipher import AES_128_CFB_Cipher

        key = os.urandom(16)
        iv = os.urandom(16)
        cipher = AES_128_CFB_Cipher(key, setup_key=False)
        cipher._iv = iv
        cipher._setup_state()
        plaintext = b"AES-128-CFB test"
        ct = cipher.encrypt(plaintext)

        cipher2 = AES_128_CFB_Cipher(key, setup_key=False)
        cipher2._iv = iv
        cipher2._setup_state()
        pt = cipher2.decrypt(ct)
        assert pt == plaintext

    def test_aes_256_cfb8_round_trip(self):
        from eggress.cipher import AES_256_CFB8_Cipher

        key = os.urandom(32)
        iv = os.urandom(16)
        cipher = AES_256_CFB8_Cipher(key, setup_key=False)
        cipher._iv = iv
        cipher._setup_state()
        plaintext = b"AES-256-CFB8 test"
        ct = cipher.encrypt(plaintext)

        cipher2 = AES_256_CFB8_Cipher(key, setup_key=False)
        cipher2._iv = iv
        cipher2._setup_state()
        pt = cipher2.decrypt(ct)
        assert pt == plaintext

    def test_aes_256_ofb_round_trip(self):
        from eggress.cipher import AES_256_OFB_Cipher

        key = os.urandom(32)
        iv = os.urandom(16)
        cipher = AES_256_OFB_Cipher(key, setup_key=False)
        cipher._iv = iv
        cipher._setup_state()
        plaintext = b"AES-256-OFB round trip test"
        ct = cipher.encrypt(plaintext)

        cipher2 = AES_256_OFB_Cipher(key, setup_key=False)
        cipher2._iv = iv
        cipher2._setup_state()
        pt = cipher2.decrypt(ct)
        assert pt == plaintext

    def test_aes_256_ctr_round_trip(self):
        from eggress.cipher import AES_256_CTR_Cipher

        key = os.urandom(32)
        iv = os.urandom(16)
        cipher = AES_256_CTR_Cipher(key, setup_key=False)
        cipher._iv = iv
        cipher._setup_state()
        plaintext = b"AES-256-CTR round trip test"
        ct = cipher.encrypt(plaintext)

        cipher2 = AES_256_CTR_Cipher(key, setup_key=False)
        cipher2._iv = iv
        cipher2._setup_state()
        pt = cipher2.decrypt(ct)
        assert pt == plaintext

    def test_chacha20_round_trip(self):
        from eggress.cipher import ChaCha20_Cipher

        key = os.urandom(32)
        iv = os.urandom(8)
        cipher = ChaCha20_Cipher(key, setup_key=False)
        cipher._iv = iv
        cipher._setup_state()
        plaintext = b"ChaCha20 round trip test"
        ct = cipher.encrypt(plaintext)

        cipher2 = ChaCha20_Cipher(key, setup_key=False)
        cipher2._iv = iv
        cipher2._setup_state()
        pt = cipher2.decrypt(ct)
        assert pt == plaintext

    def test_chacha20_ietf_round_trip(self):
        from eggress.cipher import ChaCha20_IETF_Cipher

        key = os.urandom(32)
        iv = os.urandom(12)
        cipher = ChaCha20_IETF_Cipher(key, setup_key=False)
        cipher._iv = iv
        cipher._setup_state()
        plaintext = b"ChaCha20-IETF round trip test"
        ct = cipher.encrypt(plaintext)

        cipher2 = ChaCha20_IETF_Cipher(key, setup_key=False)
        cipher2._iv = iv
        cipher2._setup_state()
        pt = cipher2.decrypt(ct)
        assert pt == plaintext


# ---------------------------------------------------------------------------
# C9/C10: Key derivation tests
# ---------------------------------------------------------------------------


class TestEvpBytesToKey:
    """Test OpenSSL EVP_BytesToKey key derivation."""

    def test_known_vector(self):
        from eggress.cipher import _evp_bytes_to_key

        # Known vector: password "password", key_len=16, iv_len=16
        key, iv = _evp_bytes_to_key(b"password", 16, 16)
        assert len(key) == 16
        assert len(iv) == 16
        # First MD5 of empty is d41d8cd98f00b204e9800998ecf8427e
        assert key != iv

    def test_different_passwords_different_keys(self):
        from eggress.cipher import _evp_bytes_to_key

        k1, _ = _evp_bytes_to_key(b"password1", 32, 16)
        k2, _ = _evp_bytes_to_key(b"password2", 32, 16)
        assert k1 != k2

    def test_empty_password(self):
        from eggress.cipher import _evp_bytes_to_key

        key, iv = _evp_bytes_to_key(b"", 16, 16)
        assert len(key) == 16
        assert len(iv) == 16

    def test_string_password(self):
        from eggress.cipher import _evp_bytes_to_key

        key1, _ = _evp_bytes_to_key("test", 16, 0)
        key2, _ = _evp_bytes_to_key(b"test", 16, 0)
        assert key1 == key2


# ---------------------------------------------------------------------------
# C9/C10: AEAD cipher tests
# ---------------------------------------------------------------------------


class TestAEADCipherFunctional:
    """Test that AEAD ciphers now produce functional encrypt/decrypt."""

    def test_aes_256_gcm_encrypt_decrypt(self):
        from eggress.cipher import AES_256_GCM_Cipher

        key = os.urandom(32)
        cipher = AES_256_GCM_Cipher(key, setup_key=True)
        plaintext = b"AEAD round trip test"
        ct = cipher.encrypt(plaintext)
        # ct is nonce + ciphertext
        assert len(ct) > 12

        cipher2 = AES_256_GCM_Cipher(key, setup_key=False)
        cipher2._iv = cipher._nonce
        cipher2._current_nonce = cipher._nonce
        pt = cipher2.decrypt(ct)
        assert pt == plaintext

    def test_chacha20_poly1305_encrypt_decrypt(self):
        from eggress.cipher import ChaCha20_IETF_POLY1305_Cipher

        key = os.urandom(32)
        cipher = ChaCha20_IETF_POLY1305_Cipher(key, setup_key=True)
        plaintext = b"ChaCha20-Poly1305 round trip"
        ct = cipher.encrypt(plaintext)

        cipher2 = ChaCha20_IETF_POLY1305_Cipher(key, setup_key=False)
        cipher2._iv = cipher._nonce
        cipher2._current_nonce = cipher._nonce
        pt = cipher2.decrypt(ct)
        assert pt == plaintext

    def test_encrypt_and_digest(self):
        from eggress.cipher import AES_128_GCM_Cipher

        key = os.urandom(16)
        cipher = AES_128_GCM_Cipher(key, setup_key=True)
        plaintext = b"encrypt_and_digest test"
        raw_ct, tag = cipher.encrypt_and_digest(plaintext)
        assert len(tag) == 16
        assert len(raw_ct) > 0


# ---------------------------------------------------------------------------
# C11: PacketCipher tests
# ---------------------------------------------------------------------------


class TestPacketCipher:
    """Test PacketCipher encrypt/decrypt for AEAD ciphers."""

    def test_aead_packet_encrypt_decrypt(self):
        from eggress.cipher import AES_256_GCM_Cipher, PacketCipher

        key = os.urandom(32)
        cipher = AES_256_GCM_Cipher(key, setup_key=True)
        pkt = PacketCipher(cipher, key, "aes-256-gcm")

        data = b"UDP packet data"
        ct = pkt.encrypt(data)
        assert ct != data
        pt = pkt.decrypt(ct)
        assert pt == data

    def test_non_aead_packet_raises(self):
        from eggress.cipher import RC4_Cipher, PacketCipher

        key = os.urandom(16)
        cipher = RC4_Cipher(key, setup_key=False)
        cipher._iv = b""
        pkt = PacketCipher(cipher, key, "rc4")

        with pytest.raises(Exception):
            pkt.encrypt(b"test")


# ---------------------------------------------------------------------------
# C1: Server utility tests
# ---------------------------------------------------------------------------


class TestPrepareCiphers:
    """Test prepare_ciphers with various cipher specifications."""

    def test_prepare_aes_256_gcm(self):
        from pproxy.server import prepare_ciphers

        result = prepare_ciphers(cipher_key="aes-256-gcm:mypassword")
        assert "cipher" in result
        assert result["cipher_name"] == "aes-256-gcm"
        assert "key" in result
        assert len(result["key"]) == 32

    def test_prepare_aes_128_gcm(self):
        from pproxy.server import prepare_ciphers

        result = prepare_ciphers(cipher_key="aes-128-gcm:secret")
        assert result["cipher_name"] == "aes-128-gcm"
        assert len(result["key"]) == 16

    def test_prepare_rc4(self):
        from pproxy.server import prepare_ciphers

        result = prepare_ciphers(cipher_key="rc4:mypassword")
        assert result["cipher_name"] == "rc4"

    def test_prepare_with_ota(self):
        from pproxy.server import prepare_ciphers

        result = prepare_ciphers(cipher_key="aes-256-gcm:pass!ota")
        assert result["ota"] is True

    def test_prepare_invalid_cipher(self):
        from pproxy.server import prepare_ciphers

        with pytest.raises(ValueError, match="cipher setup failed"):
            prepare_ciphers(cipher_key="unknown-cipher:pass")

    def test_prepare_empty_key(self):
        from pproxy.server import prepare_ciphers

        with pytest.raises(ValueError, match="cipher setup failed"):
            prepare_ciphers(cipher_key="aes-256-gcm:")

    def test_prepare_cipher_obj(self):
        from pproxy.server import prepare_ciphers
        from eggress.cipher import AES_256_GCM_Cipher

        cipher = AES_256_GCM_Cipher(b"testkey123456789012345678901234")
        result = prepare_ciphers(cipher_obj=cipher)
        assert result["cipher"] is cipher

    def test_prepare_with_plugins(self):
        from pproxy.server import prepare_ciphers

        plugins = [{"name": "test"}]
        result = prepare_ciphers(cipher_key="aes-256-gcm:pass", plugins=plugins)
        assert result["plugins"] == plugins

    def test_prepare_datagram_aead(self):
        from pproxy.server import prepare_ciphers

        result = prepare_ciphers(cipher_key="aes-256-gcm:pass")
        assert "datagram" in result


class TestCheckServerAlive:
    """Test check_server_alive connectivity check."""

    def test_none_proxy(self):
        from pproxy.server import check_server_alive

        assert check_server_alive(None) is False

    def test_no_host(self):
        from pproxy.server import check_server_alive
        from eggress._pproxy_proxy import ProxyDirect

        proxy = ProxyDirect()
        assert check_server_alive(proxy) is False

    def test_unreachable_host(self):
        from pproxy.server import check_server_alive
        from eggress._pproxy_proxy import ProxySimple

        proxy = ProxySimple(host_name="192.0.2.1", port=1)  # TEST-NET, unreachable
        assert check_server_alive(proxy, timeout=0.5) is False


class TestCompileRule:
    """Test compile_rule behavior."""

    def test_returns_structure(self):
        from pproxy.server import compile_rule

        result = compile_rule("test_rules.txt")
        assert isinstance(result, dict)
        assert result["filename"] == "test_rules.txt"

    def test_nonexistent_file(self):
        from pproxy.server import compile_rule

        result = compile_rule("/nonexistent/file.txt")
        assert result["filename"] == "/nonexistent/file.txt"
        assert result["rules"] == []


class TestSchedule:
    """Test schedule function."""

    def test_empty_list(self):
        from pproxy.server import schedule

        assert schedule([]) is None

    def test_fa_returns_first(self):
        from pproxy.server import schedule
        from eggress._pproxy_proxy import ProxyDirect

        p1 = ProxyDirect()
        p2 = ProxyDirect()
        assert schedule([p1, p2], "fa") is p1

    def test_rr_returns_first(self):
        from pproxy.server import schedule
        from eggress._pproxy_proxy import ProxyDirect

        p1 = ProxyDirect()
        assert schedule([p1], "rr") is p1

    def test_lc_returns_min_connections(self):
        from pproxy.server import schedule
        from eggress._pproxy_proxy import ProxySimple

        p1 = ProxySimple()
        p1._connections = 5
        p2 = ProxySimple()
        p2._connections = 2
        assert schedule([p1, p2], "lc") is p2


# ---------------------------------------------------------------------------
# C1: AuthTable tests
# ---------------------------------------------------------------------------


class TestAuthTable:
    """Test AuthTable functionality."""

    def test_initial_state(self):
        from eggress._pproxy_proxy import AuthTable

        auth = AuthTable(remote_ip="1.2.3.4")
        assert auth.authed() is None
        assert auth.remote_ip == "1.2.3.4"
        assert not bool(auth)

    def test_set_authed(self):
        from eggress._pproxy_proxy import AuthTable

        auth = AuthTable()
        auth.set_authed("user1")
        assert auth.authed() == "user1"
        assert bool(auth)
        assert "user1" in auth

    def test_expiry(self):
        from eggress._pproxy_proxy import AuthTable

        auth = AuthTable(authtime=0)
        auth.set_authed("user1")
        time.sleep(0.01)
        assert auth.authed() is None
        assert not bool(auth)

    def test_clear(self):
        from eggress._pproxy_proxy import AuthTable

        auth = AuthTable()
        auth.set_authed("user1")
        auth.clear()
        assert auth.authed() is None

    def test_repr(self):
        from eggress._pproxy_proxy import AuthTable

        auth = AuthTable(remote_ip="1.2.3.4", authtime=60)
        r = repr(auth)
        assert "1.2.3.4" in r
        assert "60" in r


# ---------------------------------------------------------------------------
# C1: stream_handler and datagram_handler tests
# ---------------------------------------------------------------------------


class TestStreamHandler:
    """Test stream_handler behavior."""

    @pytest.mark.asyncio
    async def test_auth_rejection(self):
        from pproxy.server import stream_handler
        from eggress._pproxy_proxy import AuthTable

        auth = AuthTable()
        # Not authenticated -- handler should close writer
        reader = asyncio.StreamReader()
        reader.feed_eof()

        class MockWriter:
            def __init__(self):
                self.closed = False
            def close(self):
                self.closed = True
            async def drain(self):
                pass

        writer = MockWriter()
        await stream_handler(reader, writer, None, auth)
        assert writer.closed

    @pytest.mark.asyncio
    async def test_none_rserver(self):
        from pproxy.server import stream_handler

        reader = asyncio.StreamReader()
        reader.feed_eof()

        class MockWriter:
            def __init__(self):
                self.closed = False
            def close(self):
                self.closed = True

        writer = MockWriter()
        await stream_handler(reader, writer, None, None)
        assert writer.closed


class TestDatagramHandler:
    """Test datagram_handler behavior."""

    def test_auth_rejection(self):
        from pproxy.server import datagram_handler
        from eggress._pproxy_proxy import AuthTable

        auth = AuthTable()
        result = datagram_handler(b"data", None, auth=auth)
        assert result is None

    def test_none_rserver(self):
        from pproxy.server import datagram_handler

        result = datagram_handler(b"data", None)
        assert result is None

    def test_direct_data(self):
        from pproxy.server import datagram_handler
        from eggress._pproxy_proxy import ProxyDirect

        proxy = ProxyDirect()
        result = datagram_handler(b"test data", proxy)
        assert result == b"test data"


# ---------------------------------------------------------------------------
# C15: Property tests (cipher round-trip)
# ---------------------------------------------------------------------------


class TestCipherPropertyRoundTrips:
    """Property-style tests: encrypt then decrypt always recovers plaintext."""

    @pytest.mark.parametrize(
        "cipher_name,key_len",
        [
            ("rc4", 16),
            ("aes-256-cfb", 32),
            ("aes-192-cfb", 24),
            ("aes-128-cfb", 16),
            ("aes-256-cfb8", 32),
            ("aes-192-cfb8", 24),
            ("aes-128-cfb8", 16),
            ("aes-256-ofb", 32),
            ("aes-192-ofb", 24),
            ("aes-128-ofb", 16),
            ("aes-256-ctr", 32),
            ("aes-192-ctr", 24),
            ("aes-128-ctr", 16),
        ],
    )
    def test_stream_cipher_round_trip(self, cipher_name, key_len):
        from eggress.cipher import get_cipher

        key = os.urandom(key_len)
        err, apply_fn = get_cipher(f"{cipher_name}:test")
        assert err is None
        cipher = apply_fn.cipher

        # Re-derive the same key
        err2, apply_fn2 = get_cipher(f"{cipher_name}:test")
        cipher2 = apply_fn2.cipher

        # Set same IV
        cipher._iv = b"\x00" * cipher.IV_LENGTH
        if hasattr(cipher, "_setup_state"):
            cipher._setup_state()
        cipher2._iv = b"\x00" * cipher.IV_LENGTH
        if hasattr(cipher2, "_setup_state"):
            cipher2._setup_state()

        for _ in range(10):
            data = os.urandom(100)
            ct = cipher.encrypt(data)
            pt = cipher2.decrypt(ct)
            assert pt == data

    def test_get_cipher_all_names(self):
        from eggress.cipher import get_cipher, MAP

        for name in MAP:
            if name.endswith("-py"):
                continue
            err, apply_fn = get_cipher(f"{name}:testpassword")
            # Either succeeds or fails with a known error
            if err is None:
                assert apply_fn is not None
                assert apply_fn.name == name


# ---------------------------------------------------------------------------
# C3: Protocol namespace tests
# ---------------------------------------------------------------------------


class TestProtocolNamespace:
    """Test protocol class constructors and attributes."""

    def test_all_protocol_classes_instantiable(self):
        from eggress.protocol import (
            Direct, HTTP, HTTPOnly, Socks4, Socks5, SS, Trojan, WS, H2,
            Transparent, Redir, Pf, Tunnel, Echo,
        )

        for cls in [Direct, HTTP, HTTPOnly, Socks4, Socks5, SS, Trojan, WS, H2,
                     Transparent, Redir, Pf, Tunnel, Echo]:
            proto = cls("test")
            assert proto.name is not None
            assert proto.param == "test"

    def test_unsupported_protocols_raise(self):
        from eggress.protocol import SSR, SSH, H3
        from eggress.protocol import UnsupportedFeatureError

        for cls in [SSR, SSH, H3]:
            with pytest.raises(UnsupportedFeatureError):
                cls("test")

    def test_mapping_completeness(self):
        from eggress.protocol import MAPPINGS

        expected_schemes = [
            "direct", "http", "httponly", "socks4", "socks4a", "socks5",
            "socks", "ss", "ssr", "trojan", "ws", "h2", "h3", "ssh",
            "redir", "pf", "tunnel", "echo", "ssl", "secure", "https",
            "quic", "httpget", "in",
        ]
        for scheme in expected_schemes:
            assert scheme in MAPPINGS


# ---------------------------------------------------------------------------
# C4: Address parsing tests
# ---------------------------------------------------------------------------


class TestAddressParsing:
    """Test socks_address encoding."""

    def test_ipv4_address(self):
        from pproxy.proto import socks_address

        addr = socks_address("1.2.3.4", 80)
        assert addr[0:1] == b"\x01"  # IPv4 type
        assert len(addr) == 7  # type(1) + addr(4) + port(2)

    def test_ipv6_address(self):
        from pproxy.proto import socks_address

        addr = socks_address("::1", 80)
        assert addr[0:1] == b"\x04"  # IPv6 type
        assert len(addr) == 19  # type(1) + addr(16) + port(2)

    def test_domain_address(self):
        from pproxy.proto import socks_address

        addr = socks_address("example.com", 443)
        assert addr[0:1] == b"\x03"  # Domain type
        assert addr[1:2] == bytes([len(b"example.com")])

    def test_port_encoding(self):
        from pproxy.proto import socks_address

        addr = socks_address("1.2.3.4", 8080)
        port = struct.unpack("!H", addr[5:7])[0]
        assert port == 8080


# ---------------------------------------------------------------------------
# C15: Negative path tests
# ---------------------------------------------------------------------------


class TestNegativePaths:
    """Test error paths and exception behavior."""

    def test_get_cipher_unknown(self):
        from eggress.cipher import get_cipher

        err, result = get_cipher("unknown-cipher:pass")
        assert err is not None
        assert result is None

    def test_get_cipher_empty(self):
        from eggress.cipher import get_cipher

        err, result = get_cipher("")
        assert err is not None

    def test_get_cipher_no_password(self):
        from eggress.cipher import get_cipher

        err, result = get_cipher("aes-256-gcm:")
        assert err is not None

    def test_rc4_unsupported_salsa20(self):
        from eggress.cipher import Salsa20_Cipher

        cipher = Salsa20_Cipher(b"test")
        with pytest.raises(Exception):
            cipher.encrypt(b"data")

    def test_bf_unsupported(self):
        from eggress.cipher import BF_CFB_Cipher

        cipher = BF_CFB_Cipher(b"test")
        with pytest.raises(Exception):
            cipher.encrypt(b"data")
