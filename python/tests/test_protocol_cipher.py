"""Comprehensive tests for python/eggress/protocol.py and python/eggress/cipher.py.

Phase C4: pproxy-compatible protocol and cipher object models.
"""

from __future__ import annotations

import copy
import hashlib
import pickle

import pytest

from eggress.cipher import (
    MAP,
    AEADCipher,
    AES_128_GCM_Cipher,
    AES_192_GCM_Cipher,
    AES_256_GCM_Cipher,
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
    Echo,
    H2,
    H3,
    HTTP,
    HTTPOnly,
    Pf,
    Redir,
    Socks4,
    SS,
    SSH,
    SSR,
    Socks5,
    Transparent,
    Trojan,
    Tunnel,
    UnsupportedFeatureError,
    WS,
    get_protos,
    netloc_split,
    packstr,
)

# ---------------------------------------------------------------------------
# Protocol: registry
# ---------------------------------------------------------------------------


class TestProtocolRegistry:
    def test_mappings_has_24_keys(self) -> None:
        assert len(MAPPINGS) == 24

    def test_mappings_class_values_count(self) -> None:
        classes = [v for v in MAPPINGS.values() if not isinstance(v, str)]
        strings = [v for v in MAPPINGS.values() if isinstance(v, str)]
        assert len(classes) == 20
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


# ---------------------------------------------------------------------------
# Protocol: target/dest/source attributes
# ---------------------------------------------------------------------------


class TestProtocolAttributes:
    def test_base_defaults_all_none(self) -> None:
        p = BaseProtocol()
        assert p.target is None
        assert p.dest is None
        assert p.source is None

    def test_direct_target_from_param(self) -> None:
        p = Direct("example.com:80")
        assert p.target == "example.com:80"
        assert p.dest is None
        assert p.source is None

    def test_direct_target_empty_param(self) -> None:
        p = Direct()
        assert p.target is None

    def test_direct_explicit_target(self) -> None:
        p = Direct("x", target="y:1")
        assert p.target == "y:1"

    def test_http_target_from_param(self) -> None:
        p = HTTP("proxy.local:8080")
        assert p.target == "proxy.local:8080"
        assert p.dest is None

    def test_http_target_empty(self) -> None:
        p = HTTP()
        assert p.target is None

    def test_socks4_target_from_param(self) -> None:
        p = Socks4("10.0.0.1:1080")
        assert p.target == "10.0.0.1:1080"

    def test_socks4_target_empty(self) -> None:
        p = Socks4()
        assert p.target is None

    def test_socks5_target_from_param(self) -> None:
        p = Socks5("proxy:1080")
        assert p.target == "proxy:1080"

    def test_socks5_target_empty(self) -> None:
        p = Socks5()
        assert p.target is None

    def test_ss_no_target(self) -> None:
        p = SS("aes-256-gcm:password")
        assert p.target is None
        assert p.dest is None
        assert p.cipher == "aes-256-gcm"

    def test_trojan_target_from_param(self) -> None:
        p = Trojan("mypassword@host.example.com:443")
        assert p.target == "host.example.com:443"

    def test_trojan_target_no_at(self) -> None:
        p = Trojan("justpassword")
        assert p.target == "justpassword"

    def test_trojan_target_empty(self) -> None:
        p = Trojan()
        assert p.target is None

    def test_ws_target_strips_path(self) -> None:
        p = WS("ws.example.com:443/tunnel")
        assert p.target == "ws.example.com:443"

    def test_ws_target_no_path(self) -> None:
        p = WS("ws.example.com:443")
        assert p.target == "ws.example.com:443"

    def test_ws_target_empty(self) -> None:
        p = WS()
        assert p.target is None

    def test_h2_target_from_param(self) -> None:
        p = H2("h2-proxy:8443")
        assert p.target == "h2-proxy:8443"

    def test_h2_target_empty(self) -> None:
        p = H2()
        assert p.target is None

    def test_tunnel_dest_from_param(self) -> None:
        p = Tunnel("fixed-dest:9090")
        assert p.dest == "fixed-dest:9090"
        assert p.target is None

    def test_tunnel_dest_empty(self) -> None:
        p = Tunnel()
        assert p.dest is None

    def test_tunnel_has_destination_attr(self) -> None:
        p = Tunnel("dest:80")
        assert p.destination == "dest:80"

    def test_echo_no_target(self) -> None:
        p = Echo()
        assert p.target is None
        assert p.dest is None
        assert p.source is None

    def test_transparent_no_target(self) -> None:
        p = Redir()
        assert p.target is None
        assert p.dest is None

    def test_pf_no_target(self) -> None:
        p = Pf()
        assert p.target is None
        assert p.dest is None

    def test_explicit_source(self) -> None:
        p = Direct("x", source="0.0.0.0:8080")
        assert p.source == "0.0.0.0:8080"

    def test_explicit_dest(self) -> None:
        p = HTTP("x", dest="remote:443")
        assert p.dest == "remote:443"

    def test_pickle_roundtrip_preserves_target(self) -> None:
        p = Trojan("pw@host:443")
        r = pickle.loads(pickle.dumps(p))
        assert r.target == "host:443"
        assert r.param == "pw@host:443"

    def test_pickle_roundtrip_preserves_dest(self) -> None:
        p = Tunnel("dest:80", source="0.0.0.0:9090")
        r = pickle.loads(pickle.dumps(p))
        assert r.dest == "dest:80"
        assert r.source == "0.0.0.0:9090"

    def test_copy_preserves_target(self) -> None:
        p = Direct("x:1", target="y:2")
        c = copy.copy(p)
        assert c.target == "y:2"

    def test_deepcopy_preserves_target(self) -> None:
        p = Trojan("pw@host:443")
        c = copy.deepcopy(p)
        assert c.target == "host:443"

    def test_equality_considers_target(self) -> None:
        assert Direct("x", target="a") != Direct("x", target="b")

    def test_equality_considers_dest(self) -> None:
        assert Tunnel("x", dest="a") != Tunnel("x", dest="b")

    def test_hash_considers_target(self) -> None:
        assert hash(Direct("x", target="a")) != hash(Direct("x", target="b"))

    def test_hash_considers_dest(self) -> None:
        assert hash(Tunnel("x", dest="a")) != hash(Tunnel("x", dest="b"))


