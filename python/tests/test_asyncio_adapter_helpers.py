"""Tests for pproxy-specific stream helpers on CompatibleStreamReader.

Covers read_w, read_n, read_until (timeout wrapper), and rollback.
"""

from __future__ import annotations

import asyncio

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

    def write(self, data: bytes) -> None:
        self._buf.extend(data)

    def drain(self) -> None:
        pass

    def write_eof(self) -> None:
        pass

    def close(self) -> None:
        self._closed = True

    def is_closing(self) -> bool:
        return self._closed

    def get_extra_info(self, key: str, default=None):
        return default

    async def wait_closed(self) -> None:
        pass


def _reader(data: bytes = b"") -> CompatibleStreamReader:
    return CompatibleStreamReader(_FakeStream(data))


# -----------------------------------------------------------------------
# read_w
# -----------------------------------------------------------------------

class TestReadW:
    def test_reads_up_to_n_bytes(self):
        r = _reader(b"hello world")
        result = asyncio.run(r.read_w(5))
        assert result == b"hello"

    def test_reads_less_than_n_if_available(self):
        r = _reader(b"hi")
        result = asyncio.run(r.read_w(100))
        assert result == b"hi"

    def test_default_n_reads_large_n(self):
        # read(n) with large n drains transport and returns available data
        r = _reader(b"all data")
        result = asyncio.run(r.read_w(1000))
        assert result == b"all data"

    def test_empty_at_eof(self):
        r = _reader(b"")
        result = asyncio.run(r.read_w(-1))
        assert result == b""

    def test_consecutive_reads(self):
        r = _reader(b"abcdefgh")
        assert asyncio.run(r.read_w(3)) == b"abc"
        assert asyncio.run(r.read_w(3)) == b"def"
        assert asyncio.run(r.read_w(3)) == b"gh"
        assert asyncio.run(r.read_w(3)) == b""

    def test_timeout_raises(self):
        stream = _FakeStream()
        r = CompatibleStreamReader(stream)
        # Make read block forever by never returning data
        original_read = stream.read

        async def _blocking_read(n: int = -1) -> bytes:
            await asyncio.sleep(100)
            return b""

        stream.read = _blocking_read  # type: ignore[assignment]
        with pytest.raises(asyncio.TimeoutError):
            asyncio.run(r.read_w(1))


# -----------------------------------------------------------------------
# read_n
# -----------------------------------------------------------------------

class TestReadN:
    def test_reads_exactly_n_bytes(self):
        r = _reader(b"abcdef")
        result = asyncio.run(r.read_n(3))
        assert result == b"abc"

    def test_incomplete_read_on_eof(self):
        r = _reader(b"ab")
        with pytest.raises(asyncio.IncompleteReadError) as exc_info:
            asyncio.run(r.read_n(5))
        assert exc_info.value.partial == b"ab"
        assert exc_info.value.expected == 5

    def test_sequential_reads(self):
        r = _reader(b"123456")
        assert asyncio.run(r.read_n(2)) == b"12"
        assert asyncio.run(r.read_n(2)) == b"34"
        assert asyncio.run(r.read_n(2)) == b"56"

    def test_zero_bytes(self):
        r = _reader(b"any")
        assert asyncio.run(r.read_n(0)) == b""

    def test_timeout_raises(self):
        stream = _FakeStream()
        r = CompatibleStreamReader(stream)

        async def _blocking_read(n: int = -1) -> bytes:
            await asyncio.sleep(100)
            return b""

        stream.read = _blocking_read  # type: ignore[assignment]
        with pytest.raises(asyncio.TimeoutError):
            asyncio.run(r.read_n(10))


# -----------------------------------------------------------------------
# read_until
# -----------------------------------------------------------------------

class TestReadUntil:
    def test_finds_separator(self):
        r = _reader(b"line1\nline2\n")
        result = asyncio.run(r.read_until(b"\n"))
        assert result == b"line1\n"

    def test_default_separator_is_newline(self):
        r = _reader(b"abc\ndef")
        result = asyncio.run(r.read_until())
        assert result == b"abc\n"

    def test_custom_separator(self):
        r = _reader(b"aaa>>>bbb")
        result = asyncio.run(r.read_until(b">>>"))
        assert result == b"aaa>>>"

    def test_separator_at_start(self):
        r = _reader(b"\nrest")
        result = asyncio.run(r.read_until(b"\n"))
        assert result == b"\n"

    def test_eof_without_separator_raises(self):
        r = _reader(b"no newline here")
        with pytest.raises(asyncio.IncompleteReadError):
            asyncio.run(r.read_until(b"\n"))

    def test_timeout_raises(self):
        stream = _FakeStream()
        r = CompatibleStreamReader(stream)

        async def _blocking_read(n: int = -1) -> bytes:
            await asyncio.sleep(100)
            return b""

        stream.read = _blocking_read  # type: ignore[assignment]
        with pytest.raises(asyncio.TimeoutError):
            asyncio.run(r.read_until(b"\n"))


# -----------------------------------------------------------------------
# rollback
# -----------------------------------------------------------------------

class TestRollback:
    def test_rollback_pushes_data_into_buffer(self):
        r = _reader(b"world")
        r.rollback(b"hello ")
        result = asyncio.run(r.read(100))
        assert result == b"hello world"

    def test_rollback_before_read(self):
        r = _reader()
        r.rollback(b"prepended")
        assert asyncio.run(r.read(4)) == b"prep"

    def test_multiple_rollbacks_accumulate(self):
        r = _reader(b"c")
        r.rollback(b"b")
        r.rollback(b"a")
        result = asyncio.run(r.read(100))
        assert result == b"abc"

    def test_rollback_then_read_n(self):
        r = _reader(b"cd")
        r.rollback(b"ab")
        assert asyncio.run(r.read_n(4)) == b"abcd"

    def test_rollback_then_readuntil(self):
        r = _reader(b"world\n")
        r.rollback(b"hello ")
        result = asyncio.run(r.read_until(b"\n"))
        assert result == b"hello world\n"

    def test_rollback_empty_bytes(self):
        r = _reader(b"data")
        r.rollback(b"")
        assert asyncio.run(r.read(100)) == b"data"

    def test_rollback_interleaved_with_normal_reads(self):
        r = _reader(b"12345")
        assert asyncio.run(r.read(2)) == b"12"
        r.rollback(b"XX")
        assert asyncio.run(r.read(2)) == b"XX"
        assert asyncio.run(r.read(100)) == b"345"
