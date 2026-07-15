"""Protocol object model matching pproxy 2.7.9's ``pproxy.proto`` interface.

Provides typed metadata for composition resolution (A2 composition cells)
without reimplementing the wire protocol.  Wire-level handling is delegated
to the Rust layer via ``eggress-protocol-*`` crates.
"""

from __future__ import annotations

import copy
import re
from typing import Any, Sequence


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

HTTP_LINE = re.compile(r"([^ ]+) +(.+?) +(HTTP/[^ ]+)$")

_SECRET_PATTERNS: list[re.Pattern[str]] = [
    re.compile(r"^[a-zA-Z0-9_+/=-]{20,}$"),  # long token / key
    re.compile(r"^[0-9a-fA-F]{32,}$"),  # hex-encoded key
]


# ---------------------------------------------------------------------------
# Errors
# ---------------------------------------------------------------------------


class UnsupportedFeatureError(Exception):
    """Raised when a protocol is recognised but not implemented by eggress."""


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def packstr(s: bytes, n: int = 1) -> bytes:
    """Encode *s* with a big-endian length prefix of *n* bytes."""
    return len(s).to_bytes(n, "big") + s


def netloc_split(
    loc: str,
    default_host: str | None = None,
    default_port: int | None = None,
) -> tuple[str | None, int | None]:
    """Split a ``host:port`` or ``[ipv6]:port`` string.

    Returns ``(host, port)`` with *default_host* / *default_port* as fallbacks.
    """
    ipv6 = re.fullmatch(r"\[([0-9a-fA-F:]*)\](?::(\d+)?)?", loc)
    if ipv6:
        host_name, port = ipv6.groups()
    elif ":" in loc:
        host_name, port = loc.rsplit(":", 1)
    else:
        host_name, port = loc, None
    return host_name or default_host, int(port) if port else default_port


def _redact_param(protocol_name: str, param: str) -> str:
    """Return a repr-safe version of *param*, hiding obvious secrets."""
    if not param:
        return param
    # Shadowsocks uses "cipher:password" format
    if protocol_name == "ss" and ":" in param:
        cipher, _, _ = param.partition(":")
        return f"{cipher}:***"
    for pattern in _SECRET_PATTERNS:
        if pattern.match(param):
            return "***"
    return param


# ---------------------------------------------------------------------------
# Base protocol
# ---------------------------------------------------------------------------