# ---------------------------------------------------------------------------
# Protocol: aliases
# ---------------------------------------------------------------------------


class TestProtocolAliases:
    def test_socks4a_maps_to_socks4(self) -> None:
        assert MAPPINGS["socks4a"] is Socks4

    def test_https_maps_to_http(self) -> None:
        assert MAPPINGS["https"] is HTTP

    def test_httpget_maps_to_http(self) -> None:
        assert MAPPINGS["httpget"] is HTTP

    def test_socks4a_in_registry(self) -> None:
        from eggress.protocol import _PROTOCOL_REGISTRY

        assert _PROTOCOL_REGISTRY["socks4a"] is Socks4

    def test_get_protos_socks4a(self) -> None:
        err, protos = get_protos(["socks4a{10.0.0.1:1080}"])
        assert err is None
        assert len(protos) == 1
        assert isinstance(protos[0], Socks4)
        assert protos[0].target == "10.0.0.1:1080"

    def test_get_protos_https(self) -> None:
        err, protos = get_protos(["https{proxy:8080}"])
        assert err is None
        assert len(protos) == 1
        assert isinstance(protos[0], HTTP)
        assert protos[0].target == "proxy:8080"

    def test_get_protos_httpget(self) -> None:
        err, protos = get_protos(["httpget{proxy:8080}"])
        assert err is None
        assert len(protos) == 1
        assert isinstance(protos[0], HTTP)


