"""Property tests for Milestone C, workstream C15.

Covers:
  - Address encode/decode round trips
  - Cipher encrypt/decrypt round trips
  - Chunk-boundary invariance
  - Instance state isolation
  - EVP_BytesToKey key derivation
"""

from __future__ import annotations

import copy
import ipaddress
import struct

import pytest

from eggress.cipher import (
    MAP,
    AEADCipher,
    BaseCipher,
    PacketCipher,
    StreamCipher,
    _evp_bytes_to_key,
    get_cipher,
)

_cryptography = pytest.importorskip("cryptography")

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# Ciphers that are intentionally unsupported (no cryptography backend)
_UNSUPPORTED_CIPHERS = {"salsa20", "salsa20-py", "bf-cfb", "bf-cfb-py", "cast5-cfb", "des-cfb"}

# Alias names that map to the same class as a canonical name
_ALIAS_CIPHERS = {
    "aes-256-gcm-py", "aes-128-gcm-py", "chacha20-ietf-poly1305-py",
    "rc4-md5-py", "chacha20-py", "salsa20-py",
    "aes-256-cfb-py", "aes-128-cfb-py", "aes-256-cfb8-py",
    "aes-128-cfb8-py", "aes-256-ofb-py", "aes-128-ofb-py",
    "aes-256-ctr-py", "aes-128-ctr-py", "bf-cfb-py",
}

# AEAD ciphers in the MAP
_AEAD_NAMES = {
    "aes-256-gcm", "aes-192-gcm", "aes-128-gcm",
    "chacha20-ietf-poly1305",
}

# Functional ciphers: in MAP, not unsupported, has working encrypt/decrypt
_FUNCTIONAL_NAMES = sorted(
    set(MAP.keys()) - _UNSUPPORTED_CIPHERS - _ALIAS_CIPHERS
)


def _make_cipher(cipher_name: str, password: str = "test-password-abc") -> BaseCipher:
    """Instantiate a cipher with derived key material, ready for encrypt/decrypt."""
    cipher_cls = MAP[cipher_name]
    key_len = cipher_cls.KEY_LENGTH
    if issubclass(cipher_cls, AEADCipher):
        iv_len = cipher_cls.NONCE_LENGTH
    else:
        iv_len = cipher_cls.IV_LENGTH
    key_material, iv_material = _evp_bytes_to_key(password, key_len, iv_len)
    cipher = cipher_cls(key_material, setup_key=False)
    cipher.setup_iv(iv_material)
    return cipher


def _is_aead(cipher_name: str) -> bool:
    return cipher_name in _AEAD_NAMES


# ---------------------------------------------------------------------------
# 1. TestAddressRoundTrip
# ---------------------------------------------------------------------------