class BaseProtocol:
    """Base class for all protocol objects.

    Subclasses must preserve the ``__init__(self, param='')`` signature so
    that instances are picklable via ``__reduce__``.
    """

    # A2 composition metadata (overridden by subclasses as needed)
    _SUPPORTED_IN_EGRESS: bool = True
    _TRAFFIC_KINDS: tuple[str, ...] = ("tcp",)
    _ROLE: str = "both"  # "listener" | "upstream" | "both"

    def __init__(
        self,
        param: str = "",
        target: str | None = None,
        dest: str | None = None,
        source: str | None = None,
    ) -> None:
        self.param = param
        self.target = target
        self.dest = dest
        self.source = source

    # -- identity -----------------------------------------------------------

    @property
    def name(self) -> str:
        return self.__class__.__name__.lower()

    def reuse(self) -> bool:
        return False

    # -- UDP ----------------------------------------------------------------

    def udp_accept(self, data: bytes, **kw: Any) -> Any:
        raise NotImplementedError(f"{self.name} does not support UDP server")

    def udp_connect(
        self,
        rauth: bytes | None,
        host_name: str,
        port: int,
        data: bytes,
        **kw: Any,
    ) -> Any:
        raise NotImplementedError(f"{self.name} does not support UDP client")

    def udp_unpack(self, data: bytes) -> bytes:
        return data

    def udp_pack(self, host_name: str, port: int, data: bytes) -> bytes:
        return data

    # -- TCP ----------------------------------------------------------------

    async def connect(
        self,
        reader_remote: Any,
        writer_remote: Any,
        rauth: bytes | None,
        host_name: str,
        port: int,
        **kw: Any,
    ) -> None:
        raise NotImplementedError(f"{self.name} does not support client mode")

    # -- protocol detection stubs (used by module-level accept/udp_accept) ---

    async def guess(self, reader: Any, **kw: Any) -> Any:
        raise NotImplementedError(f"{self.name} does not implement guess")

    async def accept(self, reader: Any, user: Any, **kw: Any) -> tuple[Any, str, int]:
        raise NotImplementedError(f"{self.name} does not implement accept")

    # -- equality / hashing / repr ------------------------------------------

    def __eq__(self, other: object) -> bool:
        return (
            type(self) is type(other)
            and self.param == getattr(other, "param", None)
            and self.target == getattr(other, "target", None)
            and self.dest == getattr(other, "dest", None)
            and self.source == getattr(other, "source", None)
        )

    def __hash__(self) -> int:
        return hash((type(self), self.param, self.target, self.dest, self.source))

    def __repr__(self) -> str:
        redacted = _redact_param(self.name, self.param)
        if redacted:
            return f"{self.__class__.__name__}({redacted!r})"
        return f"{self.__class__.__name__}()"

    def __str__(self) -> str:
        return self.name

    # -- pickling / copying -------------------------------------------------

    def __reduce__(self) -> tuple[type[BaseProtocol], tuple[str], dict[str, str | None]]:
        return (
            self.__class__,
            (self.param,),
            {"target": self.target, "dest": self.dest, "source": self.source},
        )

    def __copy__(self) -> BaseProtocol:
        return self.__class__(
            self.param, target=self.target, dest=self.dest, source=self.source
        )

    def __deepcopy__(self, memo: dict[int, Any]) -> BaseProtocol:
        return self.__class__(
            copy.deepcopy(self.param, memo),
            target=copy.deepcopy(self.target, memo),
            dest=copy.deepcopy(self.dest, memo),
            source=copy.deepcopy(self.source, memo),
        )


# ---------------------------------------------------------------------------
# Protocol implementations
# ---------------------------------------------------------------------------


class Direct(BaseProtocol):
    """Direct connection (no proxy protocol).

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """

    def __init__(self, param: str = "", **kw: Any) -> None:
        kw.setdefault("target", param or None)
        super().__init__(param, **kw)


class HTTP(BaseProtocol):
    """HTTP forward proxy (CONNECT tunnelling and plain HTTP).

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """

    def __init__(self, param: str = "", **kw: Any) -> None:
        kw.setdefault("target", param or None)
        super().__init__(param, **kw)
        self.httpget: dict[str, Any] = {}


class HTTPOnly(HTTP):
    """HTTP-only forward proxy (no CONNECT tunnelling).

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """


class Socks4(BaseProtocol):
    """SOCKS4 / SOCKS4a proxy protocol.

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """

    def __init__(self, param: str = "", **kw: Any) -> None:
        kw.setdefault("target", param or None)
        super().__init__(param, **kw)


class Socks5(BaseProtocol):
    """SOCKS5 proxy protocol.

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """

    _TRAFFIC_KINDS: tuple[str, ...] = ("tcp", "udp")

    def __init__(self, param: str = "", **kw: Any) -> None:
        kw.setdefault("target", param or None)
        super().__init__(param, **kw)


class SSR(BaseProtocol):
    """ShadowsocksR (legacy) -- intentionally unsupported by eggress.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. ShadowsocksR is rejected with
    UnsupportedFeatureError on construction.
    """

    _SUPPORTED_IN_EGRESS: bool = False

    def __init__(self, param: str = "") -> None:
        raise UnsupportedFeatureError(
            "ShadowsocksR (ssr://) is not supported by eggress. "
            "Use Shadowsocks (ss://) with standard AEAD methods instead. "
            "See docs/adr/ADR_legacy_shadowsocks_ssr_compatibility.md"
        )