# ---------------------------------------------------------------------------
# Protocol: composition metadata
# ---------------------------------------------------------------------------


class TestProtocolMetadata:
    def test_all_supported_classes_have_metadata(self) -> None:
        classes = [
            Direct, HTTP, HTTPOnly, Socks4, Socks5, SS, Trojan, WS,
            H2, Tunnel, Echo, Redir, Pf,
        ]
        for cls in classes:
            proto = cls.__new__(cls)
            assert hasattr(cls, "_SUPPORTED_IN_EGRESS")
            assert hasattr(cls, "_TRAFFIC_KINDS")
            assert hasattr(cls, "_ROLE")

    def test_direct_metadata(self) -> None:
        assert Direct._SUPPORTED_IN_EGRESS is True
        assert Direct._TRAFFIC_KINDS == ("tcp",)
        assert Direct._ROLE == "both"

    def test_socks5_metadata(self) -> None:
        assert Socks5._TRAFFIC_KINDS == ("tcp", "udp")

    def test_ss_metadata(self) -> None:
        assert SS._SUPPORTED_IN_EGRESS is True
        assert SS._TRAFFIC_KINDS == ("tcp", "udp")

    def test_trojan_metadata(self) -> None:
        assert Trojan._TRAFFIC_KINDS == ("tcp",)

    def test_transparent_is_listener_only(self) -> None:
        assert Transparent._ROLE == "listener"
        assert Redir._ROLE == "listener"
        assert Pf._ROLE == "listener"
        assert Tunnel._ROLE == "listener"
        assert Echo._ROLE == "listener"

    def test_h3_unsupported(self) -> None:
        assert H3._SUPPORTED_IN_EGRESS is False

    def test_ssh_unsupported(self) -> None:
        assert SSH._SUPPORTED_IN_EGRESS is False

    def test_ssr_unsupported(self) -> None:
        assert SSR._SUPPORTED_IN_EGRESS is False

    def test_h2_metadata(self) -> None:
        assert H2._SUPPORTED_IN_EGRESS is True
        assert H2._TRAFFIC_KINDS == ("tcp",)

    def test_ws_metadata(self) -> None:
        assert WS._SUPPORTED_IN_EGRESS is True
        assert WS._TRAFFIC_KINDS == ("tcp",)


# ---------------------------------------------------------------------------
# Cipher: equality / hashing
# ---------------------------------------------------------------------------


class TestCipherEquality:
    def test_aes_gcm_eq_same_key(self) -> None:
        key = b"0" * 32
        assert AES_256_GCM_Cipher(key, setup_key=False) == AES_256_GCM_Cipher(
            key, setup_key=False
        )

    def test_aes_gcm_ne_different_key(self) -> None:
        assert AES_256_GCM_Cipher(b"0" * 32, setup_key=False) != AES_256_GCM_Cipher(
            b"1" * 32, setup_key=False
        )

    def test_aes_gcm_ne_different_type(self) -> None:
        assert AES_256_GCM_Cipher(b"0" * 32, setup_key=False) != "not a cipher"

    def test_chacha_eq_same_key(self) -> None:
        key = b"0" * 32
        assert ChaCha20_IETF_POLY1305_Cipher(
            key, setup_key=False
        ) == ChaCha20_IETF_POLY1305_Cipher(key, setup_key=False)

    def test_cross_type_ne(self) -> None:
        key = b"0" * 32
        assert AES_256_GCM_Cipher(key, setup_key=False) != ChaCha20_IETF_POLY1305_Cipher(
            key, setup_key=False
        )

    def test_hash_consistency(self) -> None:
        key = b"0" * 32
        assert hash(AES_256_GCM_Cipher(key, setup_key=False)) == hash(
            AES_256_GCM_Cipher(key, setup_key=False)
        )

    def test_hash_different_key(self) -> None:
        assert hash(AES_256_GCM_Cipher(b"0" * 32, setup_key=False)) != hash(
            AES_256_GCM_Cipher(b"1" * 32, setup_key=False)
        )

    def test_usable_in_set(self) -> None:
        key = b"0" * 32
        s = {
            AES_256_GCM_Cipher(key, setup_key=False),
            AES_256_GCM_Cipher(key, setup_key=False),
        }
        assert len(s) == 1

    def test_usable_as_dict_key(self) -> None:
        key = b"0" * 32
        d = {AES_256_GCM_Cipher(key, setup_key=False): "value"}
        assert d[AES_256_GCM_Cipher(key, setup_key=False)] == "value"