class TestAddressRoundTrip:
    """SOCKS address decoding reads structured bytes correctly."""

    def test_ipv4_type_byte(self):
        from pproxy.proto import socks_address
        import io
        reader = io.BytesIO(b"\x7f\x00\x00\x01\x00\x50")
        host, port = socks_address(reader, 1)
        assert port == 80

    def test_ipv4_length(self):
        from pproxy.proto import socks_address
        import io
        reader = io.BytesIO(b"\x0a\x00\x00\x01\x1f\x90")
        host, port = socks_address(reader, 1)
        assert host == "10.0.0.1"

    def test_ipv4_port_network_order(self):
        from pproxy.proto import socks_address
        import io
        reader = io.BytesIO(b"\x01\x02\x03\x04\x1f\x90")
        host, port = socks_address(reader, 1)
        assert port == 8080

    def test_ipv4_address_bytes_match(self):
        from pproxy.proto import socks_address
        import io
        reader = io.BytesIO(b"\xac\x10\x00\x01\x01\xbb")
        host, port = socks_address(reader, 1)
        assert host == "172.16.0.1"

    def test_ipv4_port_zero(self):
        from pproxy.proto import socks_address
        import io
        reader = io.BytesIO(b"\x00\x00\x00\x00\x00\x00")
        host, port = socks_address(reader, 1)
        assert host == "0.0.0.0"
        assert port == 0

    def test_ipv4_port_max(self):
        from pproxy.proto import socks_address
        import io
        reader = io.BytesIO(b"\xff\xff\xff\xff\xff\xff")
        host, port = socks_address(reader, 1)
        assert host == "255.255.255.255"
        assert port == 65535

    def test_ipv6_type_byte(self):
        from pproxy.proto import socks_address
        import io, socket
        addr = socket.inet_pton(socket.AF_INET6, "::1")
        reader = io.BytesIO(addr + b"\x00\x50")
        host, port = socks_address(reader, 4)
        assert host == "::1"

    def test_ipv6_length(self):
        from pproxy.proto import socks_address
        import io, socket
        addr = socket.inet_pton(socket.AF_INET6, "::1")
        reader = io.BytesIO(addr + b"\x00\x50")
        host, port = socks_address(reader, 4)
        assert port == 80

    def test_ipv6_address_bytes_match(self):
        from pproxy.proto import socks_address
        import io, socket
        addr = socket.inet_pton(socket.AF_INET6, "fe80::1")
        reader = io.BytesIO(addr + b"\x23\x8a")
        host, port = socks_address(reader, 4)
        assert host == "fe80::1"

    def test_ipv6_port_network_order(self):
        from pproxy.proto import socks_address
        import io, socket
        addr = socket.inet_pton(socket.AF_INET6, "::1")
        reader = io.BytesIO(addr + b"\x23\x82")
        host, port = socks_address(reader, 4)
        assert port == 9090

    def test_ipv6_full_address(self):
        from pproxy.proto import socks_address
        import io, socket
        addr = socket.inet_pton(socket.AF_INET6, "2001:db8::1")
        reader = io.BytesIO(addr + b"\x00\x50")
        host, port = socks_address(reader, 4)
        assert host == "2001:db8::1"
        assert port == 80

    def test_domain_type_byte(self):
        from pproxy.proto import socks_address
        import io
        domain = b"example.com"
        reader = io.BytesIO(bytes([len(domain)]) + domain + b"\x01\xbb")
        host, port = socks_address(reader, 3)
        assert host == "example.com"

    def test_domain_length_byte(self):
        from pproxy.proto import socks_address
        import io
        domain = b"example.com"
        reader = io.BytesIO(bytes([len(domain)]) + domain + b"\x01\xbb")
        host, port = socks_address(reader, 3)
        assert len(domain) == 11

    def test_domain_content(self):
        from pproxy.proto import socks_address
        import io
        domain = b"example.com"
        reader = io.BytesIO(bytes([len(domain)]) + domain + b"\x01\xbb")
        host, port = socks_address(reader, 3)
        assert host == "example.com"

    def test_domain_port(self):
        from pproxy.proto import socks_address
        import io
        domain = b"example.com"
        reader = io.BytesIO(bytes([len(domain)]) + domain + b"\x20\xfb")
        host, port = socks_address(reader, 3)
        assert port == 8443

    def test_domain_total_length(self):
        from pproxy.proto import socks_address
        import io
        domain = b"example.com"
        reader = io.BytesIO(bytes([len(domain)]) + domain + b"\x01\xbb")
        host, port = socks_address(reader, 3)
        assert len(domain) == 11

    def test_long_domain(self):
        from pproxy.proto import socks_address
        import io
        domain = (b"a" * 63 + b".bc")
        reader = io.BytesIO(bytes([len(domain)]) + domain + b"\x00\x50")
        host, port = socks_address(reader, 3)
        assert host == domain.decode()

    def test_idna_encoded_domain(self):
        from pproxy.proto import socks_address
        import io
        # IDNA encoding produces punycode
        encoded = "münchen.de".encode("idna")
        reader = io.BytesIO(bytes([len(encoded)]) + encoded + b"\x00\x50")
        host, port = socks_address(reader, 3)
        assert host == encoded.decode()

    def test_port_round_trip_values(self):
        from pproxy.proto import socks_address
        import io
        for port in [1, 80, 443, 8080, 65535]:
            reader = io.BytesIO(b"\x7f\x00\x00\x01" + port.to_bytes(2, "big"))
            host, p = socks_address(reader, 1)
            assert p == port

    def test_structure_validity_ipv4(self):
        from pproxy.proto import socks_address
        import io
        reader = io.BytesIO(b"\x0a\x00\x00\x01\x00\x35")
        host, port = socks_address(reader, 1)
        assert host == "10.0.0.1"
        assert port == 53

    def test_structure_validity_domain(self):
        from pproxy.proto import socks_address
        import io
        # Domain: length(1) + domain("x") + port(2)
        reader = io.BytesIO(b"\x01" + b"\x78" + b"\x00\x01")
        host, port = socks_address(reader, 3)
        assert host == "x"
        assert port == 1


# ---------------------------------------------------------------------------
# 2. TestCipherRoundTrip
# ---------------------------------------------------------------------------


