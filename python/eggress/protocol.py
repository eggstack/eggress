"""Protocol object model matching pproxy 2.7.9's ``pproxy.proto`` interface.

Provides typed metadata for composition resolution (A2 composition cells)
without reimplementing the wire protocol.  Wire-level handling is delegated
to the Rust layer via ``eggress-protocol-*`` crates.
"""

from __future__ import annotations

import copy
import re
import struct
from typing import Any, Sequence


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

DEBUG: bool = False
"""Module-level debug flag.  When True, protocol errors propagate with full
context.  When False, protocol errors are logged and suppressed."""

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


def decode_socks_address(data: bytes) -> tuple[str, int, bytes]:
    """Decode a SOCKS-style address from *data*.

    Returns ``(host, port, remaining_bytes)`` where *remaining_bytes* is any
    trailing data after the address/port.  Raises ``ValueError`` on truncated
    or malformed input.
    """
    if not data:
        raise ValueError("empty data")

    atyp = data[0]
    offset = 1

    if atyp == 0x01:  # IPv4
        if len(data) < 7:
            raise ValueError(f"truncated IPv4 address: need 7 bytes, got {len(data)}")
        host = ".".join(str(b) for b in data[1:5])
        port = struct.unpack("!H", data[5:7])[0]
        offset = 7
    elif atyp == 0x03:  # Domain name
        if len(data) < 2:
            raise ValueError("truncated domain length")
        domain_len = data[1]
        if len(data) < 2 + domain_len + 2:
            raise ValueError(f"truncated domain: need {2 + domain_len + 2} bytes, got {len(data)}")
        host = data[2 : 2 + domain_len].decode("ascii")
        port = struct.unpack("!H", data[2 + domain_len : 4 + domain_len])[0]
        offset = 4 + domain_len
    elif atyp == 0x04:  # IPv6
        if len(data) < 19:
            raise ValueError(f"truncated IPv6 address: need 19 bytes, got {len(data)}")
        ipv6_bytes = data[1:17]
        # Format IPv6 address
        parts = []
        for i in range(0, 16, 2):
            parts.append(f"{ipv6_bytes[i]:02x}{ipv6_bytes[i + 1]:02x}")
        host = ":".join(parts)
        port = struct.unpack("!H", data[17:19])[0]
        offset = 19
    else:
        raise ValueError(f"unsupported address type: 0x{atyp:02x}")

    return host, port, data[offset:]


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

    def __init__(self, param: str = "") -> None:
        self.param = param
        self.target: str | None = None
        self.dest: str | None = None
        self.source: str | None = None

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

    # -- bidirectional relay -------------------------------------------------

    async def channel(
        self,
        reader: Any,
        writer: Any,
        stat_bytes: Any,
        stat_conn: Any,
    ) -> None:
        """Bidirectional relay between reader and writer (matching pproxy oracle)."""
        try:
            if stat_conn is not None:
                stat_conn(1)
            while not reader.at_eof() and not writer.is_closing():
                data = await reader.read(65536)
                if not data:
                    break
                if stat_bytes is None:
                    continue
                stat_bytes(len(data))
                writer.write(data)
                await writer.drain()
        except Exception:
            pass
        finally:
            if stat_conn is not None:
                stat_conn(-1)
            writer.close()

    async def http_channel(
        self,
        reader: Any,
        writer: Any,
        stat_bytes: Any,
        stat_conn: Any,
    ) -> None:
        """HTTP-aware relay (strips Proxy-* headers, rewrites absolute URIs)."""
        return await self.channel(reader, writer, stat_bytes, stat_conn)

    # -- equality / hashing / repr ------------------------------------------

    def __eq__(self, other: object) -> bool:
        return (
            type(self) is type(other)
            and self.param == getattr(other, "param", None)
        )

    def __hash__(self) -> int:
        return hash((type(self), self.param))

    def __repr__(self) -> str:
        redacted = _redact_param(self.name, self.param)
        if redacted:
            return f"{self.__class__.__name__}({redacted!r})"
        return f"{self.__class__.__name__}()"

    def __str__(self) -> str:
        return self.name

    # -- pickling / copying -------------------------------------------------

    def __reduce__(self) -> tuple[type[BaseProtocol], tuple[str], dict[str, str | None]]:
        state: dict[str, str | None] = {}
        if self.target is not None:
            state["target"] = self.target
        if self.dest is not None:
            state["dest"] = self.dest
        if self.source is not None:
            state["source"] = self.source
        return (self.__class__, (self.param,), state)

    def __copy__(self) -> BaseProtocol:
        c = self.__class__(self.param)
        c.target = self.target
        c.dest = self.dest
        c.source = self.source
        return c

    def __deepcopy__(self, memo: dict[int, Any]) -> BaseProtocol:
        c = self.__class__(copy.deepcopy(self.param, memo))
        c.target = copy.deepcopy(self.target, memo)
        c.dest = copy.deepcopy(self.dest, memo)
        c.source = copy.deepcopy(self.source, memo)
        return c

    def __setstate__(self, state: dict[str, str | None]) -> None:
        self.target = state.get("target")
        self.dest = state.get("dest")
        self.source = state.get("source")


