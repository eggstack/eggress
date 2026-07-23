"""pproxy server object model matching pproxy 2.7.9's ``pproxy.server`` classes.

Provides ``ProxyDirect``, ``ProxySimple``, ``ProxyBackward``, ``ProxyH2``,
``ProxySSH``, ``ProxyQUIC``, ``ProxyH3``, and ``AuthTable`` — the proxy
handler hierarchy used by pproxy's server runtime.

These classes carry typed metadata for composition resolution without
reimplementing wire-level protocol handling.  Async methods that require
runtime integration raise :class:`NotImplementedError` at this layer.
"""

from __future__ import annotations

from typing import Any, Callable, Optional, Sequence, Tuple

# Reverse mapping from protocol class to pproxy scheme name.
# Used by start_server to build listen URIs from stored protocol classes.
_SCHEME_BY_CLASS: dict[type, str] = {}

# Canonical scheme names preferred when a class maps to multiple schemes.
_PREFERRED_SCHEMES = {"socks5", "socks4", "http", "ss", "trojan", "h2", "h3"}


def _proto_to_scheme(proto_cls: Any) -> str:
    """Convert a protocol class to its pproxy scheme name."""
    if not _SCHEME_BY_CLASS:
        from eggress.protocol import _PROTOCOL_REGISTRY
        for scheme, cls in _PROTOCOL_REGISTRY.items():
            if cls not in _SCHEME_BY_CLASS:
                _SCHEME_BY_CLASS[cls] = scheme
            elif scheme in _PREFERRED_SCHEMES:
                _SCHEME_BY_CLASS[cls] = scheme
    return _SCHEME_BY_CLASS.get(proto_cls, "http")


# ---------------------------------------------------------------------------
# AuthTable
# ---------------------------------------------------------------------------


class AuthTable:
    """pproxy-compatible authentication state table.

    State is shared across instances for the same remote_ip, matching
    pproxy 2.7.9 behavior. Different IPs are isolated.

    Args:
        remote_ip: IP address of the remote client.
        authtime: Timeout in seconds for the auth entry.
    """

    # Class-level shared state: {remote_ip: {"user": ..., "auth_time": ...}}
    _shared_state: dict[str, dict[str, Any]] = {}

    def __init__(self, remote_ip: str | None = None, authtime: int | None = None) -> None:
        self.remote_ip = remote_ip
        self.authtime = authtime
        # Use shared state for this IP
        if remote_ip is not None:
            if remote_ip not in AuthTable._shared_state:
                AuthTable._shared_state[remote_ip] = {"user": None, "auth_time": None}
            self._state = AuthTable._shared_state[remote_ip]
        else:
            # No IP: per-instance state (matches oracle behavior)
            self._state = {"user": None, "auth_time": None}

    def authed(self) -> Any:
        """Return the currently authenticated user, or ``None``."""
        user = self._state["user"]
        if user is None:
            return None
        if self.authtime is not None and self._state["auth_time"] is not None:
            import time
            if time.monotonic() - self._state["auth_time"] > self.authtime:
                self._state["user"] = None
                self._state["auth_time"] = None
                return None
        return user

    def set_authed(self, user: Any) -> None:
        """Mark *user* as authenticated."""
        import time
        self._state["user"] = user
        self._state["auth_time"] = time.monotonic()

    def clear(self) -> None:
        """Clear authentication state."""
        self._state["user"] = None
        self._state["auth_time"] = None

    def __bool__(self) -> bool:
        """Truthiness: True if any user is currently authenticated."""
        return self.authed() is not None

    def __contains__(self, item: Any) -> bool:
        """Membership test: ``user in auth_table``."""
        return self.authed() == item

    def __repr__(self) -> str:
        return f"<AuthTable remote_ip={self.remote_ip!r} authtime={self.authtime}>"

    @classmethod
    def _reset_shared(cls) -> None:
        """Reset all shared state. For testing only."""
        cls._shared_state.clear()


# ---------------------------------------------------------------------------
# ProxyDirect
# ---------------------------------------------------------------------------