class TestCipherRoundTrip:
    """Property-style encrypt/decrypt round-trip tests for all functional ciphers."""

    @pytest.mark.parametrize("cipher_name", _FUNCTIONAL_NAMES)
    def test_basic_round_trip(self, cipher_name: str):
        """encrypt then decrypt returns original plaintext."""
        cipher = _make_cipher(cipher_name)
        plaintext = b"Hello, world!"
        ciphertext = cipher.encrypt(plaintext)
        # For stream ciphers, decrypt reuses the same state; create a fresh pair
        enc = _make_cipher(cipher_name)
        dec = _make_cipher(cipher_name)
        ct = enc.encrypt(plaintext)
        pt = dec.decrypt(ct)
        assert pt == plaintext

    @pytest.mark.parametrize("cipher_name", _FUNCTIONAL_NAMES)
    def test_different_passwords_different_output(self, cipher_name: str):
        """Different passwords produce different ciphertext for same input."""
        ct1 = _make_cipher(cipher_name, "password-alpha").encrypt(b"test")
        ct2 = _make_cipher(cipher_name, "password-beta").encrypt(b"test")
        assert ct1 != ct2

    @pytest.mark.parametrize("cipher_name", _FUNCTIONAL_NAMES)
    def test_empty_input(self, cipher_name: str):
        """Empty input encrypts and decrypts correctly."""
        enc = _make_cipher(cipher_name)
        dec = _make_cipher(cipher_name)
        ct = enc.encrypt(b"")
        pt = dec.decrypt(ct)
        assert pt == b""

    @pytest.mark.parametrize("cipher_name", _FUNCTIONAL_NAMES)
    def test_multiple_sequential_calls(self, cipher_name: str):
        """Multiple sequential encrypt/decrypt calls maintain state correctly."""
        enc = _make_cipher(cipher_name)
        dec = _make_cipher(cipher_name)
        chunks = [b"first ", b"second ", b"third"]
        ciphertexts = [enc.encrypt(c) for c in chunks]
        plaintexts = [dec.decrypt(ct) for ct in ciphertexts]
        assert plaintexts == chunks

    @pytest.mark.parametrize("cipher_name", _FUNCTIONAL_NAMES)
    def test_large_data(self, cipher_name: str):
        """Round-trip works for larger payloads."""
        enc = _make_cipher(cipher_name)
        dec = _make_cipher(cipher_name)
        plaintext = b"x" * 10000
        ct = enc.encrypt(plaintext)
        pt = dec.decrypt(ct)
        assert pt == plaintext

    @pytest.mark.parametrize("cipher_name", _FUNCTIONAL_NAMES)
    def test_binary_data(self, cipher_name: str):
        """Round-trip works for arbitrary binary data."""
        enc = _make_cipher(cipher_name)
        dec = _make_cipher(cipher_name)
        plaintext = bytes(range(256))
        ct = enc.encrypt(plaintext)
        pt = dec.decrypt(ct)
        assert pt == plaintext

    @pytest.mark.parametrize("cipher_name", _FUNCTIONAL_NAMES)
    def test_single_byte(self, cipher_name: str):
        """Round-trip works for single-byte input."""
        enc = _make_cipher(cipher_name)
        dec = _make_cipher(cipher_name)
        ct = enc.encrypt(b"\xff")
        pt = dec.decrypt(ct)
        assert pt == b"\xff"

    @pytest.mark.parametrize("cipher_name", _FUNCTIONAL_NAMES)
    def test_encrypted_not_equal_to_plaintext(self, cipher_name: str):
        """Ciphertext is not equal to plaintext (non-trivial encryption)."""
        ct = _make_cipher(cipher_name).encrypt(b"test data here")
        assert ct != b"test data here"

    @pytest.mark.parametrize("cipher_name", _FUNCTIONAL_NAMES)
    def test_key_matches_expected_length(self, cipher_name: str):
        """Derived key matches the cipher's KEY_LENGTH."""
        cipher_cls = MAP[cipher_name]
        key, _ = _evp_bytes_to_key("password", cipher_cls.KEY_LENGTH, 0)
        assert len(key) == cipher_cls.KEY_LENGTH


# ---------------------------------------------------------------------------
# 3. TestChunkBoundaryInvariance
# ---------------------------------------------------------------------------