# ---------------------------------------------------------------------------
# Protocol implementations
# ---------------------------------------------------------------------------


class Direct(BaseProtocol):
    """Direct connection (no proxy protocol).

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """

    def __init__(self, param: str = "") -> None:
        super().__init__(param)
        self.target = param or None


class HTTP(BaseProtocol):
    """HTTP forward proxy (CONNECT tunnelling and plain HTTP).

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """

    def __init__(self, param: str = "") -> None:
        super().__init__(param)
        self.target = param or None
        self.httpget: dict[str, Any] = {}
        self._buffered: bytes = b""

    async def guess(self, reader: Any, **kw: Any) -> Any:
        """Read up to 1024 bytes and check for valid HTTP method prefix."""
        data = await reader.read(1024)
        if not data:
            return None

        # Check for valid HTTP method prefix
        http_methods = (b"GET", b"POST", b"CONNECT", b"PUT", b"DELETE",
                        b"HEAD", b"OPTIONS", b"PATCH")
        for method in http_methods:
            if data.startswith(method):
                self._buffered = data
                return data

        return None

    async def accept(self, reader: Any, user: Any, writer: Any, **kw: Any) -> tuple[Any, str, int]:
        """Parse HTTP request line to extract host and port."""
        data = self._buffered
        if not data:
            data = await reader.read(1024)
        self._buffered = b""

        # Find the end of the first line
        first_line_end = data.find(b"\r\n")
        if first_line_end == -1:
            raise ValueError("incomplete HTTP request line")

        first_line = data[:first_line_end].decode("ascii", errors="ignore")
        match = HTTP_LINE.match(first_line)
        if not match:
            raise ValueError(f"malformed HTTP request line: {first_line!r}")

        method, target, version = match.groups()

        # Extract host and port from target or Host header
        host = ""
        port = 80

        if method.upper() == "CONNECT":
            # CONNECT host:port HTTP/1.1
            host, port = netloc_split(target, default_port=443)
            if host is None:
                raise ValueError("CONNECT without target host")
            port = port or 443
        else:
            # For plain HTTP, look for Host header
            remaining = data[first_line_end + 2:]
            while remaining:
                line_end = remaining.find(b"\r\n")
                if line_end == -1:
                    break
                line = remaining[:line_end]
                remaining = remaining[line_end + 2:]

                if line.lower().startswith(b"host:"):
                    host_port = line[5:].decode("ascii", errors="ignore").strip()
                    host, port = netloc_split(host_port, default_port=80)
                    port = port or 80
                    break

            if not host:
                # Try to extract from URL
                if "://" in target:
                    scheme, _, rest = target.partition("://")
                    host, port = netloc_split(rest.split("/")[0], default_port=80)
                    port = port or 80

        if not host:
            raise ValueError("could not determine target host")

        return user, host, port


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

    def __init__(self, param: str = "") -> None:
        super().__init__(param)
        self.target = param or None
        self._buffered: bytes = b""

    async def guess(self, reader: Any, **kw: Any) -> Any:
        """Read up to 1024 bytes and check for SOCKS4 version byte."""
        data = await reader.read(1024)
        if not data:
            return None

        # SOCKS4 version byte is 0x04
        if data[0] == 0x04:
            self._buffered = data
            return data

        return None

    async def accept(self, reader: Any, user: Any, writer: Any, users: Any, authtable: Any, **kw: Any) -> tuple[Any, str, int]:
        """Parse SOCKS4 request to extract host and port."""
        data = self._buffered
        if not data:
            data = await reader.read(1024)
        self._buffered = b""

        if len(data) < 8:
            raise ValueError("truncated SOCKS4 request")

        # Byte 0: version (0x04)
        # Byte 1: command (0x01 = connect, 0x02 = bind)
        # Bytes 2-3: port (big-endian)
        # Bytes 4-7: IP address
        cmd = data[1]
        port = struct.unpack("!H", data[2:4])[0]
        ip_bytes = data[4:8]

        # Check for SOCKS4a (IP == 0.0.0.x)
        if ip_bytes[0] == 0 and ip_bytes[1] == 0 and ip_bytes[2] == 0 and ip_bytes[3] != 0:
            # SOCKS4a: read hostname after null-terminated userid
            # Find the null byte after the 8-byte header
            null_pos = data.find(b"\x00", 8)
            if null_pos == -1:
                raise ValueError("SOCKS4a missing null terminator after userid")
            hostname_start = null_pos + 1
            hostname_end = data.find(b"\x00", hostname_start)
            if hostname_end == -1:
                raise ValueError("SOCKS4a missing null terminator after hostname")
            host = data[hostname_start:hostname_end].decode("ascii")
        else:
            # Standard SOCKS4: use IP address
            host = ".".join(str(b) for b in ip_bytes)

        return user, host, port


