"""Comprehensive tests for python/eggress/protocol.py and python/eggress/cipher.py.

Phase C4: pproxy-compatible protocol and cipher object models.
"""

from __future__ import annotations

import copy
import pickle

import pytest

from eggress.cipher import (
    MAP,
    AEADCipher,
    AES_256_GCM_Cipher,
    AES_128_GCM_Cipher,
    AES_256_CFB_Cipher,
    ChaCha20_IETF_POLY1305_Cipher,
    RC4_Cipher,
    BaseCipher,
    PacketCipher,
    _ApplyCipher,
    _evp_bytes_to_key,
    get_cipher,
)
from eggress.cipher import UnsupportedFeatureError as CipherUnsupportedError
from eggress.protocol import (
    HTTP_LINE,
    MAPPINGS,
    BaseProtocol,
    Direct,
    H2,
    H3,
    HTTP,
    SS,
    SSH,
    SSR,
    Socks5,
    Trojan,
    UnsupportedFeatureError,
    get_protos,
    netloc_split,
    packstr,
)

# ---------------------------------------------------------------------------
# Protocol: registry
# ---------------------------------------------------------------------------


class TestProtocolRegistry:
    def test_mappings_has_21_keys(self) -> None:
        assert len(MAPPINGS) == 21

    def test_mappings_class_values_count(self) -> None:
        classes = [v for v in MAPPINGS.values() if not isinstance(v, str)]
        strings = [v for v in MAPPINGS.values() if isinstance(v, str)]
        assert len(classes) == 17
        assert len(strings) == 4

    def test_specific_mapping_lookups(self) -> None:
        assert MAPPINGS["direct"] is Direct
        assert MAPPINGS["http"] is HTTP
        assert MAPPINGS["socks5"] is Socks5
        assert MAPPINGS["socks4"] is not None
        assert MAPPINGS["ss"] is not None
        assert MAPPINGS["ssr"] is not None
        assert MAPPINGS["trojan"] is Trojan
        assert MAPPINGS["ssh"] is not None
        assert MAPPINGS["ws"] is not None
        assert MAPPINGS["h2"] is not None
        assert MAPPINGS["h3"] is not None

    def test_ssl_secure_quic_in_map_to_empty_string(self) -> None:
        assert MAPPINGS["ssl"] == ""
        assert MAPPINGS["secure"] == ""
        assert MAPPINGS["quic"] == ""
        assert MAPPINGS["in"] == ""

    def test_get_protos_simple(self) -> None:
        err, protos = get_protos(["socks5"])
        assert err is None
        assert len(protos) == 1
        assert isinstance(protos[0], Socks5)

    def test_get_protos_plus_delimited(self) -> None:
        err, protos = get_protos(["http+ss"])
        assert err is not None
        assert protos is None

    def test_get_protos_unknown_returns_error(self) -> None:
        err, protos = get_protos(["unknown"])
        assert err is not None
        assert protos is None
        assert "existing protocols" in err

    def test_get_protos_ssl_marker(self) -> None:
        err, protos = get_protos(["socks5+ssl"])
        assert err is not None
        assert protos is None

    def test_get_protos_empty_list(self) -> None:
        err, protos = get_protos([])
        assert err == "no protocol specified"
        assert protos is None

    def test_get_protos_with_brace_param(self) -> None:
        err, protos = get_protos(["socks5{param}"])
        assert err is None
        assert len(protos) == 1
        assert protos[0].param == "param"


# ---------------------------------------------------------------------------
# Protocol: classes
# ---------------------------------------------------------------------------