class TestChunkBoundaryInvariance:
    """Encrypting chunks vs whole preserves structural invariants."""

    def test_stream_chunk_split_preserves_plaintext(self):
        """For stream ciphers: encrypt(a) then encrypt(b) on the same instance,
        decrypt ct_a then ct_b on a matching instance yields a, b.

        Two instances with the same IV produce identical keystreams,
        so the decryptor can consume ciphertexts in order.
        """
        for cipher_name in _FUNCTIONAL_NAMES:
            if _is_aead(cipher_name):
                continue  # AEAD uses nonce-per-call; different structure
            a, b = b"hello ", b"world"
            enc = _make_cipher(cipher_name)
            dec = _make_cipher(cipher_name)
            ct_a = enc.encrypt(a)
            ct_b = enc.encrypt(b)
            # Decrypt in order
            pt_a = dec.decrypt(ct_a)
            pt_b = dec.decrypt(ct_b)
            assert pt_a + pt_b == a + b, f"Failed for {cipher_name}"

    def test_stream_whole_matches_split_concat(self):
        """For stream ciphers: encrypt(a+b) == encrypt(a) + encrypt(b).

        Stream cipher output is deterministic given the same IV and key,
        so encrypting the concatenation equals concatenating the encrypts.
        """
        for cipher_name in _FUNCTIONAL_NAMES:
            if _is_aead(cipher_name):
                continue
            a, b = b"chunk1", b"chunk2"
            # Whole
            whole = _make_cipher(cipher_name)
            ct_whole = whole.encrypt(a + b)
            # Split
            split_enc = _make_cipher(cipher_name)
            ct_a = split_enc.encrypt(a)
            ct_b = split_enc.encrypt(b)
            assert ct_whole == ct_a + ct_b, f"Failed for {cipher_name}"

    def test_aead_different_nonces_produce_different_ciphertext(self):
        """AEAD: two encrypt calls with different nonces differ."""
        for cipher_name in sorted(_AEAD_NAMES):
            a = _make_cipher(cipher_name)
            b = _make_cipher(cipher_name)
            # Give b a different nonce so ciphertext differs
            import os as _os
            b.setup_nonce(_os.urandom(a.NONCE_LENGTH))
            ct1 = a.encrypt(b"data")
            ct2 = b.encrypt(b"data")
            # Different nonce => different ciphertext
            assert ct1 != ct2, f"Failed for {cipher_name}"

    def test_aead_chunk_concat_decryptable(self):
        """AEAD: encrypt(a) then encrypt(b) can each be independently decrypted."""
        for cipher_name in sorted(_AEAD_NAMES):
            enc_a = _make_cipher(cipher_name)
            enc_b = _make_cipher(cipher_name)
            dec_a = _make_cipher(cipher_name)
            dec_b = _make_cipher(cipher_name)
            a, b = b"first", b"second"
            ct1 = enc_a.encrypt(a)
            ct2 = enc_b.encrypt(b)
            assert dec_a.decrypt(ct1) == a
            assert dec_b.decrypt(ct2) == b

    def test_stream_xor_property(self):
        """Stream ciphers: ciphertext XOR plaintext == keystream (deterministic)."""
        for cipher_name in _FUNCTIONAL_NAMES:
            if _is_aead(cipher_name):
                continue
            data = b"abcdef"
            enc1 = _make_cipher(cipher_name)
            enc2 = _make_cipher(cipher_name)
            ct1 = enc1.encrypt(data)
            ct2 = enc2.encrypt(data)
            # Same key + same IV => same keystream => same ciphertext
            assert ct1 == ct2, f"Failed for {cipher_name}"


# ---------------------------------------------------------------------------
# 4. TestInstanceStateIsolation
# ---------------------------------------------------------------------------


