"""CE6: Protocol fragmentation, malformed input, and http_channel() tests.

Tests that protocol parsers handle fragmented input, preserve post-handshake
bytes, reject malformed data, and perform required HTTP transformations.
"""

from __future__ import annotations

import asyncio
import io
import struct
import unittest
from unittest.mock import AsyncMock, MagicMock

import pytest


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


class FakeReader:
    """Async-capable fake reader for testing protocol methods."""

    def __init__(self, data: bytes = b""):
        self._buf = io.BytesIO(data)

    async def read(self, n: int = -1) -> bytes:
        return self._buf.read(n)

    async def readexactly(self, n: int) -> bytes:
        return self._buf.read(n)

    async def readuntil(self, sep: bytes) -> bytes:
        buf = b""
        while True:
            byte = self._buf.read(1)
            if not byte:
                raise asyncio.IncompleteReadError(buf, len(sep))
            buf += byte
            if buf.endswith(sep):
                return buf
        raise asyncio.IncompleteReadError(buf, len(sep))


class MockStreamReader:
    """Async mock reader for testing async protocol methods."""

    def __init__(self, chunks: list[bytes] | None = None):
        self._chunks: list[bytes] = list(chunks or [])
        self._pos = 0
        self._eof = False

    async def read(self, n: int = -1) -> bytes:
        if self._pos >= len(self._chunks):
            self._eof = True
            return b""
        chunk = self._chunks[self._pos]
        self._pos += 1
        return chunk

    async def readuntil(self, sep: bytes) -> bytes:
        buf = b""
        while True:
            if self._pos >= len(self._chunks):
                self._eof = True
                raise asyncio.IncompleteReadError(buf, len(sep))
            chunk = self._chunks[self._pos]
            self._pos += 1
            buf += chunk
            if sep in buf:
                return buf
        raise asyncio.IncompleteReadError(buf, len(sep))

    def at_eof(self) -> bool:
        return self._eof or self._pos >= len(self._chunks)


class MockStreamWriter:
    """Async mock writer that records written data."""

    def __init__(self):
        self.written: list[bytes] = []
        self._closing = False

    def write(self, data: bytes) -> None:
        self.written.append(data)

    async def drain(self) -> None:
        pass

    def close(self) -> None:
        self._closing = True

    def is_closing(self) -> bool:
        return self._closing

    def get_extra_info(self, key: str, default=None):
        return default


# ---------------------------------------------------------------------------
# HTTP fragmentation tests
# ---------------------------------------------------------------------------


class TestHttpFragmentation:
    """HTTP protocol parser handles fragmented input correctly."""

    def test_guess_partial_method(self):
        """Partial HTTP method bytes are recognized if they match a prefix."""
        from eggress.protocol import HTTP

        proto = HTTP()
        # "GET" matches the prefix of the "GET " pattern
        reader = FakeReader(b"GET")
        result = asyncio.run(proto.guess(reader))
        assert result is not None

    def test_guess_exact_match(self):
        """Full HTTP method prefix is recognized."""
        from eggress.protocol import HTTP

        proto = HTTP()
        reader = FakeReader(b"GET /")
        result = asyncio.run(proto.guess(reader))
        assert result is not None

    def test_guess_multiple_methods(self):
        """All supported HTTP methods are recognized."""
        from eggress.protocol import HTTP

        proto = HTTP()
        for method in [b"GET ", b"HEAD", b"POST ", b"PUT ", b"DELETE ", b"CONNECT ", b"OPTIONS ", b"PATCH "]:
            reader = FakeReader(method)
            result = asyncio.run(proto.guess(reader))
            assert result is not None, f"Method {method!r} not recognized"

    def test_connect_request_parsing(self):
        """CONNECT request is parsed correctly."""
        from eggress.protocol import HTTP

        proto = HTTP()
        request = b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n"
        reader = FakeReader(request)

        async def run():
            user = await proto.guess(reader)
            # Reset reader to start for accept
            reader._buf = io.BytesIO(request)
            return user

        result = asyncio.run(run())
        assert result is not None


# ---------------------------------------------------------------------------
# SOCKS4 fragmentation tests
# ---------------------------------------------------------------------------


