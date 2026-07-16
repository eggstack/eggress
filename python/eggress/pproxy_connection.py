"""Native pproxy-compatible outbound connection facade.

``ProxyConnection`` is retained as the familiar compatibility name, but its
implementation is now a thin wrapper over :class:`eggress.OutboundConnector`.
It never starts a listener or opens a temporary local proxy socket.
"""

from __future__ import annotations

import warnings
from typing import Any

from eggress.outbound import AsyncOutboundStream, OutboundConnector, OutboundStream


class ProxyConnection:
    """Open outbound TCP streams through a pproxy-style chain.

    Each positional URI is an upstream hop. The hops are composed into one
    native chain. For example::

        conn = ProxyConnection("socks5://proxy:1080")
        stream = conn.tcp_connect("example.com", 443)
        stream.sendall(b"hello")
        stream.close()
        conn.close()

    ``tcp_connect`` returns an :class:`~eggress.outbound.OutboundStream`, not
    a ``socket.socket``. This is necessary for TLS, WebSocket, H2, and
    multi-hop transports whose connected stream has no stable file descriptor.
    The stream provides ``sendall``/``recv`` aliases for common synchronous
    pproxy programs.
    """

    __slots__ = ("_connector", "_closed", "_uris", "_streams", "_config_toml")

    def __init__(self, *uris: str, **kwargs: Any) -> None:
        if not uris:
            raise ValueError("at least one URI argument is required")
        if kwargs:
            unexpected = ", ".join(sorted(kwargs))
            raise TypeError(f"unexpected keyword argument(s): {unexpected}")

        self._uris = tuple(uris)
        self._closed = False
        self._streams: set[OutboundStream | AsyncOutboundStream] = set()

        if len(uris) == 1 and uris[0].lower().startswith("direct://"):
            self._config_toml = uris[0]
            self._connector = OutboundConnector.from_pproxy_uri(uris[0])
            return

        # The translation layer already owns pproxy chain parsing and
        # redaction. Build an upstream-only config from the supplied hops.
        from eggress.pproxy import translate_pproxy_args

        args = ["-r", "__".join(uris)]
        result = translate_pproxy_args(args)
        if not result.ok:
            details = "; ".join(
                f"{item.feature}: {item.message}" for item in result.unsupported
            )
            raise ValueError(f"unsupported outbound chain: {details}")
        self._config_toml = result.toml
        self._connector = OutboundConnector.from_toml(result.toml)

    @property
    def closed(self) -> bool:
        """Whether this connection factory has been closed."""
        return self._closed

    @property
    def addresses(self) -> dict[str, str]:
        """Return no listener addresses; native connections bind on demand."""
        return {}

    @property
    def config(self) -> str:
        """The translated, credential-bearing TOML configuration.

        Callers should use the redacted configuration APIs for logs.
        """
        return self._config_toml

    def tcp_connect(
        self, host: str, port: int, timeout: float | None = 10.0
    ) -> OutboundStream:
        """Open a native outbound stream to ``host:port``."""
        if self._closed:
            raise RuntimeError("ProxyConnection is closed")
        stream = self._connector.connect_tcp(host, port, timeout)
        self._streams.add(stream)
        return stream

    async def atcp_connect(
        self, host: str, port: int, timeout: float | None = 10.0
    ) -> AsyncOutboundStream:
        """Open a native outbound stream without blocking the event loop."""
        if self._closed:
            raise RuntimeError("ProxyConnection is closed")
        stream = await self._connector.aconnect_tcp(host, port, timeout)
        self._streams.add(stream)
        return stream

    def close(self) -> None:
        """Close all streams and release the connector."""
        if self._closed:
            return
        self._closed = True
        for stream in tuple(self._streams):
            try:
                stream.close()
            except Exception:
                pass
        self._streams.clear()

    def __enter__(self) -> ProxyConnection:
        return self

    def __exit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> bool:
        self.close()
        return False

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            warnings.warn(
                "ProxyConnection was not properly closed. Use 'with' or call close().",
                ResourceWarning,
                stacklevel=2,
            )
            try:
                self.close()
            except Exception:
                pass

    def __repr__(self) -> str:
        state = "closed" if self._closed else "open"
        redacted = ", ".join(uri.split("@")[-1] for uri in self._uris)
        return f"ProxyConnection(state='{state}', uris=[{redacted}])"

    def __bool__(self) -> bool:
        return not self._closed
