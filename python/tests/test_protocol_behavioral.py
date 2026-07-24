"""Behavioral tests for protocol internal methods in ``eggress.protocol``.

Covers ``guess``, ``accept``, ``connect``, ``udp_pack``, ``udp_unpack``,
``query_remote``, ``get_protos``, and ``MAPPINGS`` for all concrete protocol
classes.
"""

from __future__ import annotations

import asyncio
import struct
from typing import Any

import pytest

from eggress.protocol import (
    SS,
    BaseProtocol,
    Direct,
    Echo,
    HTTP,
    HTTP_LINE,
    HTTPOnly,
    MAPPINGS,
    Socks4,
    Socks5,
    _PROTOCOL_REGISTRY,
    decode_socks_address,
    get_protos,
    netloc_split,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


class FakeReader:
    """Minimal async reader backed by a bytes buffer."""

    def __init__(self, data: bytes = b"") -> None:
        self._buf = data

    async def read(self, n: int = -1) -> bytes:
        if n == -1:
            out = self._buf
            self._buf = b""
            return out
        out = self._buf[:n]
        self._buf = self._buf[n:]
        return out


async def _run(coro: Any) -> Any:
    return await coro


# ---------------------------------------------------------------------------
# Direct
# ---------------------------------------------------------------------------


class TestDirect:
    def test_name(self) -> None:
        assert Direct().name == "direct"

    def test_param_defaults(self) -> None:
        d = Direct()
        assert d.param == ""
        assert d.target is None

    def test_param_sets_target(self) -> None:
        d = Direct("example.com:443")
        assert d.param == "example.com:443"
        assert d.target == "example.com:443"

    def test_udp_pack_passthrough(self) -> None:
        d = Direct()
        data = b"hello world"
        assert d.udp_pack("example.com", 443, data) is data

    def test_udp_unpack_passthrough(self) -> None:
        d = Direct()
        data = b"hello world"
        assert d.udp_unpack(data) is data

    def test_guess_not_implemented(self) -> None:
        d = Direct()
        with pytest.raises(NotImplementedError, match="direct does not implement guess"):
            asyncio.get_event_loop().run_until_complete(d.guess(FakeReader()))

    def test_accept_not_implemented(self) -> None:
        d = Direct()
        with pytest.raises(NotImplementedError, match="direct does not implement accept"):
            asyncio.get_event_loop().run_until_complete(d.accept(FakeReader(), None))


# ---------------------------------------------------------------------------
# HTTP
# ---------------------------------------------------------------------------


class TestHTTP:
    def test_name(self) -> None:
        assert HTTP().name == "http"

    def test_guess_detects_get(self) -> None:
        proto = HTTP()
        reader = FakeReader(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is not None
        assert result.startswith(b"GET")

    def test_guess_detects_connect(self) -> None:
        proto = HTTP()
        reader = FakeReader(b"CONNECT example.com:443 HTTP/1.1\r\n\r\n")
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is not None
        assert result.startswith(b"CONNECT")

    def test_guess_detects_post(self) -> None:
        proto = HTTP()
        reader = FakeReader(b"POST /submit HTTP/1.1\r\nHost: x.com\r\n\r\n")
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is not None

    def test_guess_rejects_non_http(self) -> None:
        proto = HTTP()
        reader = FakeReader(b"\x05\x01\x00")
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is None

    def test_guess_empty_data(self) -> None:
        proto = HTTP()
        reader = FakeReader(b"")
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is None

    def test_accept_connect_parses_host_port(self) -> None:
        proto = HTTP()
        proto._buffered = b"CONNECT example.com:8443 HTTP/1.1\r\n\r\n"
        user, host, port = asyncio.get_event_loop().run_until_complete(
            proto.accept(FakeReader(), "testuser", writer=None)
        )
        assert host == "example.com"
        assert port == 8443
        assert user == "testuser"

    def test_accept_connect_default_port(self) -> None:
        proto = HTTP()
        proto._buffered = b"CONNECT example.com HTTP/1.1\r\n\r\n"
        user, host, port = asyncio.get_event_loop().run_until_complete(
            proto.accept(FakeReader(), None, writer=None)
        )
        assert host == "example.com"
        assert port == 443

    def test_accept_get_with_host_header(self) -> None:
        proto = HTTP()
        proto._buffered = (
            b"GET /index.html HTTP/1.1\r\n"
            b"Host: myserver.com:9090\r\n"
            b"\r\n"
        )
        user, host, port = asyncio.get_event_loop().run_until_complete(
            proto.accept(FakeReader(), "u", writer=None)
        )
        assert host == "myserver.com"
        assert port == 9090

    def test_accept_get_extracts_host_from_url(self) -> None:
        proto = HTTP()
        proto._buffered = (
            b"GET http://from-url.com/page HTTP/1.1\r\n"
            b"\r\n"
        )
        user, host, port = asyncio.get_event_loop().run_until_complete(
            proto.accept(FakeReader(), None, writer=None)
        )
        assert host == "from-url.com"
        assert port == 80

    def test_connect_requires_writer(self) -> None:
        proto = HTTP()
        with pytest.raises(AttributeError):
            asyncio.get_event_loop().run_until_complete(
                proto.connect(None, None, None, "example.com", 443)
            )


# ---------------------------------------------------------------------------
# HTTPOnly
# ---------------------------------------------------------------------------


class TestHTTPOnly:
    def test_name(self) -> None:
        assert HTTPOnly().name == "httponly"

    def test_inherits_http_guess(self) -> None:
        proto = HTTPOnly()
        reader = FakeReader(b"GET / HTTP/1.1\r\n\r\n")
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is not None

    def test_guess_rejects_non_http(self) -> None:
        proto = HTTPOnly()
        reader = FakeReader(b"\x05\x01")
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is None


# ---------------------------------------------------------------------------
# Socks4
# ---------------------------------------------------------------------------


class TestSocks4:
    def test_name(self) -> None:
        assert Socks4().name == "socks4"

    def test_guess_detects_version_byte(self) -> None:
        proto = Socks4()
        # SOCKS4 connect request: version=0x04, cmd=0x01, port, ip
        data = b"\x04\x01\x00\x50\x7f\x00\x00\x01"
        reader = FakeReader(data)
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is not None
        assert result[0] == 0x04

    def test_guess_rejects_non_socks4(self) -> None:
        proto = Socks4()
        reader = FakeReader(b"\x05\x01\x00")
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is None

    def test_guess_empty(self) -> None:
        proto = Socks4()
        reader = FakeReader(b"")
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is None

    def test_accept_standard_socks4(self) -> None:
        proto = Socks4()
        # Build a SOCKS4 CONNECT request for 192.168.1.1:80
        port = struct.pack("!H", 80)
        ip = bytes([192, 168, 1, 1])
        data = b"\x04\x01" + port + ip + b"testuser\x00"
        proto._buffered = data
        user, host, port_val = asyncio.get_event_loop().run_until_complete(
            proto.accept(FakeReader(), "orig_user", writer=None, users=None, authtable=None)
        )
        assert host == "192.168.1.1"
        assert port_val == 80
        assert user == "orig_user"

    def test_accept_socks4a(self) -> None:
        proto = Socks4()
        # SOCKS4a: IP = 0.0.0.x where x != 0, then hostname after userid
        port = struct.pack("!H", 443)
        ip = bytes([0, 0, 0, 1])
        data = b"\x04\x01" + port + ip + b"testuser\x00example.com\x00"
        proto._buffered = data
        user, host, port_val = asyncio.get_event_loop().run_until_complete(
            proto.accept(FakeReader(), None, writer=None, users=None, authtable=None)
        )
        assert host == "example.com"
        assert port_val == 443


# ---------------------------------------------------------------------------
# Socks5
# ---------------------------------------------------------------------------


class TestSocks5:
    def test_name(self) -> None:
        assert Socks5().name == "socks5"

    def test_guess_detects_version_byte(self) -> None:
        proto = Socks5()
        # SOCKS5 greeting: version=0x05, nmethods=1, method=0x00
        data = b"\x05\x01\x00"
        reader = FakeReader(data)
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is not None
        assert result[0] == 0x05

    def test_guess_rejects_non_socks5(self) -> None:
        proto = Socks5()
        reader = FakeReader(b"\x04\x01\x00\x50")
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is None

    def test_guess_empty(self) -> None:
        proto = Socks5()
        reader = FakeReader(b"")
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is None

    def test_accept_socks5_no_auth_ipv4(self) -> None:
        proto = Socks5()
        # Build full SOCKS5 greeting + CONNECT request
        # Greeting: version=5, nmethods=1, method=0x00 (no auth)
        greeting = b"\x05\x01\x00"
        # Connect request: version=5, cmd=1(connect), rsv=0, atyp=1(IPv4), addr, port
        ip = bytes([10, 0, 0, 1])
        port = struct.pack("!H", 8080)
        connect = b"\x05\x01\x00\x01" + ip + port
        proto._buffered = greeting + connect
        user, host, port_val = asyncio.get_event_loop().run_until_complete(
            proto.accept(FakeReader(), "authed_user", writer=None, users=None, authtable=None)
        )
        assert host == "10.0.0.1"
        assert port_val == 8080
        assert user == "authed_user"

    def test_accept_socks5_domain(self) -> None:
        proto = Socks5()
        greeting = b"\x05\x01\x00"
        domain = b"example.com"
        connect = (
            b"\x05\x01\x00\x03"
            + bytes([len(domain)])
            + domain
            + struct.pack("!H", 9090)
        )
        proto._buffered = greeting + connect
        user, host, port_val = asyncio.get_event_loop().run_until_complete(
            proto.accept(FakeReader(), None, writer=None, users=None, authtable=None)
        )
        assert host == "example.com"
        assert port_val == 9090

    def test_accept_socks5_with_auth(self) -> None:
        proto = Socks5()
        # Greeting: nmethods=2, methods=[0x00, 0x02]
        greeting = b"\x05\x02\x00\x02"
        # Auth response: version=1, ulen=4, username, plen=4, password
        auth = b"\x01\x04user\x04pass"
        # Connect request: IPv4 10.0.0.1:80
        connect = b"\x05\x01\x00\x01" + bytes([10, 0, 0, 1]) + struct.pack("!H", 80)
        proto._buffered = greeting + auth + connect
        user, host, port_val = asyncio.get_event_loop().run_until_complete(
            proto.accept(FakeReader(), None, writer=None, users=None, authtable=None)
        )
        assert host == "10.0.0.1"
        assert port_val == 80
        assert user == (b"user", b"pass")

    def test_udp_pack_ipv4(self) -> None:
        proto = Socks5()
        payload = b"test data"
        result = proto.udp_pack("10.0.0.1", 80, payload)
        # Header: 3 bytes reserved + SOCKS address + payload
        assert result[:3] == b"\x00\x00\x00"
        assert result[3] == 0x01  # IPv4 atyp
        assert result[4:8] == bytes([10, 0, 0, 1])
        assert struct.unpack("!H", result[8:10])[0] == 80
        assert result[10:] == payload

    def test_udp_pack_domain(self) -> None:
        proto = Socks5()
        payload = b"\xaa\xbb"
        result = proto.udp_pack("example.com", 443, payload)
        assert result[:3] == b"\x00\x00\x00"
        assert result[3] == 0x03  # Domain atyp
        domain_len = result[4]
        assert domain_len == len(b"example.com")
        assert result[5 : 5 + domain_len] == b"example.com"
        port_val = struct.unpack("!H", result[5 + domain_len : 7 + domain_len])[0]
        assert port_val == 443
        assert result[7 + domain_len :] == payload

    def test_udp_unpack_ipv4(self) -> None:
        proto = Socks5()
        # Build: 3-byte header + IPv4 address (atyp=0x01, 4 bytes, 2 bytes port) + payload
        ip = bytes([192, 168, 1, 1])
        port = struct.pack("!H", 8080)
        data = b"\x00\x00\x00\x01" + ip + port + b"payload"
        host, port_val, payload = proto.udp_unpack(data)
        assert host == "192.168.1.1"
        assert port_val == 8080
        assert payload == b"payload"

    def test_udp_unpack_domain(self) -> None:
        proto = Socks5()
        domain = b"test.org"
        port = struct.pack("!H", 1234)
        data = b"\x00\x00\x00\x03" + bytes([len(domain)]) + domain + port + b"\xde\xad"
        host, port_val, payload = proto.udp_unpack(data)
        assert host == "test.org"
        assert port_val == 1234
        assert payload == b"\xde\xad"

    def test_udp_unpack_too_short(self) -> None:
        proto = Socks5()
        with pytest.raises(ValueError, match="SOCKS5 UDP header too short"):
            proto.udp_unpack(b"\x00\x00")


# ---------------------------------------------------------------------------
# Echo
# ---------------------------------------------------------------------------


class TestEcho:
    def test_name(self) -> None:
        assert Echo().name == "echo"

    def test_query_remote(self) -> None:
        e = Echo()
        result = e.query_remote(None)
        assert result == ("echo", 0)

    def test_role(self) -> None:
        assert Echo._ROLE == "listener"


# ---------------------------------------------------------------------------
# Protocol detection (guess)
# ---------------------------------------------------------------------------


class TestProtocolDetection:
    @pytest.mark.parametrize(
        "data,expected_class",
        [
            (b"GET / HTTP/1.1\r\n\r\n", HTTP),
            (b"POST /data HTTP/1.1\r\n\r\n", HTTP),
            (b"CONNECT host:443 HTTP/1.1\r\n\r\n", HTTP),
            (b"PUT /res HTTP/1.1\r\n\r\n", HTTP),
            (b"DELETE /res HTTP/1.1\r\n\r\n", HTTP),
            (b"HEAD / HTTP/1.1\r\n\r\n", HTTP),
            (b"OPTIONS * HTTP/1.1\r\n\r\n", HTTP),
            (b"PATCH /res HTTP/1.1\r\n\r\n", HTTP),
        ],
        ids=[
            "GET", "POST", "CONNECT", "PUT", "DELETE",
            "HEAD", "OPTIONS", "PATCH",
        ],
    )
    def test_http_methods_detected(self, data: bytes, expected_class: type) -> None:
        proto = HTTP()
        reader = FakeReader(data)
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is not None

    @pytest.mark.parametrize(
        "data",
        [
            b"\x05\x01\x00",
            b"\x05\x02\x00\x02",
        ],
        ids=["greeting-noauth", "greeting-auth"],
    )
    def test_socks5_detected(self, data: bytes) -> None:
        proto = Socks5()
        reader = FakeReader(data)
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is not None
        assert result[0] == 0x05

    @pytest.mark.parametrize(
        "data",
        [
            b"\x04\x01\x00\x50\x7f\x00\x00\x01",
            b"\x04\x01\x01\xbb\x00\x00\x00\x01",
        ],
        ids=["socks4-port80", "socks4-port443"],
    )
    def test_socks4_detected(self, data: bytes) -> None:
        proto = Socks4()
        reader = FakeReader(data)
        result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
        assert result is not None
        assert result[0] == 0x04

    def test_unknown_data_returns_none(self) -> None:
        for ProtoClass in (HTTP, Socks4, Socks5):
            proto = ProtoClass()
            reader = FakeReader(b"\xff\xfe\xfd\xfc\xfb")
            result = asyncio.get_event_loop().run_until_complete(proto.guess(reader))
            assert result is None


# ---------------------------------------------------------------------------
# get_protos()
# ---------------------------------------------------------------------------


class TestGetProtos:
    def test_returns_instances(self) -> None:
        err, protos = get_protos(["http", "socks5"])
        assert err is None
        assert protos is not None
        assert len(protos) == 2
        assert protos[0].name == "http"
        assert protos[1].name == "socks5"

    def test_single_proto(self) -> None:
        err, protos = get_protos(["socks4"])
        assert err is None
        assert protos is not None
        assert len(protos) == 1
        assert protos[0].name == "socks4"

    def test_with_param(self) -> None:
        err, protos = get_protos(["socks5{myauth}"])
        assert err is None
        assert protos is not None
        assert protos[0].param == "myauth"

    def test_unknown_proto_returns_error(self) -> None:
        err, protos = get_protos(["nonexistent"])
        assert err is not None
        assert "nonexistent" not in err
        assert protos is None

    def test_empty_list_returns_error(self) -> None:
        err, protos = get_protos([])
        assert err is not None
        assert protos is None

    def test_direct(self) -> None:
        err, protos = get_protos(["direct"])
        assert err is None
        assert protos is not None
        assert protos[0].name == "direct"

    def test_different_params_not_deduped(self) -> None:
        err, protos = get_protos(["http", "http{auth}"])
        assert err is None
        assert protos is not None
        assert len(protos) == 2


# ---------------------------------------------------------------------------
# MAPPINGS registry
# ---------------------------------------------------------------------------


class TestMappings:
    def test_contains_core_protocols(self) -> None:
        for name in ("http", "socks5", "socks4", "direct", "ss", "trojan"):
            assert name in MAPPINGS

    def test_aliases(self) -> None:
        assert MAPPINGS["socks"] is Socks5
        assert MAPPINGS["socks4a"] is Socks4
        assert MAPPINGS["https"] is HTTP
        assert MAPPINGS["httpget"] is HTTP

    def test_tls_marker_is_empty_string(self) -> None:
        assert MAPPINGS["ssl"] == ""
        assert MAPPINGS["secure"] == ""

    def test_protocol_registry_matches_mappings(self) -> None:
        for name, cls in _PROTOCOL_REGISTRY.items():
            assert name in MAPPINGS
            assert MAPPINGS[name] is cls


# ---------------------------------------------------------------------------
# decode_socks_address
# ---------------------------------------------------------------------------


class TestDecodeSocksAddress:
    def test_ipv4(self) -> None:
        data = b"\x01" + bytes([10, 0, 0, 1]) + struct.pack("!H", 80) + b"\x01\x02"
        host, port, remaining = decode_socks_address(data)
        assert host == "10.0.0.1"
        assert port == 80
        assert remaining == b"\x01\x02"

    def test_domain(self) -> None:
        domain = b"example.com"
        data = b"\x03" + bytes([len(domain)]) + domain + struct.pack("!H", 443)
        host, port, remaining = decode_socks_address(data)
        assert host == "example.com"
        assert port == 443
        assert remaining == b""

    def test_ipv6(self) -> None:
        ipv6 = bytes(range(16))
        data = b"\x04" + ipv6 + struct.pack("!H", 8080)
        host, port, remaining = decode_socks_address(data)
        assert ":" in host
        assert port == 8080
        assert remaining == b""

    def test_empty_raises(self) -> None:
        with pytest.raises(ValueError, match="empty data"):
            decode_socks_address(b"")

    def test_unsupported_atyp(self) -> None:
        with pytest.raises(ValueError, match="unsupported address type"):
            decode_socks_address(b"\xff")


# ---------------------------------------------------------------------------
# BaseProtocol edge cases
# ---------------------------------------------------------------------------


class TestBaseProtocol:
    def test_equality(self) -> None:
        assert HTTP("auth") == HTTP("auth")
        assert HTTP("auth") != HTTP("other")
        assert HTTP("a") != Socks5("a")

    def test_hash(self) -> None:
        s = {HTTP("a"), HTTP("a"), Socks5("a")}
        assert len(s) == 2

    def test_repr_redacts_long_tokens(self) -> None:
        p = SS("aes-256-gcm:verylongsecretkeythatmatchespattern")
        r = repr(p)
        assert "verylongsecretkeythatmatchespattern" not in r

    def test_repr_shows_param(self) -> None:
        p = HTTP("myauth")
        assert "myauth" in repr(p)

    def test_str(self) -> None:
        assert str(HTTP()) == "http"
        assert str(Socks5()) == "socks5"

    def test_copy(self) -> None:
        import copy as copy_mod
        original = HTTP("auth")
        original.target = "t"
        copied = copy_mod.copy(original)
        assert copied == original
        assert copied is not original

    def test_deepcopy(self) -> None:
        import copy as copy_mod
        original = HTTP("auth")
        original.target = "t"
        deep = copy_mod.deepcopy(original)
        assert deep == original
        assert deep is not original

    def test_reduce(self) -> None:
        p = HTTP("auth")
        p.target = "t"
        cls, args, state = p.__reduce__()
        assert cls is HTTP
        assert args == ("auth",)
        assert state == {"target": "t"}