class ProxyDirect:
    """pproxy-compatible direct proxy handler.

    Base class in the pproxy server proxy hierarchy.  ``ProxyDirect``
    instances represent direct connections (no upstream proxy).  Each
    instance is unique — ``==`` returns ``False`` for distinct instances,
    matching pproxy 2.7.9 behavior.

    Args:
        lbind: Optional local bind address override.
    """

    def __init__(self, lbind: str | None = None) -> None:
        self._lbind = lbind
        self._bind: str | None = None
        self._host_name: str | None = None
        self._port: int | None = None
        self._unix: str | None = None
        self._alive: int = 0
        self._connections: int = 0
        self.udpmap: dict = {}

    # -- properties ---------------------------------------------------------

    @property
    def direct(self) -> bool:
        """``True`` — direct connections bypass upstream proxies."""
        return True

    @property
    def bind(self) -> str:
        return self._bind if self._bind is not None else "DIRECT"

    @property
    def lbind(self) -> str | None:
        return self._lbind

    @property
    def unix(self) -> str | None:
        return self._unix

    @property
    def alive(self) -> bool:
        return True

    @property
    def connections(self) -> int:
        return self._connections

    @property
    def rproto(self) -> Any:
        return None

    @property
    def auth(self) -> Any:
        return None

    @property
    def jump(self) -> Any:
        return getattr(self, "_jump", None)

    # -- connection lifecycle -----------------------------------------------

    def connection_change(self, delta: int) -> None:
        """Update the connection count by *delta*."""
        self._connections += delta

    def destination(self, host: str, port: int) -> Tuple[str, int]:
        """Return the effective destination for *host*/*port*."""
        return (host, port)

    def logtext(self, host: str, port: int) -> str:
        """Return a log-friendly description of the connection target."""
        if host == "tunnel":
            return ""
        return f" -> {host}:{port}"

    def match_rule(self, host: str, port: int) -> bool:
        """Always matches for direct connections."""
        return True

    async def open_connection(
        self,
        host: str,
        port: int,
        local_addr: Any,
        lbind: str | None,
        timeout: float = 60,
    ) -> Any:
        """Open a TCP connection through this proxy."""
        return await self.tcp_connect(host, port, local_addr=local_addr, lbind=lbind)

    async def prepare_connection(
        self,
        reader_remote: Any,
        writer_remote: Any,
        host: str,
        port: int,
    ) -> Any:
        """Prepare a connection after the TCP handshake.

        Default implementation is a passthrough (matches pproxy oracle).
        Subclasses may override for protocol-specific preparation.
        """
        return reader_remote, writer_remote

    async def tcp_connect(
        self,
        host: str,
        port: int,
        local_addr: str | None = None,
        lbind: str | None = None,
    ) -> Any:
        """Open a direct TCP connection."""
        import asyncio
        reader, writer = await asyncio.open_connection(host, port, local_addr=local_addr)
        return reader, writer

    async def udp_open_connection(
        self,
        host: str,
        port: int,
        data: bytes,
        addr: Any,
        reply: Any,
    ) -> Any:
        """Open a UDP association through this proxy.

        Direct implementation: sends data via asyncio datagram endpoint.
        """
        import asyncio
        import time

        udpmap = getattr(self, "udpmap", {})
        if addr in udpmap:
            prot = udpmap[addr]
            if prot.transport:
                prot.transport.sendto(data)
            return

        class _UdpProto(asyncio.DatagramProtocol):
            def __init__(prot, data):
                udpmap[addr] = prot
                prot.databuf = [data]
                prot.transport = None
                prot.update = 0

            def connection_made(prot, transport):
                prot.transport = transport
                for d in prot.databuf:
                    transport.sendto(d)
                prot.databuf.clear()
                prot.update = time.perf_counter()

            def datagram_received(prot, d, addr_recv):
                d = self.udp_packet_unpack(d)
                reply(d)
                prot.update = time.perf_counter()

            def connection_lost(prot, exc):
                udpmap.pop(addr, None)

        self.connection_change(1)
        limit = getattr(self, "UDP_LIMIT", 30)
        if len(udpmap) > limit:
            min_addr = min(udpmap, key=lambda x: udpmap[x].update)
            old_prot = udpmap.pop(min_addr)
            if old_prot.transport:
                old_prot.transport.close()

        remote = self.destination(host, port)
        loop = asyncio.get_event_loop()
        await loop.create_datagram_endpoint(
            lambda: _UdpProto(data), remote_addr=remote
        )

    def udp_packet_unpack(self, data: bytes) -> bytes:
        """Unpack a UDP datagram payload.  Identity by default."""
        return data

    def udp_prepare_connection(self, host: str, port: int, data: bytes) -> bytes:
        """Prepare a UDP packet for sending.  Returns *data* unchanged."""
        return data

    async def udp_sendto(
        self,
        host: str,
        port: int,
        data: bytes,
        answer_cb: Callable[..., Any],
        local_addr: str | None = None,
    ) -> Any:
        """Send UDP data through this proxy.

        Matches pproxy oracle: prepare packet, then open connection.
        """
        import random
        if local_addr is None:
            local_addr = random.randrange(2**32)
        data = self.udp_prepare_connection(host, port, data)
        await self.udp_open_connection(host, port, data, local_addr, answer_cb)

    def wait_open_connection(
        self,
        host: str,
        port: int,
        local_addr: Any,
        family: int,
    ) -> Any:
        """Wait for an existing connection to (host, port).

        Synchronous by default; returns ``None`` when no cached connection
        exists.
        """
        return None

    # -- identity -----------------------------------------------------------

    def __eq__(self, other: object) -> bool:
        """Different ``ProxyDirect`` instances are never equal, matching
        pproxy 2.7.9 behavior."""
        return self is other

    def __hash__(self) -> int:
        return id(self)

    def __repr__(self) -> str:
        return f"<{type(self).__name__} lbind={self._lbind!r}>"