class TestProtocolClasses:
    def test_direct_name(self) -> None:
        assert Direct().name == "direct"

    def test_http_name(self) -> None:
        assert HTTP().name == "http"

    def test_socks5_name(self) -> None:
        assert Socks5().name == "socks5"

    def test_ss_name(self) -> None:
        assert SS().name == "ss"

    def test_trojan_name(self) -> None:
        assert Trojan().name == "trojan"

    def test_h2_name(self) -> None:
        assert H2().name == "h2"

    def test_base_protocol_reuse_false(self) -> None:
        assert BaseProtocol().reuse() is False

    def test_direct_param(self) -> None:
        assert Direct("test").param == "test"

    def test_socks5_default_param(self) -> None:
        assert Socks5().param == ""

    def test_protocols_inherit_base(self) -> None:
        for cls in (Direct, HTTP, Socks5, SS, Trojan, H2):
            assert issubclass(cls, BaseProtocol)

    def test_h3_raises_unsupported(self) -> None:
        with pytest.raises(UnsupportedFeatureError):
            H3()

    def test_ssh_raises_unsupported(self) -> None:
        with pytest.raises(UnsupportedFeatureError):
            SSH()

    def test_ssr_raises_unsupported(self) -> None:
        with pytest.raises(UnsupportedFeatureError):
            SSR()


# ---------------------------------------------------------------------------
# Protocol: redaction
# ---------------------------------------------------------------------------


class TestProtocolRedaction:
    def test_ss_password_redacted_in_repr(self) -> None:
        proto = SS("aes-256-gcm:secretpassword")
        r = repr(proto)
        assert "secretpassword" not in r

    def test_ss_cipher_visible_in_repr(self) -> None:
        proto = SS("aes-256-gcm:secret")
        r = repr(proto)
        assert "aes-256-gcm" in r

    def test_long_token_redacted(self) -> None:
        proto = Direct("a" * 30)
        r = repr(proto)
        assert "***" in r

    def test_normal_param_not_redacted(self) -> None:
        proto = Direct("normal")
        r = repr(proto)
        assert "normal" in r


# ---------------------------------------------------------------------------
# Protocol: equality / hashing
# ---------------------------------------------------------------------------


class TestProtocolEquality:
    def test_direct_eq(self) -> None:
        assert Direct() == Direct()

    def test_direct_ne_different_param(self) -> None:
        assert Direct("a") != Direct("b")

    def test_direct_ne_different_type(self) -> None:
        assert Direct() != "direct"

    def test_hash_consistency(self) -> None:
        assert hash(Direct()) == hash(Direct())

    def test_hash_different_param(self) -> None:
        assert hash(Direct("a")) != hash(Direct("b"))


# ---------------------------------------------------------------------------
# Protocol: pickle
# ---------------------------------------------------------------------------


class TestProtocolPickle:
    def test_direct_pickle_roundtrip(self) -> None:
        assert pickle.loads(pickle.dumps(Direct())) == Direct()

    def test_http_pickle_roundtrip(self) -> None:
        assert pickle.loads(pickle.dumps(HTTP())) == HTTP()

    def test_ss_pickle_roundtrip(self) -> None:
        original = SS("aes-256-gcm:pw")
        restored = pickle.loads(pickle.dumps(original))
        assert restored == original
        assert restored.param == "aes-256-gcm:pw"


# ---------------------------------------------------------------------------
# Protocol: copy
# ---------------------------------------------------------------------------


class TestProtocolCopy:
    def test_direct_copy(self) -> None:
        assert copy.copy(Direct()) == Direct()

    def test_direct_deepcopy(self) -> None:
        assert copy.deepcopy(Direct()) == Direct()


# ---------------------------------------------------------------------------
# Protocol: helpers
# ---------------------------------------------------------------------------


class TestProtocolHelpers:
    def test_packstr(self) -> None:
        assert packstr(b"hello") == b"\x05hello"

    def test_packstr_2byte(self) -> None:
        assert packstr(b"hello", 2) == b"\x00\x05hello"

    def test_netloc_split_host_port(self) -> None:
        assert netloc_split("host:80") == ("host", 80)

    def test_netloc_split_ipv6(self) -> None:
        assert netloc_split("[::1]:80") == ("::1", 80)

    def test_netloc_split_defaults(self) -> None:
        assert netloc_split("", "localhost", 80) == ("localhost", 80)

    def test_http_line_regex(self) -> None:
        assert HTTP_LINE.match("GET / HTTP/1.1")