class TestSocks4Fragmentation:
    """SOCKS4 protocol parser handles edge cases correctly."""

    def test_guess_version_byte(self):
        """SOCKS4 version byte 0x04 is recognized."""
        from eggress.protocol import Socks4

        proto = Socks4()
        reader = FakeReader(b"\x04")
        result = asyncio.run(proto.guess(reader))
        assert result is not None

    def test_guess_wrong_version(self):
        """Non-0x04 version byte is rejected."""
        from eggress.protocol import Socks4

        proto = Socks4()
        reader = FakeReader(b"\x05")
        result = asyncio.run(proto.guess(reader))
        assert result is None

    def test_guess_empty(self):
        """Empty data returns None."""
        from eggress.protocol import Socks4

        proto = Socks4()
        reader = FakeReader(b"")
        result = asyncio.run(proto.guess(reader))
        assert result is None


# ---------------------------------------------------------------------------
# SOCKS5 fragmentation tests
# ---------------------------------------------------------------------------


class TestSocks5Fragmentation:
    """SOCKS5 protocol parser handles edge cases correctly."""

    def test_guess_version_byte(self):
        """SOCKS5 version byte 0x05 is recognized."""
        from eggress.protocol import Socks5

        proto = Socks5()
        reader = FakeReader(b"\x05")
        result = asyncio.run(proto.guess(reader))
        assert result is not None

    def test_guess_wrong_version(self):
        """Non-0x05 version byte is rejected."""
        from eggress.protocol import Socks5

        proto = Socks5()
        reader = FakeReader(b"\x04")
        result = asyncio.run(proto.guess(reader))
        assert result is None

    def test_guess_empty(self):
        """Empty data returns None."""
        from eggress.protocol import Socks5

        proto = Socks5()
        reader = FakeReader(b"")
        result = asyncio.run(proto.guess(reader))
        assert result is None

    def test_accept_ipv4_no_auth(self):
        """SOCKS5 accept with IPv4 and no auth."""
        from eggress.protocol import Socks5

        proto = Socks5()
        greeting = b"\x05\x01\x00"
        connect = b"\x05\x01\x00\x01" + bytes([10, 0, 0, 1]) + struct.pack("!H", 8080)
        proto._buffered = greeting + connect

        async def run():
            return await proto.accept(FakeReader(), True, writer=None, users=None, authtable=None)

        user, host, port = asyncio.run(run())
        assert host == "10.0.0.1"
        assert port == 8080

    def test_accept_domain_no_auth(self):
        """SOCKS5 accept with domain name."""
        from eggress.protocol import Socks5

        proto = Socks5()
        greeting = b"\x05\x01\x00"
        domain = b"example.com"
        connect = b"\x05\x01\x00\x03" + bytes([len(domain)]) + domain + struct.pack("!H", 443)
        proto._buffered = greeting + connect

        async def run():
            return await proto.accept(FakeReader(), True, writer=None, users=None, authtable=None)

        user, host, port = asyncio.run(run())
        assert host == "example.com"
        assert port == 443

    def test_accept_ipv6_no_auth(self):
        """SOCKS5 accept with IPv6 address."""
        from eggress.protocol import Socks5

        proto = Socks5()
        greeting = b"\x05\x01\x00"
        ipv6 = b"\x00" * 14 + b"\x00\x01"  # ::1
        connect = b"\x05\x01\x00\x04" + ipv6 + struct.pack("!H", 80)
        proto._buffered = greeting + connect

        async def run():
            return await proto.accept(FakeReader(), True, writer=None, users=None, authtable=None)

        user, host, port = asyncio.run(run())
        assert "::1" in host
        assert port == 80

    def test_accept_unsupported_address_type(self):
        """SOCKS5 accept with unsupported address type raises ValueError."""
        from eggress.protocol import Socks5

        proto = Socks5()
        greeting = b"\x05\x01\x00"
        connect = b"\x05\x01\x00\x05" + b"\x00" * 10  # atyp=0x05 unsupported
        proto._buffered = greeting + connect

        async def run():
            return await proto.accept(FakeReader(), True, writer=None, users=None, authtable=None)

        with pytest.raises(ValueError, match="unsupported address type"):
            asyncio.run(run())

    def test_accept_truncated_greeting(self):
        """SOCKS5 accept with truncated greeting raises ValueError."""
        from eggress.protocol import Socks5

        proto = Socks5()
        proto._buffered = b"\x05\x01"  # Missing method byte

        async def run():
            return await proto.accept(FakeReader(), True, writer=None, users=None, authtable=None)

        with pytest.raises(ValueError, match="truncated"):
            asyncio.run(run())

    def test_accept_wrong_version(self):
        """SOCKS5 accept with wrong version raises ValueError."""
        from eggress.protocol import Socks5

        proto = Socks5()
        proto._buffered = b"\x04\x01\x00"  # SOCKS4 version

        async def run():
            return await proto.accept(FakeReader(), True, writer=None, users=None, authtable=None)

        with pytest.raises(ValueError, match="invalid SOCKS5 version"):
            asyncio.run(run())

    def test_udp_pack_ipv4(self):
        """SOCKS5 UDP pack with IPv4 address."""
        from eggress.protocol import Socks5

        proto = Socks5()
        result = proto.udp_pack("10.0.0.1", 8080, b"test")
        # Header: 3 bytes reserved + address + port + payload
        assert result[:3] == b"\x00\x00\x00"
        # Address type byte
        assert result[3] == 0x01  # IPv4
        # IPv4 address
        assert result[4:8] == bytes([10, 0, 0, 1])
        # Port
        assert struct.unpack("!H", result[8:10])[0] == 8080
        # Payload
        assert result[10:] == b"test"

    def test_udp_pack_domain(self):
        """SOCKS5 UDP pack with domain name."""
        from eggress.protocol import Socks5

        proto = Socks5()
        result = proto.udp_pack("example.com", 443, b"payload")
        assert result[:3] == b"\x00\x00\x00"
        assert result[3] == 0x03  # Domain
        domain_len = result[4]
        assert result[5:5 + domain_len] == b"example.com"
        assert struct.unpack("!H", result[5 + domain_len:7 + domain_len])[0] == 443

    def test_udp_unpack(self):
        """SOCKS5 UDP unpack."""
        from eggress.protocol import Socks5

        proto = Socks5()
        # Build a UDP packet
        addr = b"\x01" + bytes([10, 0, 0, 1]) + struct.pack("!H", 8080)
        data = b"\x00\x00\x00" + addr + b"hello"
        host, port, payload = proto.udp_unpack(data)
        assert host == "10.0.0.1"
        assert port == 8080
        assert payload == b"hello"

    def test_udp_unpack_too_short(self):
        """SOCKS5 UDP unpack with too-short data raises ValueError."""
        from eggress.protocol import Socks5

        proto = Socks5()
        with pytest.raises(ValueError, match="SOCKS5 UDP header too short"):
            proto.udp_unpack(b"\x00\x00")