class SS(SSR):
    """Shadowsocks AEAD proxy protocol.

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling. All encryption and decryption
    is handled by the Rust backend.
    """

    _SUPPORTED_IN_EGRESS: bool = True
    _TRAFFIC_KINDS: tuple[str, ...] = ("tcp", "udp")

    def __init__(self, param: str = "") -> None:
        # Bypass SSR.__init__ (which raises UnsupportedFeatureError) and
        # initialise directly from BaseProtocol.
        BaseProtocol.__init__(self, param)
        self.cipher: str | None = None
        if param and ":" in param:
            self.cipher, _, _ = param.partition(":")


class Trojan(BaseProtocol):
    """Trojan proxy protocol.

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """

    def __init__(self, param: str = "", **kw: Any) -> None:
        if "target" not in kw:
            target: str | None = None
            if param:
                _, _, host_part = param.rpartition("@")
                target = host_part if host_part else param
            kw["target"] = target
        super().__init__(param, **kw)


class WS(BaseProtocol):
    """WebSocket tunnel protocol.

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """

    def __init__(self, param: str = "", **kw: Any) -> None:
        if "target" not in kw:
            target: str | None = None
            if param:
                target = param.split("/", 1)[0] if "/" in param else param
            kw["target"] = target
        super().__init__(param, **kw)


class H2(HTTP):
    """HTTP/2 CONNECT proxy protocol.

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """

    def __init__(self, param: str = "", **kw: Any) -> None:
        super().__init__(param, **kw)


class H3(H2):
    """HTTP/3 proxy protocol -- intentionally unsupported by eggress.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. HTTP/3 is rejected with UnsupportedFeatureError
    on construction.
    """

    _SUPPORTED_IN_EGRESS: bool = False

    def __init__(self, param: str = "") -> None:
        raise UnsupportedFeatureError(
            "HTTP/3 (h3://) is not supported by eggress. "
            "Use HTTP/2 (h2://), WebSocket (ws://), or raw tunnel instead."
        )


class SSH(BaseProtocol):
    """SSH proxy protocol -- intentionally unsupported by eggress.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. SSH is rejected with UnsupportedFeatureError
    on construction.
    """

    _SUPPORTED_IN_EGRESS: bool = False

    def __init__(self, param: str = "") -> None:
        raise UnsupportedFeatureError(
            "SSH (ssh://) is not supported by eggress. "
            "Use OpenSSH dynamic forwarding (ssh -D) instead."
        )


class Transparent(BaseProtocol):
    """Transparent proxy base class (listener-only)."""

    _ROLE: str = "listener"

    async def guess(self, reader: Any, sock: Any, **kw: Any) -> bool:
        remote = self.query_remote(sock)
        return remote is not None and (sock is None or sock.getsockname() != remote)

    async def accept(
        self, reader: Any, user: Any, sock: Any, **kw: Any
    ) -> tuple[Any, str, int]:
        remote = self.query_remote(sock)
        return user, remote[0], remote[1]

    def udp_accept(self, data: bytes, sock: Any, **kw: Any) -> tuple[bool, str, int, bytes]:
        remote = self.query_remote(sock)
        return True, remote[0], remote[1], data

    def query_remote(self, sock: Any) -> tuple[str, int] | None:
        raise NotImplementedError("Subclasses must implement query_remote")


class Redir(Transparent):
    """Linux transparent proxy (SO_ORIGINAL_DST / IP6T_SO_ORIGINAL_DST)."""


class Pf(Transparent):
    """macOS PF transparent proxy."""


class Tunnel(Transparent):
    """Fixed-target tunnel (data forwarded to a predetermined destination)."""

    def __init__(self, param: str = "", **kw: Any) -> None:
        kw.setdefault("dest", param or None)
        super().__init__(param, **kw)
        self.destination: str = param

    def query_remote(self, sock: Any) -> tuple[str, int]:
        if not self.param:
            return ("tunnel", 0)
        dst = sock.getsockname() if sock else (None, None)
        return netloc_split(self.param, dst[0], dst[1])

    async def connect(
        self,
        reader_remote: Any,
        writer_remote: Any,
        rauth: Any,
        host_name: str,
        port: int,
        **kw: Any,
    ) -> None:
        pass

    def udp_connect(
        self, rauth: Any, host_name: str, port: int, data: bytes, **kw: Any
    ) -> bytes:
        return data