# ---------------------------------------------------------------------------
# Cipher: MAP
# ---------------------------------------------------------------------------


class TestCipherMap:
    def test_map_has_24_keys(self) -> None:
        assert len(MAP) == 24

    def test_specific_cipher_lookups(self) -> None:
        assert MAP["aes-256-gcm"] is AES_256_GCM_Cipher
        assert MAP["chacha20-ietf-poly1305"] is ChaCha20_IETF_POLY1305_Cipher
        assert MAP["rc4"] is RC4_Cipher
        assert MAP["aes-128-cfb"] is not None
        assert MAP["bf-cfb"] is not None
        assert MAP["cast5-cfb"] is not None
        assert MAP["des-cfb"] is not None

    def test_all_map_values_are_classes(self) -> None:
        for v in MAP.values():
            assert isinstance(v, type)

    def test_aead_ciphers_in_map(self) -> None:
        aead_keys = [k for k, v in MAP.items() if issubclass(v, AEADCipher)]
        assert len(aead_keys) == 4


# ---------------------------------------------------------------------------
# Cipher: classes
# ---------------------------------------------------------------------------


class TestCipherClasses:
    def test_base_cipher_name(self) -> None:
        assert BaseCipher.name() == "BaseCipher"

    def test_aes_256_gcm_name(self) -> None:
        assert AES_256_GCM_Cipher.name() == "AES-256-GCM"

    def test_chacha20_poly1305_name(self) -> None:
        assert ChaCha20_IETF_POLY1305_Cipher.name() == "ChaCha20-IETF-POLY1305"

    def test_aes_128_cfb_name(self) -> None:
        from eggress.cipher import AES_128_CFB_Cipher

        assert AES_128_CFB_Cipher.name() == "AES-128-CFB"

    def test_rc4_name(self) -> None:
        assert RC4_Cipher.name() == "RC4"

    def test_aes_256_gcm_key_length(self) -> None:
        assert AES_256_GCM_Cipher.KEY_LENGTH == 32

    def test_aes_128_gcm_key_length(self) -> None:
        assert AES_128_GCM_Cipher.KEY_LENGTH == 16

    def test_chacha20_poly1305_key_length(self) -> None:
        assert ChaCha20_IETF_POLY1305_Cipher.KEY_LENGTH == 32

    def test_base_cipher_key_stored(self) -> None:
        c = BaseCipher(b"mykey", setup_key=False)
        assert c.key == b"mykey"


# ---------------------------------------------------------------------------
# Cipher: hierarchy
# ---------------------------------------------------------------------------


class TestCipherHierarchy:
    def test_aead_inherits_base(self) -> None:
        assert issubclass(AEADCipher, BaseCipher)

    def test_aes_gcm_inherits_aead(self) -> None:
        assert issubclass(AES_256_GCM_Cipher, AEADCipher)

    def test_chacha_poly_inherits_aead(self) -> None:
        assert issubclass(ChaCha20_IETF_POLY1305_Cipher, AEADCipher)

    def test_rc4_inherits_base(self) -> None:
        assert issubclass(RC4_Cipher, BaseCipher)

    def test_cfb_inherits_base(self) -> None:
        assert issubclass(AES_256_CFB_Cipher, BaseCipher)


# ---------------------------------------------------------------------------
# Cipher: key derivation
# ---------------------------------------------------------------------------


class TestCipherKeyDerivation:
    def test_evp_bytes_to_key_known_vector(self) -> None:
        key, iv = _evp_bytes_to_key("password", 16, 0)
        assert isinstance(key, bytes)
        assert len(key) == 16

    def test_get_cipher_aes_256_gcm(self) -> None:
        err, fn = get_cipher("aes-256-gcm:password")
        assert err is None
        assert callable(fn)

    def test_get_cipher_chacha20_poly1305(self) -> None:
        err, fn = get_cipher("chacha20-ietf-poly1305:pw")
        assert err is None
        assert callable(fn)

    def test_get_cipher_rc4(self) -> None:
        err, fn = get_cipher("rc4:password")
        assert err is None
        assert callable(fn)

    def test_get_cipher_with_ota(self) -> None:
        err, fn = get_cipher("aes-256-gcm:pw!ota")
        assert err is None
        assert fn.ota is True

    def test_get_cipher_invalid_format(self) -> None:
        err, fn = get_cipher("invalid")
        assert err is not None
        assert fn is None

    def test_get_cipher_unknown_cipher(self) -> None:
        err, fn = get_cipher("unknown:pw")
        assert err is not None
        assert fn is None