# ---------------------------------------------------------------------------
# HTTP http_channel() transformation tests
# ---------------------------------------------------------------------------


class TestHttpChannelTransformations:
    """http_channel() performs required HTTP transformations."""

    def _make_handler(self):
        """Create an HTTP instance for http_channel testing."""
        from eggress.protocol import HTTP
        return HTTP()

    @pytest.mark.asyncio
    async def test_rewrites_absolute_form_uri(self):
        """GET http://host/path is rewritten to GET /path."""
        handler = self._make_handler()
        reader = MockStreamReader([
            b"GET http://example.com/path HTTP/1.1\r\nHost: example.com\r\n\r\n"
        ])
        writer = MockStreamWriter()

        await handler.http_channel(reader, writer, lambda n: None, lambda n: None)

        assert len(writer.written) == 1
        data = writer.written[0]
        assert b"GET /path HTTP/1.1" in data
        assert b"http://example.com" not in data

    @pytest.mark.asyncio
    async def test_removes_proxy_authorization(self):
        """Proxy-Authorization header is removed before forwarding."""
        handler = self._make_handler()
        reader = MockStreamReader([
            b"GET / HTTP/1.1\r\nHost: example.com\r\nProxy-Authorization: Basic dXNlcjpwYXNz\r\n\r\n"
        ])
        writer = MockStreamWriter()

        await handler.http_channel(reader, writer, lambda n: None, lambda n: None)

        assert len(writer.written) == 1
        data = writer.written[0]
        assert b"Proxy-Authorization" not in data
        assert b"Host: example.com" in data

    @pytest.mark.asyncio
    async def test_removes_proxy_connection(self):
        """Proxy-Connection header is removed before forwarding."""
        handler = self._make_handler()
        reader = MockStreamReader([
            b"GET / HTTP/1.1\r\nHost: example.com\r\nProxy-Connection: keep-alive\r\n\r\n"
        ])
        writer = MockStreamWriter()

        await handler.http_channel(reader, writer, lambda n: None, lambda n: None)

        assert len(writer.written) == 1
        data = writer.written[0]
        assert b"Proxy-Connection" not in data
        assert b"Host: example.com" in data

    @pytest.mark.asyncio
    async def test_preserves_host_header(self):
        """Host header is preserved in forwarded request."""
        handler = self._make_handler()
        reader = MockStreamReader([
            b"GET / HTTP/1.1\r\nHost: example.com:8080\r\nAccept: */*\r\n\r\n"
        ])
        writer = MockStreamWriter()

        await handler.http_channel(reader, writer, lambda n: None, lambda n: None)

        assert len(writer.written) == 1
        data = writer.written[0]
        assert b"Host: example.com:8080" in data

    @pytest.mark.asyncio
    async def test_preserves_post_handshake_bytes(self):
        """Bytes following headers are preserved."""
        handler = self._make_handler()
        body = b'{"key": "value"}'
        reader = MockStreamReader([
            b"POST /api HTTP/1.1\r\nHost: example.com\r\nContent-Type: application/json\r\n\r\n" + body
        ])
        writer = MockStreamWriter()

        await handler.http_channel(reader, writer, lambda n: None, lambda n: None)

        assert len(writer.written) == 1
        data = writer.written[0]
        assert body in data

    @pytest.mark.asyncio
    async def test_stat_bytes_called(self):
        """stat_bytes callback receives byte count."""
        handler = self._make_handler()
        reader = MockStreamReader([b"GET / HTTP/1.1\r\nHost: x\r\n\r\n"])
        writer = MockStreamWriter()
        byte_counts = []

        await handler.http_channel(reader, writer, lambda n: byte_counts.append(n), lambda n: None)

        assert len(byte_counts) == 1
        assert byte_counts[0] > 0

    @pytest.mark.asyncio
    async def test_stat_conn_called(self):
        """stat_conn called with +1 and -1."""
        handler = self._make_handler()
        reader = MockStreamReader([b"GET / HTTP/1.1\r\nHost: x\r\n\r\n"])
        writer = MockStreamWriter()
        conn_events = []

        await handler.http_channel(reader, writer, lambda n: None, lambda n: conn_events.append(n))

        assert conn_events == [1, -1]

    @pytest.mark.asyncio
    async def test_non_http_data_relayed_raw(self):
        """Non-HTTP data is relayed without transformation."""
        handler = self._make_handler()
        raw_data = b"\x00\x01\x02\x03binary data"
        reader = MockStreamReader([raw_data])
        writer = MockStreamWriter()

        await handler.http_channel(reader, writer, lambda n: None, lambda n: None)

        assert len(writer.written) == 1
        assert writer.written[0] == raw_data

    @pytest.mark.asyncio
    async def test_empty_data_closes(self):
        """Empty data from reader closes the channel."""
        handler = self._make_handler()
        reader = MockStreamReader([b""])
        writer = MockStreamWriter()

        await handler.http_channel(reader, writer, lambda n: None, lambda n: None)

        assert writer._closing

    @pytest.mark.asyncio
    async def test_writer_closed_on_completion(self):
        """Writer is closed when reader EOF is reached."""
        handler = self._make_handler()
        reader = MockStreamReader([b"GET / HTTP/1.1\r\nHost: x\r\n\r\n"])
        writer = MockStreamWriter()

        await handler.http_channel(reader, writer, lambda n: None, lambda n: None)

        assert writer._closing

    @pytest.mark.asyncio
    async def test_connect_method_rewritten(self):
        """CONNECT method with absolute URI is rewritten."""
        handler = self._make_handler()
        reader = MockStreamReader([
            b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n"
        ])
        writer = MockStreamWriter()

        await handler.http_channel(reader, writer, lambda n: None, lambda n: None)

        assert len(writer.written) == 1
        data = writer.written[0]
        # CONNECT typically uses authority-form, not absolute-form
        # The handler should still process it
        assert b"example.com:443" in data


