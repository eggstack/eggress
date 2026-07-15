"""pproxy-compatible Connection facade.

This module provides :class:`ProxyConnection`, a pproxy-compatible interface
for opening outbound TCP connections through a proxy chain.

.. note::

   **Architectural difference from pproxy**: pproxy's ``Connection`` is a
   lightweight factory that directly performs protocol handshakes with the
   upstream proxy. eggress's ``ProxyConnection`` starts a minimal local
   listener service and connects through it using the standard proxy
   protocol (SOCKS5 or HTTP CONNECT). This adds minimal overhead (one
   local TCP round-trip) but ensures full compatibility with all proxy
   chain configurations supported by eggress.

Usage::

    from eggress.pproxy_connection import ProxyConnection

    # Create a connection factory through a SOCKS5 proxy
    conn = ProxyConnection("socks5://user:pass@proxy:1080")

    # Open an outbound TCP connection through the proxy
    sock = conn.tcp_connect("example.com", 443)
    sock.sendall(b"GET / HTTP/1.1\\r\\nHost: example.com\\r\\n\\r\\n")
    data = sock.recv(4096)
    sock.close()

    # Or use as a context manager
    with ProxyConnection("http://proxy:8080") as conn:
        sock = conn.tcp_connect("example.com", 80)
        # ...
        sock.close()

    # Clean up the underlying service
    conn.close()
"""

from __future__ import annotations

import socket
import warnings
from typing import Any, Optional, Sequence


