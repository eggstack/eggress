"""Asyncio stream adapter for pproxy compatibility.

Wraps :class:`~eggress.outbound.OutboundConnector` /
:class:`~eggress.outbound.AsyncOutboundStream` into the
``asyncio.StreamReader`` / ``asyncio.StreamWriter`` interface expected by
pproxy's ``ProxyDirect.tcp_connect()``.

Usage::

    from eggress._asyncio_adapter import open_tcp_connection

    reader, writer = await open_tcp_connection("example.com", 443)
    writer.write(b"GET / HTTP/1.1\\r\\nHost: example.com\\r\\n\\r\\n")
    await writer.drain()
    data = await reader.read(4096)
    writer.close()
    await writer.wait_closed()
"""

from __future__ import annotations

import asyncio
import io
from typing import Any, Optional

from eggress.outbound import AsyncOutboundStream, OutboundConnector


class CompatibleStreamReader:
    """Asyncio ``StreamReader``-like wrapper over an :class:`AsyncOutboundStream`.

    Maintains an internal buffer so that ``readexactly``, ``readuntil``,
    and ``readline`` can satisfy their contracts even when the underlying
    transport delivers data in arbitrary chunks.

    This class is **not** a drop-in replacement for
    :class:`asyncio.StreamReader` — it is designed to satisfy the subset
    of the interface consumed by pproxy.
    """

    __slots__ = ("_stream", "_buffer", "_eof")

    def __init__(self, stream: AsyncOutboundStream) -> None:
        self._stream = stream
        self._buffer = bytearray()
        self._eof = False

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    async def _fill_buffer(self, n: int = 1) -> None:
        """Read from the transport until at least *n* bytes are buffered.

        Sets ``_eof`` when the transport is exhausted.
        """
        while len(self._buffer) < n and not self._eof:
            try:
                chunk = await self._stream.read(4096)
            except Exception:
                self._eof = True
                return
            if not chunk:
                self._eof = True
                return
            self._buffer.extend(chunk)

    # ------------------------------------------------------------------
    # Public API (asyncio.StreamReader-like)
    # ------------------------------------------------------------------

    async def read(self, n: int = -1) -> bytes:
        """Read up to *n* bytes.

        If *n* is ``-1`` (the default), read until EOF and return all
        available data.
        """
        if self._eof and not self._buffer:
            return b""
        if n == -1:
            if not self._eof:
                await self._fill_buffer(n=0)  # drain everything available
            data = bytes(self._buffer)
            self._buffer.clear()
            return data
        if n < 0:
            raise ValueError("read n must be >= 0 or -1")
        if len(self._buffer) >= n:
            data = bytes(self._buffer[:n])
            del self._buffer[:n]
            return data
        if not self._eof:
            await self._fill_buffer(n=n)
        take = min(n, len(self._buffer))
        data = bytes(self._buffer[:take])
        del self._buffer[:take]
        return data

    async def readexactly(self, n: int) -> bytes:
        """Read exactly *n* bytes.

        Raises :exc:`asyncio.IncompleteReadError` if EOF is reached
        before *n* bytes are available (matching the stdlib contract).
        """
        if n < 0:
            raise ValueError("readexactly n must be >= 0")
        if n == 0:
            return b""
        if not self._eof and len(self._buffer) < n:
            await self._fill_buffer(n=n)
        if len(self._buffer) < n:
            got = bytes(self._buffer)
            self._buffer.clear()
            raise asyncio.IncompleteReadError(got, n)
        data = bytes(self._buffer[:n])
        del self._buffer[:n]
        return data

    async def readuntil(self, separator: bytes = b"\n") -> bytes:
        """Read until *separator* is found.

        The separator is included in the returned data.  Raises
        :exc:`asyncio.IncompleteReadError` on EOF before the separator
        is found.
        """
        if not separator:
            raise ValueError("separator must not be empty")
        while True:
            idx = self._buffer.find(separator)
            if idx >= 0:
                end = idx + len(separator)
                data = bytes(self._buffer[:end])
                del self._buffer[:end]
                return data
            if self._eof:
                break
            await self._fill_buffer(n=len(self._buffer) + 1)
        # EOF reached — return whatever remains (matching StreamReader).
        raise asyncio.IncompleteReadError(bytes(self._buffer), -1)

    async def readline(self) -> bytes:
        """Read until a newline (``\\n``) or EOF."""
        return await self.readuntil(b"\n")

    def at_eof(self) -> bool:
        """Return ``True`` if the stream is at EOF and the buffer is empty."""
        return self._eof and not self._buffer

    async def __aiter__(self) -> "CompatibleStreamReader":
        return self

    async def __anext__(self) -> bytes:
        line = await self.readline()
        if not line:
            raise StopAsyncIteration
        return line


