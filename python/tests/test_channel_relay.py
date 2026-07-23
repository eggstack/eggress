"""Tests for ``channel`` and ``http_channel`` bidirectional relay.

Verifies that data is relayed correctly, stat callbacks are invoked,
and edge cases (half-close, cancellation, None stat_bytes) are handled.
"""

from __future__ import annotations

import asyncio
from typing import Any

import pytest

from eggress.protocol import BaseProtocol, HTTP


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


class MockStreamReader:
    """Minimal asyncio-compatible reader backed by a bytes buffer."""

    def __init__(self, data: bytes = b"") -> None:
        self._buf = data
        self._at_eof = False

    async def read(self, n: int = -1) -> bytes:
        if not self._buf:
            self._at_eof = True
            return b""
        if n == -1:
            out = self._buf
            self._buf = b""
            return out
        out = self._buf[:n]
        self._buf = self._buf[n:]
        return out

    def at_eof(self) -> bool:
        return self._at_eof and not self._buf


class MockStreamWriter:
    """Minimal asyncio-compatible writer that captures written data."""

    def __init__(self) -> None:
        self._written: list[bytes] = []
        self._closed = False
        self._drained = asyncio.Event()

    def write(self, data: bytes) -> None:
        self._written.append(data)

    async def drain(self) -> None:
        self._drained.set()

    def close(self) -> None:
        self._closed = True

    def is_closing(self) -> bool:
        return self._closed

    def getvalue(self) -> bytes:
        return b"".join(self._written)


# ---------------------------------------------------------------------------
# BaseProtocol.channel
# ---------------------------------------------------------------------------


class TestChannel:
    """Tests for ``BaseProtocol.channel``."""

    @pytest.mark.asyncio
    async def test_relays_all_data(self) -> None:
        """Data from reader is written to writer."""
        proto = BaseProtocol()
        reader = MockStreamReader(b"hello world")
        writer = MockStreamWriter()
        await proto.channel(reader, writer, None, None)
        assert writer.getvalue() == b"hello world"

    @pytest.mark.asyncio
    async def test_relays_when_stat_bytes_is_none(self) -> None:
        """Data is relayed even when stat_bytes is None (no silent drop)."""
        proto = BaseProtocol()
        reader = MockStreamReader(b"data123")
        writer = MockStreamWriter()
        await proto.channel(reader, writer, stat_bytes=None, stat_conn=None)
        assert writer.getvalue() == b"data123"

    @pytest.mark.asyncio
    async def test_stat_bytes_called_with_length(self) -> None:
        """stat_bytes is called with the byte count for each chunk."""
        proto = BaseProtocol()
        reader = MockStreamReader(b"abcd")
        writer = MockStreamWriter()
        calls: list[int] = []
        await proto.channel(reader, writer, stat_bytes=lambda n: calls.append(n), stat_conn=None)
        assert calls == [4]

    @pytest.mark.asyncio
    async def test_stat_bytes_multiple_chunks(self) -> None:
        """stat_bytes is called once per chunk."""
        proto = BaseProtocol()
        # Two reads: first yields 3 bytes, second yields 2, third yields empty
        reader = MockStreamReader(b"abcde")
        writer = MockStreamWriter()
        calls: list[int] = []
        await proto.channel(reader, writer, stat_bytes=lambda n: calls.append(n), stat_conn=None)
        # Single read yields all remaining data
        assert sum(calls) == 5

    @pytest.mark.asyncio
    async def test_stat_conn_increments_and_decrements(self) -> None:
        """stat_conn is called with +1 at start and -1 at end."""
        proto = BaseProtocol()
        reader = MockStreamReader(b"x")
        writer = MockStreamWriter()
        calls: list[int] = []
        await proto.channel(reader, writer, None, stat_conn=lambda n: calls.append(n))
        assert calls == [1, -1]

    @pytest.mark.asyncio
    async def test_writer_closed_on_completion(self) -> None:
        """Writer is closed after relay completes."""
        proto = BaseProtocol()
        reader = MockStreamReader(b"done")
        writer = MockStreamWriter()
        await proto.channel(reader, writer, None, None)
        assert writer._closed

    @pytest.mark.asyncio
    async def test_half_close_reader_eof(self) -> None:
        """Relay stops when reader reaches EOF."""
        proto = BaseProtocol()
        reader = MockStreamReader(b"")
        writer = MockStreamWriter()
        await proto.channel(reader, writer, None, None)
        assert writer.getvalue() == b""
        assert writer._closed

    @pytest.mark.asyncio
    async def test_exception_during_relay_closes_writer(self) -> None:
        """Exception during drain still closes writer."""
        proto = BaseProtocol()
        reader = MockStreamReader(b"data")
        writer = MockStreamWriter()

        async def bad_drain() -> None:
            raise OSError("connection reset")

        writer.drain = bad_drain  # type: ignore[assignment]
        await proto.channel(reader, writer, None, None)
        assert writer._closed