class ProxyConnection:
    """pproxy-compatible outbound connection factory.

    Accepts pproxy-style URI strings and provides :meth:`tcp_connect` for
    opening outbound TCP connections through the configured proxy chain.

    Unlike pproxy's ``Connection``, which directly performs protocol
    handshakes, this implementation starts a minimal local listener and
    connects through it. The semantic outcome is identical: data flows
    through the configured proxy chain to the target.

    Args:
        *uris: pproxy-style URI strings (e.g. ``"socks5://proxy:1080"``).
            The first URI is the local listener; subsequent URIs are
            upstream proxies.

    Example::

        conn = ProxyConnection("socks5://proxy:1080")
        sock = conn.tcp_connect("example.com", 443)
        # use sock as a regular socket
        sock.close()
        conn.close()
    """

    __slots__ = ("_service", "_handle", "_closed", "_uris")

    def __init__(self, *uris: str, **kwargs: Any) -> None:
        if not uris:
            raise ValueError("at least one URI argument is required")

        self._uris = uris
        self._handle = None
        self._closed = False
        self._service = None

    def _ensure_started(self) -> Any:
        """Lazily start the underlying proxy service."""
        if self._handle is not None:
            return self._handle

        from eggress._eggress import UnsupportedFeatureError, EggressError
        from eggress.pproxy import translate_pproxy_args, PPProxyService

        # Build pproxy-style args from the URIs.
        # First URI is the local listener, rest are upstreams.
        args: list[str] = []
        if self._uris:
            args.extend(["-l", self._uris[0]])
        for uri in self._uris[1:]:
            args.extend(["-r", uri])

        result = translate_pproxy_args(args)
        if not result.ok:
            features = ", ".join(
                f"{u.feature}: {u.message}" for u in result.unsupported
            )
            raise UnsupportedFeatureError(
                f"unsupported pproxy features: {features}"
            )

        self._service = PPProxyService.from_args(args)
        self._handle = self._service.start()
        return self._handle

    @property
    def closed(self) -> bool:
        """Whether this connection has been closed."""
        return self._closed

    @property
    def addresses(self) -> dict[str, str]:
        """Bound listener addresses. Empty dict if not started."""
        if self._handle is None:
            return {}
        return self._handle.bound_addresses

    @property
    def config(self) -> str:
        """The translated eggress TOML configuration."""
        if self._service is not None:
            return self._service.config.to_toml()
        return ""

    def tcp_connect(self, host: str, port: int, timeout: float = 10.0) -> socket.socket:
        """Open a TCP connection to ``host:port`` through the proxy chain.

        Returns a connected :class:`socket.socket` object. The socket is
        connected to the target through the proxy chain configured at
        construction time.

        Args:
            host: Target hostname or IP address.
            port: Target port number.
            timeout: Connection timeout in seconds (default 10).

        Returns:
            A connected TCP socket.

        Raises:
            ConnectionError: If the connection fails.
            RuntimeError: If the service is closed.
        """
        if self._closed:
            raise RuntimeError("ProxyConnection is closed")

        handle = self._ensure_started()
        addrs = handle.bound_addresses
        if not addrs:
            raise RuntimeError("service not started or no listeners bound")

        # Get the first listener address (our local SOCKS5/HTTP proxy)
        listener_addr_str = None
        for name, addr in addrs.items():
            if not name.startswith("_"):
                listener_addr_str = addr
                break

        if listener_addr_str is None:
            raise RuntimeError("no listener address found")

        # Parse "host:port" string to (host, port) tuple
        host_part, port_part = listener_addr_str.rsplit(":", 1)
        # Strip IPv6 brackets
        host_part = host_part.strip("[]")
        listener_addr = (host_part, int(port_part))

        # Determine the listener protocol from the config
        listener_info = handle.status().get("listeners", [])
        protocol = "socks5"
        if listener_info:
            protos = listener_info[0].get("protocols", [])
            if "http" in protos:
                protocol = "http"
            elif "socks4" in protos:
                protocol = "socks4"

        # Connect to the local listener and perform the proxy handshake
        sock = socket.create_connection(listener_addr, timeout=timeout)

        try:
            if protocol == "socks5":
                self._socks5_handshake(sock, host, port)
            elif protocol == "socks4":
                self._socks4_handshake(sock, host, port)
            elif protocol == "http":
                self._http_connect_handshake(sock, host, port)
            else:
                raise ConnectionError(f"unsupported listener protocol: {protocol}")
        except Exception:
            sock.close()
            raise

        return sock

    def _socks5_handshake(
        self, sock: socket.socket, host: str, port: int
    ) -> None:
        """Perform SOCKS5 handshake to connect through the local proxy."""
        # Greeting: version 5, 1 auth method (no auth)
        sock.sendall(b"\x05\x01\x00")

        # Read greeting response
        resp = sock.recv(2)
        if len(resp) < 2 or resp[0] != 0x05 or resp[1] != 0x00:
            raise ConnectionError(
                f"SOCKS5 greeting failed: {resp.hex() if resp else 'no response'}"
            )

        # Connect request: version 5, cmd connect (0x01), reserved (0x00)
        # Determine address type: IPv4 (0x01), IPv6 (0x04), or domain (0x03)
        import ipaddress as _ipaddress

        try:
            ip = _ipaddress.ip_address(host)
            if isinstance(ip, _ipaddress.IPv4Address):
                addr_type = b"\x01"
                addr_data = ip.packed
            else:
                addr_type = b"\x04"
                addr_data = ip.packed
        except ValueError:
            # Not an IP address, treat as domain name
            addr_type = b"\x03"
            host_bytes = host.encode("ascii")
            addr_data = bytes([len(host_bytes)]) + host_bytes

        req = b"\x05\x01\x00" + addr_type + addr_data
        req += port.to_bytes(2, "big")
        sock.sendall(req)

        # Read reply (at least 10 bytes for IPv4 reply)
        reply = sock.recv(1024)
        if len(reply) < 10:
            raise ConnectionError(
                f"SOCKS5 connect reply too short: {len(reply)} bytes"
            )
        if reply[1] != 0x00:
            raise ConnectionError(
                f"SOCKS5 connect failed with code {reply[1]:#04x}"
            )

    def _socks4_handshake(
        self, sock: socket.socket, host: str, port: int
    ) -> None:
        """Perform SOCKS4 handshake to connect through the local proxy."""
        import ipaddress

        # SOCKS4 requires an IPv4 address; resolve the hostname
        try:
            ip = ipaddress.IPv4Address(host)
        except ValueError:
            raise ConnectionError(
                f"SOCKS4 does not support domain names; got: {host}"
            )

        ip_bytes = ip.packed
        req = b"\x04\x01" + port.to_bytes(2, "big") + ip_bytes + b"\x00"
        sock.sendall(req)

        reply = sock.recv(1024)
        if len(reply) < 8:
            raise ConnectionError("SOCKS4 reply too short")
        if reply[1] != 0x5A:
            raise ConnectionError(
                f"SOCKS4 connect failed with code {reply[1]:#04x}"
            )

    def _http_connect_handshake(
        self, sock: socket.socket, host: str, port: int
    ) -> None:
        """Perform HTTP CONNECT handshake to connect through the local proxy."""
        connect_req = f"CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n\r\n"
        sock.sendall(connect_req.encode("ascii"))

        # Read the full response (may need multiple reads)
        response = b""
        while b"\r\n\r\n" not in response:
            chunk = sock.recv(4096)
            if not chunk:
                raise ConnectionError("HTTP CONNECT: connection closed by proxy")
            response += chunk

        # Check for 200 OK
        status_line = response.split(b"\r\n", 1)[0]
        if b"200" not in status_line:
            raise ConnectionError(
                f"HTTP CONNECT failed: {status_line.decode('ascii', errors='replace')}"
            )

    def close(self) -> None:
        """Stop the underlying proxy service. Idempotent."""
        if self._closed:
            return
        self._closed = True
        if self._handle is not None:
            try:
                self._handle.shutdown()
            except Exception:
                pass
            self._handle = None
        self._service = None

    def __enter__(self) -> ProxyConnection:
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: Any,
    ) -> bool:
        self.close()
        return False

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            warnings.warn(
                "ProxyConnection was not properly closed. "
                "Use 'with' statement or call close().",
                ResourceWarning,
                stacklevel=2,
            )
            self.close()

    def __repr__(self) -> str:
        state = "closed" if self._closed else "open"
        uris = ", ".join(self._uris[:2]) if self._uris else ""
        if len(self._uris) > 2:
            uris += f", ... ({len(self._uris)} total)"
        return f"ProxyConnection(state='{state}', uris=[{uris}])"

    def __bool__(self) -> bool:
        return not self._closed
