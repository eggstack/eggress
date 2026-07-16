"""Native outbound connector for proxy chains.

Provides :class:`OutboundConnector`, which compiles a TOML config or
pproxy-style URI and validates outbound chain configuration without
starting a listener service.

.. note::

   The connector currently exposes configuration validation and chain
   introspection.  Direct ``connect_tcp`` from Python is not yet
   available — use :class:`~eggress.pproxy_connection.ProxyConnection`
   for outbound TCP connections through a proxy chain.

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

    # Validate a config
    hops = OutboundConnector.validate_config(toml_str)
"""

from __future__ import annotations

from typing import Any

from eggress._eggress import (
    PyOutboundConnector as _PyOutboundConnector,
    ConnectionError as _ConnectionError,
)


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

    def __repr__(self) -> str:
        return f"OutboundConnector(upstreams={self.upstream_count})"