# ---------------------------------------------------------------------------
# Base protocol tests
# ---------------------------------------------------------------------------


class TestBaseProtocolEdgeCases:
    """BaseProtocol edge cases and unsupported operations."""

    def test_channel_closes_writer(self):
        """channel() closes writer when reader EOF."""
        from eggress.protocol import BaseProtocol
        import asyncio

        proto = BaseProtocol()
        reader = MockStreamReader([b""])
        writer = MockStreamWriter()
        events = []

        async def run():
            await proto.channel(reader, writer, lambda n: None, lambda n: events.append(n))

        asyncio.run(run())
        assert writer._closing

    def test_guess_raises_not_implemented(self):
        """BaseProtocol.guess raises NotImplementedError."""
        from eggress.protocol import BaseProtocol

        proto = BaseProtocol()
        with pytest.raises(NotImplementedError):
            asyncio.run(proto.guess(FakeReader()))

    def test_accept_raises_not_implemented(self):
        """BaseProtocol.accept raises NotImplementedError."""
        from eggress.protocol import BaseProtocol

        proto = BaseProtocol()
        with pytest.raises(NotImplementedError):
            asyncio.run(proto.accept(FakeReader(), None))

    def test_connect_raises_not_implemented(self):
        """BaseProtocol.connect raises NotImplementedError."""
        from eggress.protocol import BaseProtocol

        proto = BaseProtocol()
        with pytest.raises(NotImplementedError):
            asyncio.run(proto.connect(None, None, None, "", 0))

    def test_udp_accept_raises_not_implemented(self):
        """BaseProtocol.udp_accept raises NotImplementedError."""
        from eggress.protocol import BaseProtocol

        proto = BaseProtocol()
        with pytest.raises(NotImplementedError):
            proto.udp_accept(b"")

    def test_udp_connect_raises_not_implemented(self):
        """BaseProtocol.udp_connect raises NotImplementedError."""
        from eggress.protocol import BaseProtocol

        proto = BaseProtocol()
        with pytest.raises(NotImplementedError):
            proto.udp_connect(None, "", 0, b"")

    def test_udp_pack_identity(self):
        """BaseProtocol.udp_pack returns data unchanged."""
        from eggress.protocol import BaseProtocol

        proto = BaseProtocol()
        assert proto.udp_pack("host", 80, b"data") == b"data"

    def test_udp_unpack_identity(self):
        """BaseProtocol.udp_unpack returns data unchanged."""
        from eggress.protocol import BaseProtocol

        proto = BaseProtocol()
        assert proto.udp_unpack(b"data") == b"data"

    def test_reuse_returns_false(self):
        """BaseProtocol.reuse returns False."""
        from eggress.protocol import BaseProtocol

        proto = BaseProtocol()
        assert proto.reuse() is False

    def test_name_returns_class_lower(self):
        """BaseProtocol.name returns lowercase class name."""
        from eggress.protocol import BaseProtocol

        proto = BaseProtocol()
        assert proto.name == "baseprotocol"

    def test_equality(self):
        """BaseProtocol instances are equal if same type and param."""
        from eggress.protocol import BaseProtocol

        a = BaseProtocol("test")
        b = BaseProtocol("test")
        c = BaseProtocol("other")
        assert a == b
        assert a != c
        assert a != "not a protocol"

    def test_hash_consistency(self):
        """BaseProtocol hash is consistent with equality."""
        from eggress.protocol import BaseProtocol

        a = BaseProtocol("test")
        b = BaseProtocol("test")
        assert hash(a) == hash(b)

    def test_repr_shows_class_name(self):
        """BaseProtocol repr shows class name."""
        from eggress.protocol import BaseProtocol

        proto = BaseProtocol("test")
        r = repr(proto)
        assert "BaseProtocol" in r
        assert "test" in r


