"""Native outbound connector for proxy chains.

Provides :class:`OutboundConnector`, which compiles a TOML config or
pproxy-style URI and opens native outbound streams without starting a
listener service.

Usage::

    from eggress.outbound import OutboundConnector

    # From a pproxy-style URI
    conn = OutboundConnector.from_pproxy_uri("socks5://proxy:1080")
    print(conn.upstream_count())  # 1

    # From TOML config
    conn = OutboundConnector.from_toml('''
        version = 1
        [[listeners]]
        name = "test"
        bind = "127.0.0.1:0"
        protocols = ["socks5"]
        [[upstreams]]
        id = "up"
        uri = "socks5://127.0.0.1:1080"
    ''')

    stream = conn.connect_tcp("example.com", 443, timeout=10)
    stream.sendall(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")
    response = stream.recv(4096)
    stream.close()

    # Validate a config
    hops = OutboundConnector.validate_config(toml_str)
"""

from __future__ import annotations

import asyncio
import warnings
from typing import Any

from eggress._eggress import (
    PyOutboundConnector as _PyOutboundConnector,
    PyOutboundStream as _PyOutboundStream,
)


class OutboundStream:
    """Synchronous native stream returned by :meth:`connect_tcp`.

    The stream is not a file descriptor and is intentionally not a
    ``socket.socket`` instance: advanced transports may be TLS, WebSocket, or
    HTTP/2 streams. ``sendall`` and ``recv`` aliases keep common pproxy code
    source-compatible while ``read`` and ``write`` are the canonical methods.
    """

    __slots__ = ("_inner",)

    def __init__(self, inner: _PyOutboundStream) -> None:
        self._inner = inner

    @property
    def closed(self) -> bool:
        return self._inner.closed

    def is_closing(self) -> bool:
        return self._inner.is_closing()

    @property
    def peername(self) -> str | None:
        return self._inner.peername

    @property
    def sockname(self) -> str | None:
        return self._inner.sockname

    def get_extra_info(self, name: str, default: Any = None) -> Any:
        return self._inner.get_extra_info(name, default)

    def read(self, n: int = -1) -> bytes:
        return bytes(self._inner.read(n))

    def recv(self, n: int = 4096) -> bytes:
        return self.read(n)

    def readexactly(self, n: int) -> bytes:
        return bytes(self._inner.readexactly(n))

    def write(self, data: bytes) -> int:
        return self._inner.write(data)

    def sendall(self, data: bytes) -> None:
        self._inner.sendall(data)

    def drain(self) -> None:
        self._inner.drain()

    def write_eof(self) -> None:
        self._inner.write_eof()

    def close(self) -> None:
        self._inner.close()

    def wait_closed(self) -> None:
        self._inner.wait_closed()

    def __enter__(self) -> OutboundStream:
        return self

    def __exit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> bool:
        self.close()
        return False

    def __del__(self) -> None:
        if not getattr(self, "closed", True):
            warnings.warn(
                "OutboundStream was not properly closed; cleaning up",
                ResourceWarning,
                stacklevel=2,
            )
            try:
                self.close()
            except Exception:
                pass

    def __repr__(self) -> str:
        return repr(self._inner)