# ---------------------------------------------------------------------------
# ProxySimple
# ---------------------------------------------------------------------------


class ProxySimple(ProxyDirect):
    """pproxy-compatible simple proxy handler.

    Extends ``ProxyDirect`` with upstream proxy metadata (protocol chain,
    cipher, users, SSL context, etc.).  ``direct`` returns ``False``.

    Args:
        jump: Upstream proxy URI or ``None`` for direct.
        protos: Protocol chain list.
        cipher: Cipher specification.
        users: User authentication list.
        rule: Routing rule.
        bind: Bind address.
        host_name: Target hostname.
        port: Target port.
        unix: Unix domain socket path.
        lbind: Local bind address override.
        sslclient: Client-side SSL context.
        sslserver: Server-side SSL context.
    """

    def __init__(
        self,
        jump: Any = None,
        protos: Sequence[Any] = (),
        cipher: Any = None,
        users: Any = None,
        rule: Any = None,
        bind: str | None = None,
        host_name: str | None = None,
        port: int | None = None,
        unix: str | None = None,
        lbind: str | None = None,
        sslclient: Any = None,
        sslserver: Any = None,
    ) -> None:
        super().__init__(lbind=lbind)
        self._jump = jump
        self._protos = tuple(protos)
        self._cipher = cipher
        self._users = users
        # Compile rule string to callable (matching oracle behavior)
        if rule is not None:
            from pproxy.server import compile_rule
            self._rule = compile_rule(rule) if isinstance(rule, str) else rule
        else:
            self._rule = None
        self._bind = bind
        self._host_name = host_name
        self._port = port
        self._unix = unix
        self._sslclient = sslclient
        self._sslserver = sslserver

    # -- properties ---------------------------------------------------------

    @property
    def host_name(self) -> str | None:
        return self._host_name

    @property
    def port(self) -> int | None:
        return self._port

    @property
    def direct(self) -> bool:
        """``False`` — simple proxies route through an upstream."""
        return False

    @property
    def jump(self) -> Any:
        return self._jump

    @property
    def protos(self) -> Tuple[Any, ...]:
        return self._protos

    @property
    def cipher(self) -> Any:
        return self._cipher

    @property
    def users(self) -> Any:
        return self._users

    @property
    def rule(self) -> Any:
        return self._rule

    @property
    def sslclient(self) -> Any:
        return self._sslclient

    @property
    def sslserver(self) -> Any:
        return self._sslserver

    @property
    def rproto(self) -> Any:
        """Return the first protocol in the chain."""
        return self._protos[0] if self._protos else None

    @property
    def auth(self) -> bytes:
        """Return the first user credential or empty bytes."""
        return self._users[0] if self._users else b""

    # -- methods ------------------------------------------------------------

    def destination(self, host: str, port: int) -> Tuple[str | None, int | None]:
        return (self._host_name, self._port if self._port else None)

    def logtext(self, host: str, port: int) -> str:
        return f"{host}:{port}"

    def match_rule(self, host: str, port: int) -> Any:
        """Evaluate routing rules. Returns True if no rule is set."""
        if self._rule is None:
            return True
        return self._rule(host) or self._rule(str(port))

    async def tcp_connect(
        self,
        host: str,
        port: int,
        local_addr: str | None = None,
        lbind: str | None = None,
    ) -> Any:
        """Open a TCP connection through the upstream proxy.

        If no upstream is configured, connects directly (ProxyDirect behavior).
        """
        upstream_uri = self._build_remote_uri()
        if upstream_uri is None:
            return await super().tcp_connect(
                host, port, local_addr=local_addr, lbind=lbind
            )

        from eggress.outbound import OutboundConnector
        from eggress._asyncio_adapter import (
            CompatibleStreamReader,
            CompatibleStreamWriter,
        )

        connector = OutboundConnector.from_pproxy_uri(upstream_uri)
        stream = await connector.aconnect_tcp(host, port, timeout=60)
        reader = CompatibleStreamReader(stream)
        writer = CompatibleStreamWriter(stream, reader, host, port)
        return reader, writer

    def wait_open_connection(
        self,
        host: str,
        port: int,
        local_addr: Any,
        family: int,
    ) -> Any:
        return None

    # -- start_server -------------------------------------------------------

    async def start_server(
        self,
        args: Any = None,
        stream_handler: Callable[..., Any] | None = None,
    ) -> Any:
        """Start a server for this proxy configuration.

        Matches pproxy oracle: creates an asyncio server with the stream handler.

        Args:
            args: Dict of server arguments (ruport, verbose, etc.).
            stream_handler: Callable to handle incoming streams.

        Returns:
            An ``asyncio.Server`` handle with ``close()``, ``wait_closed()``,
            and ``sockets`` — matching the pproxy 2.7.9 contract.
        """
        import asyncio
        import functools

        if args is None:
            args = {}
        if stream_handler is None:
            from pproxy.server import stream_handler as default_handler
            stream_handler = default_handler

        # Build keyword arguments matching stream_handler's parameter names.
        # pproxy 2.7.9's ProxySimple stores these as public attributes
        # (unix, lbind, protos, cipher, sslserver), but we use underscore-
        # prefixed private attrs, so we must map explicitly.
        handler_kwargs: dict[str, Any] = {
            "unix": self._unix,
            "lbind": self._lbind,
            "protos": self._protos,
            "cipher": self._cipher,
            "sslserver": self._sslserver,
        }
        # Merge caller-provided args (rserver, debug, verbose, etc.).
        if isinstance(args, dict):
            handler_kwargs.update(args)

        handler = functools.partial(stream_handler, **handler_kwargs)
        if self._unix:
            return await asyncio.start_unix_server(handler, path=self._unix)
        else:
            host = self._host_name or "0.0.0.0"
            port = self._port or 0
            ruport = handler_kwargs.get("ruport") if isinstance(args, dict) else None
            return await asyncio.start_server(
                handler, host=host, port=port,
                reuse_port=ruport,
            )

    def _build_listen_uri(self) -> str:
        """Build a pproxy-style listen URI from this proxy's config."""
        proto_name = _proto_to_scheme(self._protos[0]) if self._protos else "http"
        host = self._host_name or "127.0.0.1"
        port = self._port or 0
        return f"{proto_name}://{host}:{port}"

    def _build_remote_uri(self) -> str | None:
        """Build a pproxy-style remote URI from the jump chain.

        Returns ``None`` when no explicit upstream is configured (direct
        mode).  ``proxy_by_uri`` stores the original URI as ``_jump`` even
        when there is no chain, so we must distinguish a self-referential
        jump (no upstream) from an actual nested chain.

        For ``__``-separated chains (e.g. ``socks5://...__http://...``),
        returns only the upstream portion (after the first ``__``).
        """
        if self._jump is None:
            return None
        if isinstance(self._jump, str):
            # Handle __-separated chains: return the upstream portion only.
            if "__" in self._jump:
                parts = self._jump.split("__", 1)
                upstream = parts[1].strip()
                return upstream if upstream else None
            # If _jump matches the listen URI, there is no upstream chain.
            listen_uri = self._build_listen_uri()
            if self._jump == listen_uri:
                return None
            return self._jump
        # Nested proxy object — use its stored jump or str representation
        if hasattr(self._jump, "direct") and self._jump.direct:
            return None  # Direct terminal — no upstream
        if hasattr(self._jump, "_jump") and self._jump._jump is not None:
            inner = self._jump._jump
            if isinstance(inner, str):
                return inner
            return str(inner) if inner else None
        return str(self._jump) if self._jump else None

    def __repr__(self) -> str:
        return (
            f"<{type(self).__name__} jump={self._jump!r} "
            f"host={self._host_name!r} port={self._port}>"
        )