class Echo(Transparent):
    """Echo server (returns all data back to the sender)."""

    def query_remote(self, sock: Any) -> tuple[str, int]:
        return ("echo", 0)


# ---------------------------------------------------------------------------
# Module-level protocol functions
# ---------------------------------------------------------------------------


async def accept(
    protos: Sequence[BaseProtocol], reader: Any, **kw: Any
) -> tuple[BaseProtocol, Any, Any, Any, Any]:
    """Try each protocol in order; return ``(proto, user, host, port, extra)`` on first match."""
    for proto in protos:
        try:
            user = await proto.guess(reader, **kw)
        except Exception:
            raise Exception("Connection closed")
        if user:
            ret = await proto.accept(reader, user, **kw)
            while len(ret) < 4:
                ret += (None,)
            return (proto,) + ret
    raise Exception("Unsupported protocol")


def udp_accept(
    protos: Sequence[BaseProtocol], data: bytes, **kw: Any
) -> tuple[BaseProtocol, Any, str, int, bytes]:
    """Try each protocol in order; return ``(proto, user, host, port, payload)`` on first match."""
    for proto in protos:
        ret = proto.udp_accept(data, **kw)
        if ret:
            return (proto,) + ret
    raise Exception(f"Unsupported protocol {data[:10]}")


def get_protos(
    rawprotos: Sequence[str],
) -> tuple[str | None, list[BaseProtocol] | None]:
    """Resolve protocol name strings to instances.

    Each entry can be ``"name"`` or ``"name{param}"``.

    Returns ``(error_message, None)`` on failure or ``(None, [instances])``
    on success -- matching pproxy's ``get_protos`` return convention.
    """
    protos: list[BaseProtocol] = []
    for s in rawprotos:
        s, _, param = s.partition("{")
        param = param[:-1] if param else None
        p = MAPPINGS.get(s)
        if p is None:
            return f"existing protocols: {list(MAPPINGS.keys())}", None
        if p and p not in protos:
            protos.append(p(param or ""))
    if not protos:
        return "no protocol specified", None
    return None, protos


# ---------------------------------------------------------------------------
# MAPPINGS -- scheme string to class (or "" for TLS/in markers)
# ---------------------------------------------------------------------------

_PROTOCOL_REGISTRY: dict[str, type[BaseProtocol]] = {
    "direct": Direct,
    "http": HTTP,
    "httponly": HTTPOnly,
    "socks4": Socks4,
    "socks4a": Socks4,
    "socks5": Socks5,
    "socks": Socks5,
    "ss": SS,
    "ssr": SSR,
    "trojan": Trojan,
    "ws": WS,
    "h2": H2,
    "h3": H3,
    "ssh": SSH,
    "redir": Redir,
    "pf": Pf,
    "tunnel": Tunnel,
    "echo": Echo,
}

MAPPINGS: dict[str, type[BaseProtocol] | str] = {
    **_PROTOCOL_REGISTRY,
    "ssl": "",
    "secure": "",
    "https": HTTP,
    "quic": "",
    "httpget": HTTP,
    "in": "",
}


__all__ = [
    # Base
    "BaseProtocol",
    # Protocol classes
    "Direct",
    "HTTP",
    "HTTPOnly",
    "Socks4",
    "Socks5",
    "SS",
    "SSR",
    "Trojan",
    "WS",
    "H2",
    "H3",
    "SSH",
    "Transparent",
    "Redir",
    "Pf",
    "Tunnel",
    "Echo",
    # Registries
    "MAPPINGS",
    "_PROTOCOL_REGISTRY",
    # Constants
    "HTTP_LINE",
    # Errors
    "UnsupportedFeatureError",
    # Helpers
    "packstr",
    "netloc_split",
    # Module-level functions
    "get_protos",
    "accept",
    "udp_accept",
]