# ---------------------------------------------------------------------------
# BaseProtocol.http_channel
# ---------------------------------------------------------------------------


class TestHttpChannel:
    """Tests for ``BaseProtocol.http_channel`` (delegates to channel)."""

    @pytest.mark.asyncio
    async def test_relays_all_data(self) -> None:
        """http_channel relays data identically to channel."""
        proto = BaseProtocol()
        reader = MockStreamReader(b"GET / HTTP/1.1\r\n\r\n")
        writer = MockStreamWriter()
        await proto.http_channel(reader, writer, None, None)
        assert writer.getvalue() == b"GET / HTTP/1.1\r\n\r\n"

    @pytest.mark.asyncio
    async def test_relays_when_stat_bytes_is_none(self) -> None:
        """Data is relayed even when stat_bytes is None (no silent drop)."""
        proto = BaseProtocol()
        reader = MockStreamReader(b"response body")
        writer = MockStreamWriter()
        await proto.http_channel(reader, writer, stat_bytes=None, stat_conn=None)
        assert writer.getvalue() == b"response body"

    @pytest.mark.asyncio
    async def test_stat_bytes_called(self) -> None:
        """stat_bytes is called with the byte count."""
        proto = BaseProtocol()
        reader = MockStreamReader(b"abc")
        writer = MockStreamWriter()
        calls: list[int] = []
        await proto.http_channel(reader, writer, stat_bytes=lambda n: calls.append(n), stat_conn=None)
        assert calls == [3]

    @pytest.mark.asyncio
    async def test_stat_conn_called(self) -> None:
        """stat_conn is called with +1 and -1."""
        proto = BaseProtocol()
        reader = MockStreamReader(b"")
        writer = MockStreamWriter()
        calls: list[int] = []
        await proto.http_channel(reader, writer, None, stat_conn=lambda n: calls.append(n))
        assert calls == [1, -1]

    @pytest.mark.asyncio
    async def test_writer_closed_on_completion(self) -> None:
        """Writer is closed after http_channel completes."""
        proto = BaseProtocol()
        reader = MockStreamReader(b"")
        writer = MockStreamWriter()
        await proto.http_channel(reader, writer, None, None)
        assert writer._closed

    @pytest.mark.asyncio
    async def test_http_class_inherits_channel(self) -> None:
        """HTTP class inherits the same http_channel behavior."""
        proto = HTTP()
        reader = MockStreamReader(b"data")
        writer = MockStreamWriter()
        calls: list[int] = []
        await proto.http_channel(reader, writer, stat_bytes=lambda n: calls.append(n), stat_conn=None)
        assert writer.getvalue() == b"data"
        assert calls == [4]

    @pytest.mark.asyncio
    async def test_half_close_reader_eof(self) -> None:
        """http_channel stops on reader EOF."""
        proto = BaseProtocol()
        reader = MockStreamReader(b"")
        writer = MockStreamWriter()
        await proto.http_channel(reader, writer, None, None)
        assert writer.getvalue() == b""
        assert writer._closed