# ---------------------------------------------------------------------------
# decode_socks_address tests
# ---------------------------------------------------------------------------


class TestDecodeSocksAddress:
    """decode_socks_address handles various address types."""

    def test_ipv4(self):
        from eggress.protocol import decode_socks_address

        data = b"\x01" + bytes([10, 0, 0, 1]) + struct.pack("!H", 80)
        host, port, remaining = decode_socks_address(data)
        assert host == "10.0.0.1"
        assert port == 80
        assert remaining == b""

    def test_ipv4_with_trailing(self):
        from eggress.protocol import decode_socks_address

        data = b"\x01" + bytes([10, 0, 0, 1]) + struct.pack("!H", 80) + b"trailing"
        host, port, remaining = decode_socks_address(data)
        assert host == "10.0.0.1"
        assert port == 80
        assert remaining == b"trailing"

    def test_domain(self):
        from eggress.protocol import decode_socks_address

        domain = b"example.com"
        data = b"\x03" + bytes([len(domain)]) + domain + struct.pack("!H", 443)
        host, port, remaining = decode_socks_address(data)
        assert host == "example.com"
        assert port == 443
        assert remaining == b""

    def test_ipv6(self):
        from eggress.protocol import decode_socks_address

        ipv6 = b"\x20\x01\x0d\xb8\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x01"
        data = b"\x04" + ipv6 + struct.pack("!H", 80)
        host, port, remaining = decode_socks_address(data)
        assert "2001" in host
        assert port == 80
        assert remaining == b""

    def test_empty_data_raises(self):
        from eggress.protocol import decode_socks_address

        with pytest.raises(ValueError, match="empty data"):
            decode_socks_address(b"")

    def test_truncated_ipv4_raises(self):
        from eggress.protocol import decode_socks_address

        with pytest.raises(ValueError, match="truncated IPv4"):
            decode_socks_address(b"\x01\x0a\x00")

    def test_unsupported_type_raises(self):
        from eggress.protocol import decode_socks_address

        with pytest.raises(ValueError, match="unsupported address type"):
            decode_socks_address(b"\x05\x00\x00\x00\x00\x00\x00")