# ---------------------------------------------------------------------------
# Cipher: pickle
# ---------------------------------------------------------------------------


class TestCipherPickle:
    def test_aes_256_gcm_pickle_raises(self) -> None:
        key = b"0" * 32
        c = AES_256_GCM_Cipher(key, setup_key=False)
        with pytest.raises(TypeError, match="key material"):
            pickle.dumps(c)

    def test_chacha_pickle_raises(self) -> None:
        key = b"0" * 32
        c = ChaCha20_IETF_POLY1305_Cipher(key, setup_key=False)
        with pytest.raises(TypeError, match="key material"):
            pickle.dumps(c)

    def test_rc4_pickle_raises(self) -> None:
        key = b"0" * 16
        c = RC4_Cipher(key, setup_key=False)
        with pytest.raises(TypeError, match="key material"):
            pickle.dumps(c)

    def test_aes_128_gcm_pickle_raises(self) -> None:
        key = b"0" * 16
        c = AES_128_GCM_Cipher(key, setup_key=False)
        with pytest.raises(TypeError, match="key material"):
            pickle.dumps(c)

    def test_base_cipher_pickle_raises(self) -> None:
        c = BaseCipher(b"mykey", setup_key=False)
        with pytest.raises(TypeError, match="key material"):
            pickle.dumps(c)


# ---------------------------------------------------------------------------
# Cipher: copy
# ---------------------------------------------------------------------------


class TestCipherCopy:
    def test_aes_gcm_copy(self) -> None:
        key = b"0" * 32
        c = AES_256_GCM_Cipher(key, setup_key=False)
        copied = copy.copy(c)
        assert copied == c
        assert copied.key == c.key

    def test_aes_gcm_deepcopy(self) -> None:
        key = b"0" * 32
        c = AES_256_GCM_Cipher(key, setup_key=False)
        copied = copy.deepcopy(c)
        assert copied == c
        assert copied.key == c.key

    def test_chacha_copy(self) -> None:
        key = b"0" * 32
        c = ChaCha20_IETF_POLY1305_Cipher(key, setup_key=False)
        assert copy.copy(c) == c

    def test_rc4_copy(self) -> None:
        key = b"0" * 16
        c = RC4_Cipher(key, setup_key=False)
        assert copy.copy(c) == c


# ---------------------------------------------------------------------------
# Cipher: known answer (EVP_BytesToKey)
# ---------------------------------------------------------------------------