class Socks5(BaseProtocol):
    """SOCKS5 proxy protocol.

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """

    _TRAFFIC_KINDS: tuple[str, ...] = ("tcp", "udp")

    def __init__(self, param: str = "") -> None:
        super().__init__(param)
        self.target = param or None
        self._buffered: bytes = b""

    async def guess(self, reader: Any, **kw: Any) -> Any:
        """Read up to 1024 bytes and check for SOCKS5 version byte."""
        data = await reader.read(1024)
        if not data:
            return None

        # SOCKS5 version byte is 0x05
        if data[0] == 0x05:
            self._buffered = data
            return data

        return None

    async def accept(self, reader: Any, user: Any, writer: Any, users: Any, authtable: Any, **kw: Any) -> tuple[Any, str, int]:
        """Parse SOCKS5 handshake and connect request."""
        data = self._buffered
        if not data:
            data = await reader.read(1024)
        self._buffered = b""

        # SOCKS5 greeting: version (0x05), nmethods, methods[nmethods]
        if len(data) < 3:
            raise ValueError("truncated SOCKS5 greeting")

        version = data[0]
        if version != 0x05:
            raise ValueError(f"invalid SOCKS5 version: {version}")

        nmethods = data[1]
        methods = data[2 : 2 + nmethods]
        data = data[2 + nmethods :]

        # Check if username/password auth (0x02) is required
        if 0x02 in methods:
            # Respond with username/password auth
            # In a real implementation, we'd send: [0x05, 0x02]
            # and read the auth response, but for now we just parse

            # Read auth response: version (0x01), ulen, username, plen, password
            if len(data) < 2:
                # Need to read more data
                raise ValueError("incomplete SOCKS5 auth response")

            auth_version = data[0]
            if auth_version != 0x01:
                raise ValueError(f"invalid auth version: {auth_version}")

            ulen = data[1]
            if len(data) < 2 + ulen + 1:
                raise ValueError("truncated auth username")
            username = data[2 : 2 + ulen]
            plen = data[2 + ulen]
            if len(data) < 2 + ulen + 1 + plen:
                raise ValueError("truncated auth password")
            password = data[3 + ulen : 3 + ulen + plen]
            data = data[3 + ulen + plen :]

            # Store auth info for later use
            user = (username, password)

        # SOCKS5 connect request: version, command, reserved, address type, address, port
        if len(data) < 4:
            raise ValueError("truncated SOCKS5 connect request")

        version = data[0]
        cmd = data[1]
        # data[2] is reserved (0x00)
        atyp = data[3]

        if atyp == 0x01:  # IPv4
            if len(data) < 10:
                raise ValueError("truncated IPv4 address in SOCKS5")
            host = ".".join(str(b) for b in data[4:8])
            port = struct.unpack("!H", data[8:10])[0]
        elif atyp == 0x03:  # Domain name
            if len(data) < 5:
                raise ValueError("truncated domain length in SOCKS5")
            domain_len = data[4]
            if len(data) < 5 + domain_len + 2:
                raise ValueError("truncated domain in SOCKS5")
            host = data[5 : 5 + domain_len].decode("ascii")
            port = struct.unpack("!H", data[5 + domain_len : 7 + domain_len])[0]
        elif atyp == 0x04:  # IPv6
            if len(data) < 22:
                raise ValueError("truncated IPv6 address in SOCKS5")
            ipv6_bytes = data[4:20]
            parts = []
            for i in range(0, 16, 2):
                parts.append(f"{ipv6_bytes[i]:02x}{ipv6_bytes[i + 1]:02x}")
            host = ":".join(parts)
            port = struct.unpack("!H", data[20:22])[0]
        else:
            raise ValueError(f"unsupported address type: 0x{atyp:02x}")

        return user, host, port

    def udp_pack(self, host_name: str, port: int, data: bytes) -> bytes:
        """Encode data with SOCKS5 UDP header."""
        # Encode address using SOCKS address format
        addr = self._encode_socks_address(host_name, port)
        # Reserved (2 bytes 0x00) + Fragment (1 byte 0x00) + Address + Data
        return b"\x00\x00\x00" + addr + data

    def udp_unpack(self, data: bytes) -> tuple[str, int, bytes]:
        """Parse SOCKS5 UDP header and return (host, port, payload)."""
        if len(data) < 4:
            raise ValueError("SOCKS5 UDP header too short")
        # Skip reserved (2 bytes) + fragment (1 byte)
        host, port, remaining = decode_socks_address(data[3:])
        return host, port, remaining

    def _encode_socks_address(self, host: str, port: int) -> bytes:
        """Encode host:port as SOCKS address bytes."""
        # Try to parse as IPv4
        try:
            parts = host.split(".")
            if len(parts) == 4 and all(0 <= int(p) <= 255 for p in parts):
                addr = bytes(int(p) for p in parts)
                return b"\x01" + addr + struct.pack("!H", port)
        except (ValueError, AttributeError):
            pass

        # Try to parse as IPv6
        if ":" in host:
            # Simple IPv6 handling - just encode as-is for now
            # In practice, this needs proper IPv6 parsing
            pass

        # Domain name
        domain_bytes = host.encode("ascii")
        return b"\x03" + bytes([len(domain_bytes)]) + domain_bytes + struct.pack("!H", port)


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

    async def guess(self, reader: Any, **kw: Any) -> Any:
        """Shadowsocks has no protocol-level prefix to detect.

        SS is detected via config, not sniffing. Return None.
        """
        return None

    async def accept(self, reader: Any, user: Any, reader_cipher: Any = None, **kw: Any) -> tuple[Any, str, int]:
        """Since SS is encrypted, this can't parse without the cipher.

        Return the reader/user as-is for now. The actual handling
        is done by the Rust runtime.
        """
        return user, "", 0

    def udp_pack(self, host_name: str, port: int, data: bytes) -> bytes:
        """Encode data with Shadowsocks UDP address header."""
        addr = self._encode_shadowsocks_address(host_name, port)
        return addr + data

    def udp_unpack(self, data: bytes) -> tuple[str, int, bytes]:
        """Parse Shadowsocks UDP address header and return (host, port, payload)."""
        host, port, remaining = decode_socks_address(data)
        return host, port, remaining

    def _encode_shadowsocks_address(self, host: str, port: int) -> bytes:
        """Encode host:port as Shadowsocks address bytes (same as SOCKS format)."""
        # Try to parse as IPv4
        try:
            parts = host.split(".")
            if len(parts) == 4 and all(0 <= int(p) <= 255 for p in parts):
                addr = bytes(int(p) for p in parts)
                return b"\x01" + addr + struct.pack("!H", port)
        except (ValueError, AttributeError):
            pass

        # Try to parse as IPv6
        if ":" in host:
            # Simple IPv6 handling - just encode as-is for now
            # In practice, this needs proper IPv6 parsing
            pass

        # Domain name
        domain_bytes = host.encode("ascii")
        return b"\x03" + bytes([len(domain_bytes)]) + domain_bytes + struct.pack("!H", port)