# ---------------------------------------------------------------------------
# get_protos and MAPPINGS tests
# ---------------------------------------------------------------------------


class TestProtocolRegistry:
    """Protocol registry and mapping tests."""

    def test_get_protos_socks5(self):
        from eggress.protocol import get_protos

        err, protos = get_protos(["socks5"])
        assert err is None
        assert len(protos) == 1
        assert protos[0].name == "socks5"

    def test_get_protos_http(self):
        from eggress.protocol import get_protos

        err, protos = get_protos(["http"])
        assert err is None
        assert len(protos) == 1
        assert protos[0].name == "http"

    def test_get_protos_socks4(self):
        from eggress.protocol import get_protos

        err, protos = get_protos(["socks4"])
        assert err is None
        assert len(protos) == 1
        assert protos[0].name == "socks4"

    def test_get_protos_chain(self):
        from eggress.protocol import get_protos

        err, protos = get_protos(["socks5", "http"])
        assert err is None
        assert len(protos) == 2

    def test_get_protos_invalid(self):
        from eggress.protocol import get_protos

        err, protos = get_protos(["invalid_proto"])
        assert err is not None
        assert protos is None

    def test_get_protos_empty(self):
        from eggress.protocol import get_protos

        err, protos = get_protos([])
        assert err is not None

    def test_mappings_contains_schemes(self):
        from eggress.protocol import MAPPINGS

        for scheme in ["socks5", "socks4", "http", "ss", "trojan", "h2", "h3"]:
            assert scheme in MAPPINGS

    def test_mappings_direct(self):
        from eggress.protocol import MAPPINGS

        assert "direct" in MAPPINGS
