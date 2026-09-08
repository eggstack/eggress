"""Tests for CompatibleStreamReader readline/iteration asyncio contracts.

`readline()` must match :meth:`asyncio.StreamReader.readline` EOF semantics
(return remaining bytes on unterminated final lines, ``b""`` at clean EOF),
and ``__aiter__`` must synchronously return the iterator per the async
iterator protocol. ``readuntil()`` keeps its documented
:exc:`asyncio.IncompleteReadError` behavior independently.
"""

from __future__ import annotations

import asyncio
import inspect
import warnings

import pytest

from eggress._asyncio_adapter import CompatibleStreamReader


class _FakeStream:
    """Minimal AsyncOutboundStream stub backed by a bytearray."""

    def __init__(self, data: bytes = b"") -> None:
        self._buf = bytearray(data)
        self._closed = False

    async def read(self, n: int = -1) -> bytes:
        if not self._buf:
            return b""
        if n < 0:
            data = bytes(self._buf)
            self._buf.clear()
            return data
        take = min(n, len(self._buf))
        data = bytes(self._buf[:take])
        del self._buf[:take]
        return data

    def get_extra_info(self, key: str, default=None):
        return default


def _reader(data: bytes = b"") -> CompatibleStreamReader:
    return CompatibleStreamReader(_FakeStream(data))


def _stdlib_lines(data: bytes) -> list[bytes]:
    """Collect readline() results from a real asyncio.StreamReader."""

    async def _collect() -> list[bytes]:
        reader = asyncio.StreamReader()
        reader.feed_data(data)
        reader.feed_eof()
        lines = []
        while True:
            line = await reader.readline()
            if line == b"":
                break
            lines.append(line)
        return lines

    return asyncio.run(_collect())


def _adapter_lines(data: bytes) -> list[bytes]:
    reader = _reader(data)

    async def _collect() -> list[bytes]:
        lines = []
        while True:
            line = await reader.readline()
            if line == b"":
                break
            lines.append(line)
        return lines

    return asyncio.run(_collect())


class TestReadline:
    def test_line_ending_with_newline(self):
        r = _reader(b"hello\nworld\n")
        assert asyncio.run(r.readline()) == b"hello\n"
        assert asyncio.run(r.readline()) == b"world\n"

    def test_final_line_without_newline_returned(self):
        r = _reader(b"last line")
        assert asyncio.run(r.readline()) == b"last line"

    def test_clean_eof_returns_empty(self):
        r = _reader(b"")
        assert asyncio.run(r.readline()) == b""
        # Repeated reads at clean EOF stay empty.
        assert asyncio.run(r.readline()) == b""

    def test_empty_after_final_line(self):
        r = _reader(b"tail")
        assert asyncio.run(r.readline()) == b"tail"
        assert asyncio.run(r.readline()) == b""

    def test_multiple_lines_then_unterminated_final(self):
        r = _reader(b"a\nb\nc")
        assert asyncio.run(r.readline()) == b"a\n"
        assert asyncio.run(r.readline()) == b"b\n"
        assert asyncio.run(r.readline()) == b"c"
        assert asyncio.run(r.readline()) == b""

    def test_empty_lines_preserved(self):
        r = _reader(b"\n\nx\n")
        assert asyncio.run(r.readline()) == b"\n"
        assert asyncio.run(r.readline()) == b"\n"
        assert asyncio.run(r.readline()) == b"x\n"
        assert asyncio.run(r.readline()) == b""


class TestReadlineMatchesStdlib:
    @pytest.mark.parametrize(
        "data",
        [
            b"",
            b"\n",
            b"no newline",
            b"one\n",
            b"a\nb\nc",
            b"a\nb\nc\n",
            b"\n\n",
            b"x" * 9000 + b"\n" + b"y" * 100,
        ],
    )
    def test_same_byte_sequences(self, data: bytes):
        assert _adapter_lines(data) == _stdlib_lines(data)


class TestAsyncIteration:
    def test_aiter_returns_self_synchronously(self):
        r = _reader(b"a\n")
        it = r.__aiter__()
        assert it is r
        assert not inspect.isawaitable(it)

    def test_aiter_emits_no_warning(self):
        r = _reader(b"a\n")
        with warnings.catch_warnings():
            warnings.simplefilter("error")
            result = r.__aiter__()
            assert result is r

    def test_async_for_over_terminated_lines(self):
        async def _collect() -> list[bytes]:
            return [line async for line in _reader(b"a\nb\nc\n")]

        assert asyncio.run(_collect()) == [b"a\n", b"b\n", b"c\n"]

    def test_async_for_yields_unterminated_final_once(self):
        async def _collect() -> list[bytes]:
            return [line async for line in _reader(b"a\nb\ntail")]

        assert asyncio.run(_collect()) == [b"a\n", b"b\n", b"tail"]

    def test_async_for_empty_stream(self):
        async def _collect() -> list[bytes]:
            return [line async for line in _reader(b"")]

        assert asyncio.run(_collect()) == []

    def test_anext_stops_after_final_line(self):
        async def _drive() -> list[bytes]:
            r = _reader(b"solo")
            first = await r.__anext__()
            with pytest.raises(StopAsyncIteration):
                await r.__anext__()
            return [first]

        assert asyncio.run(_drive()) == [b"solo"]


class TestReaduntilPreserved:
    def test_missing_separator_at_eof_still_raises(self):
        r = _reader(b"no newline here")
        with pytest.raises(asyncio.IncompleteReadError):
            asyncio.run(r.readuntil(b"\n"))

    def test_readline_after_failed_readuntil(self):
        # A failed readuntil consumes nothing (partial stays buffered), so a
        # subsequent readline still returns the pending bytes.
        r = _reader(b"pending")
        with pytest.raises(asyncio.IncompleteReadError):
            asyncio.run(r.readuntil(b"\n"))
        assert asyncio.run(r.readline()) == b"pending"