# ---------------------------------------------------------------------------
# ProxyBackward
# ---------------------------------------------------------------------------


class ProxyBackward(ProxySimple):
    """pproxy-compatible backward/reverse proxy handler.

    Extends ``ProxySimple`` with backward connection management.

    Args:
        backward: Backward connection descriptor.
        backward_num: Number of backward connections.
        **kw: Forwarded to :class:`ProxySimple`.
    """

    def __init__(self, backward: Any = None, backward_num: int = 0, **kw: Any) -> None:
        super().__init__(**kw)
        self._backward = backward
        self._backward_num = backward_num

    @property
    def backward(self) -> Any:
        return self._backward

    @property
    def server(self) -> Any:
        return self._backward

    def close(self) -> None:
        """Close the backward connection."""

    def start_backward_client(self, args: Any) -> Any:
        """Start a backward client."""
        return None

    async def start_server(
        self,
        args: Any = None,
        stream_handler: Callable[..., Any] | None = None,
    ) -> Any:
        """Start the backward server.

        Delegates to the base :meth:`ProxySimple.start_server`.
        """
        return await super().start_server(args, stream_handler)

    async def start_server_run(self, handler: Any) -> None:
        """Run the backward server handler.  Requires runtime integration."""
        raise NotImplementedError("start_server_run requires runtime integration")

    async def udp_start_server(self, args: Any) -> None:
        """Start the backward UDP server.  Requires runtime integration."""
        raise NotImplementedError("udp_start_server requires runtime integration")

    async def wait_open_connection(self, *args: Any) -> Any:
        return None

    def __repr__(self) -> str:
        return (
            f"<{type(self).__name__} backward_num={self._backward_num}>"
        )