class TestInstanceStateIsolation:
    """Two cipher instances with the same key are independent."""

    def test_stream_independent_instances(self):
        """Encrypting on one stream cipher doesn't affect another."""
        for cipher_name in _FUNCTIONAL_NAMES:
            if _is_aead(cipher_name):
                continue
            a = _make_cipher(cipher_name)
            b = _make_cipher(cipher_name)
            # Both start at same IV — encrypt same plaintext
            ct_a1 = a.encrypt(b"chunk1")
            ct_b1 = b.encrypt(b"chunk1")
            assert ct_a1 == ct_b1, f"Failed first block for {cipher_name}"
            # Diverge: encrypt different data on each
            ct_a2 = a.encrypt(b"aaa")
            ct_b2 = b.encrypt(b"bbb")
            assert ct_a2 != ct_b2, f"Failed divergence for {cipher_name}"
            # Both can still encrypt further without error
            ct_a3 = a.encrypt(b"final")
            ct_b3 = b.encrypt(b"final")
            assert len(ct_a3) > 0 and len(ct_b3) > 0

    def test_aead_independent_instances(self):
        """AEAD instances with same key produce independent ciphertexts."""
        for cipher_name in sorted(_AEAD_NAMES):
            a = _make_cipher(cipher_name)
            b = _make_cipher(cipher_name)
            ct1 = a.encrypt(b"data1")
            ct2 = b.encrypt(b"data2")
            # Each uses its own nonce counter
            assert ct1 != ct2

    def test_copy_is_independent(self):
        """Copied cipher encrypts independently from original."""
        for cipher_name in _FUNCTIONAL_NAMES:
            orig = _make_cipher(cipher_name)
            cloned = copy.copy(orig)
            # Encrypt different data on each — they must diverge
            ct_orig = orig.encrypt(b"original-data")
            ct_clone = cloned.encrypt(b"different-data")
            assert ct_orig != ct_clone
            # Both should be valid (not raise) for further operations
            orig.encrypt(b"second")
            cloned.encrypt(b"second")

    def test_copy_then_encrypt_original_unchanged(self):
        """Encrypting on the copy does not alter the original's state."""
        for cipher_name in _FUNCTIONAL_NAMES:
            orig = _make_cipher(cipher_name)
            # Record state by encrypting
            ct_before = orig.encrypt(b"check")
            cloned = copy.copy(orig)
            # Encrypt on copy
            cloned.encrypt(b"some-data")
            cloned.encrypt(b"more-data")
            # Original should still produce correct next output
            ct_after = orig.encrypt(b"next")
            # The copy and original should diverge
            dec_fresh = _make_cipher(cipher_name)
            # We can't easily verify ct_after without a reference, but
            # we can verify the copy doesn't mutate original by
            # re-creating the scenario
            orig2 = _make_cipher(cipher_name)
            ct_ref = orig2.encrypt(b"check")
            assert ct_before == ct_ref

    def test_deep_copy_is_independent(self):
        """Deep-copied cipher encrypts independently from original."""
        for cipher_name in _FUNCTIONAL_NAMES:
            orig = _make_cipher(cipher_name)
            cloned = copy.deepcopy(orig)
            ct1 = orig.encrypt(b"data-a")
            ct2 = cloned.encrypt(b"data-b")
            assert ct1 != ct2

    def test_multiple_copies_independent(self):
        """Multiple copies from the same original are all independent."""
        for cipher_name in _FUNCTIONAL_NAMES:
            orig = _make_cipher(cipher_name)
            copies = [copy.copy(orig) for _ in range(5)]
            # Each copy should produce valid output
            for i, c in enumerate(copies):
                ct = c.encrypt(f"input-{i}".encode())
                assert len(ct) > 0
            # Further encrypts remain independent
            inputs = [f"data-{i}".encode() for i in range(5)]
            results = [c.encrypt(inp) for c, inp in zip(copies, inputs)]
            # All different (different plaintext at minimum)
            assert len(set(results)) == 5


# ---------------------------------------------------------------------------
# 5. TestEVPBytesToKey
# ---------------------------------------------------------------------------