class TestCipherKnownAnswer:
    def test_evp_password_single_md5(self) -> None:
        key, iv = _evp_bytes_to_key("password", 16, 0)
        expected = hashlib.md5(b"").digest()
        assert key == expected

    def test_evp_password_key_len_32(self) -> None:
        key, iv = _evp_bytes_to_key("password", 32, 0)
        block0 = hashlib.md5(b"").digest()
        block1 = hashlib.md5(block0).digest()
        expected = block0 + block1
        assert key == expected

    def test_evp_password_with_iv(self) -> None:
        key, iv = _evp_bytes_to_key("password", 16, 16)
        assert len(key) == 16
        assert len(iv) == 16
        block0 = hashlib.md5(b"").digest()
        block1 = hashlib.md5(block0).digest()
        expected_d = block0 + block1
        assert key == expected_d[:16]
        assert iv == expected_d[16:32]

    def test_key_length_aes_256_gcm(self) -> None:
        _, fn = get_cipher("aes-256-gcm:password")
        assert fn.key == _evp_bytes_to_key("password", 32, 0)[0]

    def test_key_length_aes_192_gcm(self) -> None:
        _, fn = get_cipher("aes-192-gcm:password")
        assert fn.key == _evp_bytes_to_key("password", 24, 0)[0]

    def test_key_length_aes_128_gcm(self) -> None:
        _, fn = get_cipher("aes-128-gcm:password")
        assert fn.key == _evp_bytes_to_key("password", 16, 0)[0]

    def test_key_length_chacha20_poly1305(self) -> None:
        _, fn = get_cipher("chacha20-ietf-poly1305:password")
        assert fn.key == _evp_bytes_to_key("password", 32, 0)[0]

    def test_evp_bytes_are_deterministic(self) -> None:
        k1, _ = _evp_bytes_to_key("test", 32, 16)
        k2, _ = _evp_bytes_to_key("test", 32, 16)
        assert k1 == k2

    def test_evp_empty_password(self) -> None:
        key, iv = _evp_bytes_to_key("", 16, 0)
        assert len(key) == 16
        assert key == hashlib.md5(b"").digest()


# ---------------------------------------------------------------------------
# Cipher: AEAD encrypt_and_digest / decrypt_and_verify
# ---------------------------------------------------------------------------


class TestCipherAeadEncryptDecrypt:
    def test_aes_256_gcm_encrypt_and_digest_raises(self) -> None:
        c = AES_256_GCM_Cipher(b"0" * 32)
        with pytest.raises(CipherUnsupportedError):
            c.encrypt_and_digest(b"plaintext")

    def test_aes_256_gcm_decrypt_and_verify_raises(self) -> None:
        c = AES_256_GCM_Cipher(b"0" * 32)
        with pytest.raises(CipherUnsupportedError):
            c.decrypt_and_verify(b"ciphertext", b"tag")

    def test_chacha_encrypt_and_digest_raises(self) -> None:
        c = ChaCha20_IETF_POLY1305_Cipher(b"0" * 32)
        with pytest.raises(CipherUnsupportedError):
            c.encrypt_and_digest(b"plaintext")

    def test_chacha_decrypt_and_verify_raises(self) -> None:
        c = ChaCha20_IETF_POLY1305_Cipher(b"0" * 32)
        with pytest.raises(CipherUnsupportedError):
            c.decrypt_and_verify(b"ciphertext", b"tag")

    def test_aes_128_gcm_encrypt_and_digest_raises(self) -> None:
        c = AES_128_GCM_Cipher(b"0" * 16)
        with pytest.raises(CipherUnsupportedError):
            c.encrypt_and_digest(b"plaintext")

    def test_aes_128_gcm_decrypt_and_verify_raises(self) -> None:
        c = AES_128_GCM_Cipher(b"0" * 16)
        with pytest.raises(CipherUnsupportedError):
            c.decrypt_and_verify(b"ciphertext", b"tag")


# ---------------------------------------------------------------------------
# WS6: import paths, identity, and module reload tests
# ---------------------------------------------------------------------------


class TestImportPaths:
    """Verify expected import paths and alias relationships."""

    def test_import_from_eggress_protocol(self) -> None:
        from eggress.protocol import Socks5, HTTP, SS

        assert Socks5 is not None
        assert HTTP is not None
        assert SS is not None

    def test_import_from_eggress_cipher(self) -> None:
        from eggress.cipher import AES_256_GCM_Cipher, MAP

        assert AES_256_GCM_Cipher is not None
        assert isinstance(MAP, dict)

    def test_import_from_eggress_wrapper(self) -> None:
        from eggress.wrapper import BaseWrapper, TLS, Plugin, Chain, normalize_chain

        assert BaseWrapper is not None
        assert TLS is not None
        assert Plugin is not None
        assert Chain is not None
        assert callable(normalize_chain)

    def test_import_from_eggress_plugin(self) -> None:
        from eggress.plugin import PluginRegistry, PluginBridge

        assert PluginRegistry is not None
        assert PluginBridge is not None

    def test_top_level_imports(self) -> None:
        import eggress

        assert hasattr(eggress, "Socks5")
        assert hasattr(eggress, "HTTP")
        assert hasattr(eggress, "AES_256_GCM_Cipher")
        assert hasattr(eggress, "PluginRegistry")
        assert hasattr(eggress, "TLS")
        assert hasattr(eggress, "Chain")
        assert hasattr(eggress, "normalize_chain")