class CompatibleStreamWriter:
    """Asyncio ``StreamWriter``-like wrapper over an :class:`AsyncOutboundStream`.

    Buffering semantics:

    - :meth:`write` appends to an internal buffer.
    - :meth:`drain` flushes the buffer to the underlying transport.
    - :meth:`write_eof` marks the write side as closed; subsequent
      :meth:`write` calls raise :exc:`ValueError`.

    All close and drain operations are idempotent.
    """

    __slots__ = (
        "_stream",
        "_reader",
        "_host",
        "_port",
        "_write_buf",
        "_closing",
        "_write_eof_called",
    )

    def __init__(
        self,
        stream: AsyncOutboundStream,
        reader: CompatibleStreamReader,
        host: str,
        port: int,
    ) -> None:
        self._stream = stream
        self._reader = reader
        self._host = host
        self._port = port
        self._write_buf = bytearray()
        self._closing = False
        self._write_eof_called = False

    # ------------------------------------------------------------------
    # Public API (asyncio.StreamWriter-like)
    # ------------------------------------------------------------------

    def write(self, data: bytes) -> None:
        """Buffer *data* for sending.  Does not block."""
        if self._write_eof_called:
            raise ValueError("write_eof has been called")
        if self._closing:
            raise ValueError("StreamWriter is closing")
        self._write_buf.extend(data)

    def writelines(self, lines: list[bytes]) -> None:
        """Write a list of bytes to the write buffer."""
        for line in lines:
            self.write(line)

    async def drain(self) -> None:
        """Flush the write buffer to the underlying transport."""
        if not self._write_buf:
            return
        data = bytes(self._write_buf)
        self._write_buf.clear()
        loop = asyncio.get_running_loop()
        await loop.run_in_executor(None, self._stream.write, data)
        await loop.run_in_executor(None, self._stream.drain)

    def can_write_eof(self) -> bool:
        """Return ``True`` — this adapter supports half-close."""
        return True

    async def write_eof(self) -> None:
        """Signal the write side is done (half-close).

        Any data still in the write buffer is flushed before the EOF
        is sent.
        """
        if self._write_eof_called:
            return
        await self.drain()
        self._write_eof_called = True
        await self._stream.write_eof()

    def close(self) -> None:
        """Close the underlying transport.

        Idempotent — safe to call multiple times.
        """
        if self._closing:
            return
        self._closing = True
        self._stream.close()

    async def wait_closed(self) -> None:
        """Wait for the close handshake to complete.

        Idempotent — returns immediately if already closed.
        """
        if not self._closing:
            return
        await self._stream.wait_closed()

    def is_closing(self) -> bool:
        """Return ``True`` if the stream is closing or closed."""
        return self._closing or self._stream.is_closing()

    def get_extra_info(self, key: str, default: Any = None) -> Any:
        """Return transport metadata.

        Common keys: ``'peername'``, ``'sockname'``, ``'peername'``.
        Falls back to the underlying stream's ``get_extra_info``, then
        to locally stored host/port tuples, then to *default*.
        """
        value = self._stream.get_extra_info(key)
        if value is not None:
            return value
        if key == "peername":
            return (self._host, self._port)
        if key == "sockname":
            return self._stream.get_extra_info("sockname", default)
        return default

    async def __aenter__(self) -> "CompatibleStreamWriter":
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: Any,
    ) -> None:
        self.close()
        await self.wait_closed()

    def __del__(self) -> None:
        if not self._closing:
            import warnings

            warnings.warn(
                "CompatibleStreamWriter was not properly closed; "
                "use 'async with' or call close()/wait_closed().",
                ResourceWarning,
                stacklevel=2,
            )
            try:
                self.close()
            except Exception:
                pass

    def __repr__(self) -> str:
        state = "closing" if self._closing else "open"
        return (
            f"CompatibleStreamWriter(state='{state}', "
            f"peer=({self._host!r}, {self._port}))"
        )


async def open_tcp_connection(
    host: str,
    port: int,
    timeout: float = 10.0,
    **kwargs: Any,
) -> tuple[CompatibleStreamReader, CompatibleStreamWriter]:
    """Open an outbound TCP connection and return ``(reader, writer)``.

    This is the core adapter function that bridges
    :class:`~eggress.outbound.OutboundConnector` into the
    ``asyncio.StreamReader`` / ``asyncio.StreamWriter`` contract.

    Args:
        host: Target hostname or IP address.
        port: Target port.
        timeout: Connection timeout in seconds.
        **kwargs: Additional keyword arguments are reserved for future use.

    Returns:
        A ``(reader, writer)`` tuple.

    Raises:
        ConnectionError: If the connection fails.
        OSError: If the transport cannot be opened.
    """
    if kwargs:
        unexpected = ", ".join(sorted(kwargs))
        raise TypeError(f"unexpected keyword argument(s): {unexpected}")

    connector = OutboundConnector.from_pproxy_uri("direct://")
    stream = await connector.aconnect_tcp(host, port, timeout)
    reader = CompatibleStreamReader(stream)
    writer = CompatibleStreamWriter(stream, reader, host, port)
    return reader, writer