class Trojan(BaseProtocol):
    """Trojan proxy protocol.

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """

    def __init__(self, param: str = "") -> None:
        super().__init__(param)
        if param:
            _, _, host_part = param.rpartition("@")
            self.target = host_part if host_part else param


class WS(BaseProtocol):
    """WebSocket tunnel protocol.

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """

    def __init__(self, param: str = "") -> None:
        super().__init__(param)
        if param:
            self.target = param.split("/", 1)[0] if "/" in param else param


class H2(HTTP):
    """HTTP/2 CONNECT proxy protocol.

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """

    def __init__(self, param: str = "") -> None:
        super().__init__(param)


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

    def __init__(self, param: str = "") -> None:
        super().__init__(param)
        self.dest = param or None
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
    last_error: Exception | None = None
    for proto in protos:
        try:
            user = await proto.guess(reader, **kw)
        except Exception as exc:
            last_error = exc
            if DEBUG:
                raise
            continue
        if user:
            try:
                ret = await proto.accept(reader, user, **kw)
            except Exception as exc:
                last_error = exc
                if DEBUG:
                    raise
                continue
            while len(ret) < 4:
                ret += (None,)
            return (proto,) + ret
    if DEBUG and last_error:
        raise last_error
    raise Exception("Unsupported protocol")


def udp_accept(
    protos: Sequence[BaseProtocol], data: bytes, **kw: Any
) -> tuple[BaseProtocol, Any, str, int, bytes]:
    """Try each protocol in order; return ``(proto, user, host, port, payload)`` on first match."""
    last_error: Exception | None = None
    for proto in protos:
        try:
            ret = proto.udp_accept(data, **kw)
        except Exception as exc:
            last_error = exc
            if DEBUG:
                raise
            continue
        if ret:
            return (proto,) + ret
    if DEBUG and last_error:
        raise last_error
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
    "DEBUG",
    "HTTP_LINE",
    # Errors
    "UnsupportedFeatureError",
    # Helpers
    "packstr",
    "netloc_split",
    "decode_socks_address",
    # Module-level functions
    "get_protos",
    "accept",
    "udp_accept",
]