class AsyncOutboundStream:
    """Asyncio adapter for a native outbound stream.

    Network operations run in a worker thread and the event-loop thread is
    never blocked. The underlying Rust stream remains the owner of the
    transport and can be closed deterministically.
    """

    __slots__ = ("_inner",)

    def __init__(self, inner: _PyOutboundStream) -> None:
        self._inner = inner

    @property
    def closed(self) -> bool:
        return self._inner.closed

    def is_closing(self) -> bool:
        return self._inner.is_closing()

    @property
    def peername(self) -> str | None:
        return self._inner.peername

    @property
    def sockname(self) -> str | None:
        return self._inner.sockname

    def get_extra_info(self, name: str, default: Any = None) -> Any:
        return self._inner.get_extra_info(name, default)

    async def read(self, n: int = -1) -> bytes:
        loop = asyncio.get_running_loop()
        return bytes(await loop.run_in_executor(None, self._inner.read, n))

    async def readexactly(self, n: int) -> bytes:
        loop = asyncio.get_running_loop()
        return bytes(await loop.run_in_executor(None, self._inner.readexactly, n))

    def write(self, data: bytes) -> int:
        return self._inner.write(data)

    async def drain(self) -> None:
        loop = asyncio.get_running_loop()
        await loop.run_in_executor(None, self._inner.drain)

    async def write_eof(self) -> None:
        loop = asyncio.get_running_loop()
        await loop.run_in_executor(None, self._inner.write_eof)

    def close(self) -> None:
        self._inner.close()

    async def wait_closed(self) -> None:
        loop = asyncio.get_running_loop()
        await loop.run_in_executor(None, self._inner.wait_closed)

    async def __aenter__(self) -> AsyncOutboundStream:
        return self

    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> bool:
        self.close()
        return False

    def __del__(self) -> None:
        if not getattr(self, "closed", True):
            warnings.warn(
                "AsyncOutboundStream was not properly closed; cleaning up",
                ResourceWarning,
                stacklevel=2,
            )
            try:
                self.close()
            except Exception:
                pass

    def __repr__(self) -> str:
        return repr(self._inner)


class OutboundConnector:
    """Native outbound connector for proxy chains.

    Compiles routing/upstream state from a TOML config or pproxy-style URI
    and validates that the chain is usable for outbound connections, without
    starting a listener service.

    Use the ``from_pproxy_uri`` or ``from_toml`` factory methods to create
    an instance.  Call :meth:`validate_config` for a quick validation
    without creating a full connector.
    """

    __slots__ = ("_inner",)

    def __init__(self, inner: _PyOutboundConnector) -> None:
        self._inner = inner

    @staticmethod
    def from_pproxy_uri(uri: str) -> OutboundConnector:
        """Create a connector from a pproxy-style URI.

        Args:
            uri: A pproxy-style URI (e.g. ``"socks5://proxy:1080"``).

        Returns:
            A new :class:`OutboundConnector`.

        Raises:
            ConnectionError: If the URI cannot be translated to a valid config.
        """
        inner = _PyOutboundConnector.from_pproxy_uri(uri)
        return OutboundConnector(inner)

    @staticmethod
    def from_toml(config_toml: str) -> OutboundConnector:
        """Create a connector from a TOML configuration string.

        Args:
            config_toml: A TOML configuration string with at least one
                upstream defined.

        Returns:
            A new :class:`OutboundConnector`.

        Raises:
            ConnectionError: If the config is invalid or has no upstreams.
        """
        inner = _PyOutboundConnector.from_toml(config_toml)
        return OutboundConnector(inner)

    @staticmethod
    def validate_config(config_toml: str) -> int:
        """Validate a TOML config for outbound connections.

        Returns the number of hops in the first upstream's chain, or raises
        if the config is invalid.

        Args:
            config_toml: A TOML configuration string.

        Returns:
            The number of hops in the first upstream's chain.

        Raises:
            ConnectionError: If the config is invalid.
        """
        return _PyOutboundConnector.validate_config(config_toml)

    @property
    def upstream_count(self) -> int:
        """Number of upstreams configured."""
        return self._inner.upstream_count()

    def preview_connect(self, host: str, port: int) -> dict[str, Any]:
        """Preview connection metadata without actually connecting.

        Args:
            host: Target hostname or IP.
            port: Target port.

        Returns:
            A dict with ``target_host``, ``target_port``, and ``hop_count``.
        """
        return self._inner.preview_connect(host, port)

    def connect_tcp(
        self, host: str, port: int, timeout: float | None = None
    ) -> OutboundStream:
        """Open a native TCP stream through the configured chain."""
        return OutboundStream(self._inner.connect_tcp(host, port, timeout))

    async def aconnect_tcp(
        self, host: str, port: int, timeout: float | None = None
    ) -> AsyncOutboundStream:
        """Open a native TCP stream without blocking the event-loop thread."""
        loop = asyncio.get_running_loop()
        inner = await loop.run_in_executor(
            None, self._inner.connect_tcp, host, port, timeout
        )
        return AsyncOutboundStream(inner)

    def __repr__(self) -> str:
        return f"OutboundConnector(upstreams={self.upstream_count})"