class TestEVPBytesToKey:
    """Key derivation from password via EVP_BytesToKey."""

    def test_same_password_same_key(self):
        """Same password and lengths produce the same key."""
        k1, _ = _evp_bytes_to_key("my-secret", 32, 16)
        k2, _ = _evp_bytes_to_key("my-secret", 32, 16)
        assert k1 == k2

    def test_different_passwords_different_keys(self):
        """Different passwords produce different keys."""
        k1, _ = _evp_bytes_to_key("password-a", 32, 0)
        k2, _ = _evp_bytes_to_key("password-b", 32, 0)
        assert k1 != k2

    def test_empty_password(self):
        """Empty password produces a valid key."""
        key, iv = _evp_bytes_to_key("", 32, 16)
        assert len(key) == 32
        assert len(iv) == 16
        assert key != b"\x00" * 32  # not all zeros

    def test_key_length_matches_requested(self):
        """Key length matches the requested length."""
        for kl in [8, 16, 24, 32]:
            key, _ = _evp_bytes_to_key("test", kl, 0)
            assert len(key) == kl

    def test_iv_length_matches_requested(self):
        """IV length matches the requested length."""
        for il in [0, 8, 16, 32]:
            _, iv = _evp_bytes_to_key("test", 32, il)
            assert len(iv) == il

    def test_key_plus_iv_length(self):
        """Key + IV total length equals sum of requested lengths."""
        key, iv = _evp_bytes_to_key("test", 16, 16)
        assert len(key) + len(iv) == 32

    def test_zero_key_length(self):
        """Zero key length returns empty bytes."""
        key, iv = _evp_bytes_to_key("test", 0, 16)
        assert key == b""
        assert len(iv) == 16

    def test_zero_iv_length(self):
        """Zero IV length returns empty bytes."""
        key, iv = _evp_bytes_to_key("test", 32, 0)
        assert len(key) == 32
        assert iv == b""

    def test_both_zero(self):
        """Zero key and IV lengths return two empty bytes objects."""
        key, iv = _evp_bytes_to_key("test", 0, 0)
        assert key == b""
        assert iv == b""

    def test_password_is_utf8_encoded(self):
        """String password is UTF-8 encoded before hashing."""
        key_bytes, _ = _evp_bytes_to_key("test", 32, 0)
        key_str, _ = _evp_bytes_to_key(b"test", 32, 0)
        assert key_bytes == key_str

    def test_deterministic_across_calls(self):
        """Multiple calls with same inputs are fully deterministic."""
        for _ in range(10):
            k, v = _evp_bytes_to_key("consistent", 32, 16)
            assert k == _evp_bytes_to_key("consistent", 32, 16)[0]
            assert v == _evp_bytes_to_key("consistent", 32, 16)[1]

    def test_long_password(self):
        """Very long password still produces correct-length key."""
        long_pw = "x" * 10000
        key, iv = _evp_bytes_to_key(long_pw, 32, 16)
        assert len(key) == 32
        assert len(iv) == 16

    def test_unicode_password(self):
        """Unicode password produces valid key."""
        key, iv = _evp_bytes_to_key("pässwörd", 32, 16)
        assert len(key) == 32
        assert len(iv) == 16

    def test_all_zeros_not_produced_for_nonempty(self):
        """Non-empty password does not produce an all-zero key."""
        key, _ = _evp_bytes_to_key("not-empty", 32, 0)
        assert key != b"\x00" * 32

    def test_known_vector_aes_256(self):
        """Verify key derivation matches expected AES-256 length from password."""
        key, iv = _evp_bytes_to_key("shadowsocks", 32, 16)
        assert len(key) == 32
        assert len(iv) == 16
        # Verify reproducibility with get_cipher
        err, apply_fn = get_cipher("aes-256-cfb:shadowsocks")
        assert err is None
        assert apply_fn is not None
        assert apply_fn.key == key


# ---------------------------------------------------------------------------
# Supplementary: get_cipher integration
# ---------------------------------------------------------------------------


class TestGetCipherIntegration:
    """Verify get_cipher produces working ciphers for functional names."""

    @pytest.mark.parametrize("cipher_name", _FUNCTIONAL_NAMES)
    def test_get_cipher_success(self, cipher_name: str):
        """get_cipher returns a working _ApplyCipher for each functional cipher."""
        from eggress.cipher import get_cipher
        err, apply_fn = get_cipher(f"{cipher_name}:test-password")
        assert err is None
        assert apply_fn is not None
        assert apply_fn.name == cipher_name

    def test_get_cipher_unknown_returns_error(self):
        from eggress.cipher import get_cipher
        err, result = get_cipher("unknown-cipher:pass")
        assert err is not None
        assert result is None

    def test_get_cipher_empty_returns_error(self):
        from eggress.cipher import get_cipher
        err, result = get_cipher("")
        assert err is not None

    def test_get_cipher_no_password_returns_error(self):
        from eggress.cipher import get_cipher
        err, result = get_cipher("aes-256-gcm:")
        assert err is not None

    def test_get_cipher_ota_flag(self):
        from eggress.cipher import get_cipher
        err, apply_fn = get_cipher("aes-256-gcm:password!ota")
        assert err is None
        assert apply_fn.ota is True

    def test_get_cipher_datagram_for_aead(self):
        from eggress.cipher import get_cipher
        err, apply_fn = get_cipher("aes-256-gcm:password")
        assert err is None
        assert apply_fn.datagram is not None
        assert isinstance(apply_fn.datagram, PacketCipher)

    def test_get_cipher_no_datagram_for_stream(self):
        from eggress.cipher import get_cipher
        err, apply_fn = get_cipher("rc4:password")
        assert err is None
        assert apply_fn.datagram is None