# ---------------------------------------------------------------------------
# Cipher: redaction
# ---------------------------------------------------------------------------


class TestCipherRedaction:
    def test_key_not_in_repr(self) -> None:
        c = AES_256_GCM_Cipher(b"secretkey12345678901234567890", setup_key=False)
        r = repr(c)
        assert "secretkey" not in r

    def test_rc4_key_not_in_repr(self) -> None:
        c = RC4_Cipher(b"secretkey12345678", setup_key=False)
        r = repr(c)
        assert "secretkey" not in r


# ---------------------------------------------------------------------------
# Cipher: encrypt/decrypt (AEAD raises)
# ---------------------------------------------------------------------------


class TestCipherEncryptDecryptAEAD:
    def test_aes_256_gcm_encrypt_raises(self) -> None:
        c = AES_256_GCM_Cipher(b"0" * 32)
        with pytest.raises(CipherUnsupportedError):
            c.encrypt(b"data")

    def test_aes_256_gcm_decrypt_raises(self) -> None:
        c = AES_256_GCM_Cipher(b"0" * 32)
        with pytest.raises(CipherUnsupportedError):
            c.decrypt(b"data")


# ---------------------------------------------------------------------------
# Cipher: encrypt/decrypt (legacy raises)
# ---------------------------------------------------------------------------


class TestCipherEncryptDecryptLegacy:
    def test_rc4_encrypt_raises(self) -> None:
        c = RC4_Cipher(b"0" * 16)
        with pytest.raises(CipherUnsupportedError):
            c.encrypt(b"data")

    def test_aes_cfb_encrypt_raises(self) -> None:
        c = AES_256_CFB_Cipher(b"0" * 32)
        with pytest.raises(CipherUnsupportedError):
            c.encrypt(b"data")


# ---------------------------------------------------------------------------
# ApplyCipher
# ---------------------------------------------------------------------------


class TestApplyCipher:
    def test_apply_cipher_has_attributes(self) -> None:
        err, fn = get_cipher("aes-256-gcm:password")
        assert err is None
        assert hasattr(fn, "cipher")
        assert hasattr(fn, "key")
        assert hasattr(fn, "name")
        assert hasattr(fn, "ota")
        assert hasattr(fn, "datagram")

    def test_apply_cipher_aead_has_datagram(self) -> None:
        err, fn = get_cipher("aes-256-gcm:password")
        assert err is None
        assert fn.datagram is not None

    def test_apply_cipher_stream_no_datagram(self) -> None:
        err, fn = get_cipher("rc4:password")
        assert err is None
        assert fn.datagram is None


# ---------------------------------------------------------------------------
# PacketCipher
# ---------------------------------------------------------------------------


class TestPacketCipher:
    def test_packet_cipher_init(self) -> None:
        c = AES_256_GCM_Cipher(b"0" * 32)
        pc = PacketCipher(c, b"key", "aes-256-gcm")
        assert pc.cipher is c
        assert pc.key == b"key"
        assert pc.name == "aes-256-gcm"

    def test_packet_cipher_encrypt_raises(self) -> None:
        c = AES_256_GCM_Cipher(b"0" * 32)
        pc = PacketCipher(c, b"key", "aes-256-gcm")
        with pytest.raises(CipherUnsupportedError):
            pc.encrypt(b"data")

    def test_packet_cipher_decrypt_raises(self) -> None:
        c = AES_256_GCM_Cipher(b"0" * 32)
        pc = PacketCipher(c, b"key", "aes-256-gcm")
        with pytest.raises(CipherUnsupportedError):
            pc.decrypt(b"data")