# ---------------------------------------------------------------------------
# ProxyH2
# ---------------------------------------------------------------------------


class ProxyH2(ProxySimple):
    """pproxy-compatible HTTP/2 proxy handler.

    Args:
        sslserver: Server-side SSL context.
        sslclient: Client-side SSL context.
        **kw: Forwarded to :class:`ProxySimple`.
    """

    def __init__(self, sslserver: Any = None, sslclient: Any = None, **kw: Any) -> None:
        super().__init__(sslserver=sslserver, sslclient=sslclient, **kw)

    def get_stream(self, conn: Any, writer: Any, stream_id: int) -> Any:
        """Get an H2 stream from a connection."""
        return None

    async def handler(
        self,
        reader: Any,
        writer: Any,
        client_side: bool = True,
        stream_handler: Callable[..., Any] | None = None,
        **kw: Any,
    ) -> None:
        """Handle an H2 connection.  Requires runtime integration."""
        raise NotImplementedError("handler requires runtime integration")

    async def start_server(
        self,
        args: Any = None,
        stream_handler: Callable[..., Any] | None = None,
    ) -> Any:
        """Start the H2 server.

        Delegates to the base :meth:`ProxySimple.start_server`.
        """
        return await super().start_server(args, stream_handler)

    async def udp_start_server(self, args: Any) -> None:
        raise NotImplementedError("udp_start_server requires runtime integration")

    async def wait_h2_connection(self, local_addr: Any, family: int) -> Any:
        """Wait for an H2 connection.  Requires runtime integration."""
        raise NotImplementedError("wait_h2_connection requires runtime integration")

    async def wait_open_connection(self, *args: Any) -> Any:
        return None

    def __repr__(self) -> str:
        return (
            f"<{type(self).__name__} sslserver={self._sslserver!r} "
            f"host={self._host_name!r} port={self._port}>"
        )


# ---------------------------------------------------------------------------
# ProxySSH
# ---------------------------------------------------------------------------