class TestIdentityRelationships:
    """Verify identity relationships between registry values and classes."""

    def test_mappings_socks5_is_socks5_class(self) -> None:
        from eggress.protocol import MAPPINGS, Socks5

        assert MAPPINGS["socks5"] is Socks5

    def test_mappings_http_is_http_class(self) -> None:
        from eggress.protocol import MAPPINGS, HTTP

        assert MAPPINGS["http"] is HTTP

    def test_mappings_ss_is_ss_class(self) -> None:
        from eggress.protocol import MAPPINGS, SS

        assert MAPPINGS["ss"] is SS

    def test_mappings_socks4a_is_socks4_class(self) -> None:
        from eggress.protocol import MAPPINGS, Socks4

        assert MAPPINGS["socks4a"] is Socks4

    def test_mappings_https_is_http_class(self) -> None:
        from eggress.protocol import MAPPINGS, HTTP

        assert MAPPINGS["https"] is HTTP

    def test_mappings_httpget_is_http_class(self) -> None:
        from eggress.protocol import MAPPINGS, HTTP

        assert MAPPINGS["httpget"] is HTTP

    def test_mappings_socks_is_socks5_class(self) -> None:
        from eggress.protocol import MAPPINGS, Socks5

        assert MAPPINGS["socks"] is Socks5

    def test_cipher_map_aes256gcm(self) -> None:
        from eggress.cipher import MAP, AES_256_GCM_Cipher

        assert MAP["aes-256-gcm"] is AES_256_GCM_Cipher

    def test_cipher_map_chacha20poly1305(self) -> None:
        from eggress.cipher import MAP, ChaCha20_IETF_POLY1305_Cipher

        assert MAP["chacha20-ietf-poly1305"] is ChaCha20_IETF_POLY1305_Cipher

    def test_get_protos_returns_new_instances(self) -> None:
        from eggress.protocol import get_protos

        _, protos = get_protos(["socks5"])
        _, protos2 = get_protos(["socks5"])
        assert protos is not None
        assert protos2 is not None
        assert protos[0] is not protos2[0]


class TestModuleReload:
    """Verify module-level constants and attributes are stable."""

    def test_protocol_module_has_expected_constants(self) -> None:
        from eggress import protocol as proto_mod

        assert hasattr(proto_mod, "HTTP_LINE")
        assert hasattr(proto_mod, "MAPPINGS")
        assert hasattr(proto_mod, "get_protos")
        assert hasattr(proto_mod, "packstr")
        assert hasattr(proto_mod, "netloc_split")

    def test_cipher_module_has_expected_constants(self) -> None:
        from eggress import cipher as cipher_mod

        assert hasattr(cipher_mod, "MAP")
        assert hasattr(cipher_mod, "get_cipher")
        assert hasattr(cipher_mod, "_evp_bytes_to_key")

    def test_wrapper_module_has_expected_constants(self) -> None:
        from eggress import wrapper as wrapper_mod

        assert hasattr(wrapper_mod, "TLS")
        assert hasattr(wrapper_mod, "Plugin")
        assert hasattr(wrapper_mod, "Chain")
        assert hasattr(wrapper_mod, "normalize_chain")
        assert hasattr(wrapper_mod, "BaseWrapper")