class ProxySSH(ProxySimple):
    """pproxy-compatible SSH proxy handler.

    SSH is intentionally unsupported by eggress.  This class exists for
    API compatibility with pproxy 2.7.9's class hierarchy.

    Args:
        **kw: Forwarded to :class:`ProxySimple`.
    """

    def __init__(self, **kw: Any) -> None:
        super().__init__(**kw)

    def patch_stream(
        self,
        ssh_reader: Any,
        writer: Any,
        host: str,
        port: int,
    ) -> None:
        """Patch a stream for SSH tunneling."""

    async def start_server(
        self,
        args: Any = None,
        stream_handler: Callable[..., Any] | None = None,
        tunnel: Any = None,
    ) -> None:
        raise NotImplementedError("SSH is not supported by eggress")

    async def udp_start_server(self, args: Any) -> None:
        raise NotImplementedError("SSH is not supported by eggress")

    async def wait_open_connection(
        self,
        host: str,
        port: int,
        local_addr: Any,
        family: int,
        tunnel: Any = None,
    ) -> Any:
        return None

    async def wait_ssh_connection(
        self,
        local_addr: str | None = None,
        family: int = 0,
        tunnel: Any = None,
    ) -> Any:
        """Wait for an SSH connection.  Requires runtime integration."""
        raise NotImplementedError("SSH is not supported by eggress")

    def __repr__(self) -> str:
        return (
            f"<{type(self).__name__} host={self._host_name!r} port={self._port}>"
        )


# ---------------------------------------------------------------------------
# ProxyQUIC
# ---------------------------------------------------------------------------


class ProxyQUIC(ProxySimple):
    """pproxy-compatible QUIC proxy handler.

    Args:
        quicserver: QUIC server configuration.
        quicclient: QUIC client configuration.
        **kw: Forwarded to :class:`ProxySimple`.
    """

    def __init__(
        self, quicserver: Any = None, quicclient: Any = None, **kw: Any
    ) -> None:
        super().__init__(**kw)
        self._quicserver = quicserver
        self._quicclient = quicclient

    def patch_writer(self, writer: Any) -> Any:
        """Patch a writer for QUIC transport."""
        return writer

    async def start_server(
        self,
        args: Any = None,
        stream_handler: Callable[..., Any] | None = None,
    ) -> None:
        raise NotImplementedError("QUIC is not supported by eggress")

    async def udp_start_server(self, args: Any) -> None:
        raise NotImplementedError("QUIC is not supported by eggress")

    async def wait_open_connection(self, *args: Any) -> Any:
        return None

    async def wait_quic_connection(self) -> Any:
        """Wait for a QUIC connection.  Requires runtime integration."""
        raise NotImplementedError("QUIC is not supported by eggress")

    def __repr__(self) -> str:
        return (
            f"<{type(self).__name__} host={self._host_name!r} port={self._port}>"
        )


# ---------------------------------------------------------------------------
# ProxyH3
# ---------------------------------------------------------------------------


class ProxyH3(ProxyQUIC):
    """pproxy-compatible HTTP/3 proxy handler.

    Args:
        quicserver: QUIC server configuration.
        quicclient: QUIC client configuration.
        **kw: Forwarded to :class:`ProxyQUIC`.
    """

    def __init__(
        self, quicserver: Any = None, quicclient: Any = None, **kw: Any
    ) -> None:
        super().__init__(quicserver=quicserver, quicclient=quicclient, **kw)

    def get_protocol(
        self, server_side: bool = False, handler: Any = None
    ) -> Any:
        """Get the H3 protocol factory."""
        return None

    def get_stream(self, conn: Any, stream_id: int) -> Any:
        """Get an H3 stream from a connection."""
        return None

    async def udp_start_server(self, args: Any) -> None:
        raise NotImplementedError("H3 is not supported by eggress")

    async def wait_h3_connection(self) -> Any:
        """Wait for an H3 connection.  Requires runtime integration."""
        raise NotImplementedError("H3 is not supported by eggress")

    async def wait_open_connection(self, *args: Any) -> Any:
        return None

    async def wait_quic_connection(self) -> Any:
        raise NotImplementedError("H3 is not supported by eggress")

    def __repr__(self) -> str:
        return (
            f"<{type(self).__name__} host={self._host_name!r} port={self._port}>"
        )


# ---------------------------------------------------------------------------
# Singleton
# ---------------------------------------------------------------------------

DIRECT: ProxyDirect = ProxyDirect()
"""Module-level ``ProxyDirect`` singleton, equivalent to ``pproxy.DIRECT``."""

__all__ = [
    "AuthTable",
    "ProxyDirect",
    "ProxySimple",
    "ProxyBackward",
    "ProxyH2",
    "ProxySSH",
    "ProxyQUIC",
    "ProxyH3",
    "DIRECT",
]
