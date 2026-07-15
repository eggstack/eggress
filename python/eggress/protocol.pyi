"""Type stubs for eggress.protocol module."""

from __future__ import annotations

import re
from typing import Any, Sequence

HTTP_LINE: re.Pattern[str]

class UnsupportedFeatureError(Exception): ...

def packstr(s: bytes, n: int = ...) -> bytes: ...
def netloc_split(
    loc: str, default_host: str | None = ..., default_port: int | None = ...
) -> tuple[str | None, int | None]: ...

class BaseProtocol:
    param: str
    target: str | None
    dest: str | None
    source: str | None
    _SUPPORTED_IN_EGRESS: bool
    _TRAFFIC_KINDS: tuple[str, ...]
    _ROLE: str
    def __init__(self, param: str = ..., target: str | None = ..., dest: str | None = ..., source: str | None = ...) -> None: ...
    @property
    def name(self) -> str: ...
    def reuse(self) -> bool: ...
    def udp_accept(self, data: bytes, **kw: Any) -> Any: ...
    def udp_connect(self, rauth: Any, host_name: str, port: int, data: bytes, **kw: Any) -> Any: ...
    def udp_unpack(self, data: bytes) -> bytes: ...
    def udp_pack(self, host_name: str, port: int, data: bytes) -> bytes: ...
    async def connect(self, reader_remote: Any, writer_remote: Any, rauth: Any, host_name: str, port: int, **kw: Any) -> None: ...
    async def guess(self, reader: Any, **kw: Any) -> Any: ...
    async def accept(self, reader: Any, user: Any, **kw: Any) -> tuple[Any, str, int]: ...
    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...
    def __copy__(self) -> BaseProtocol: ...
    def __deepcopy__(self, memo: dict[int, Any]) -> BaseProtocol: ...
    def __reduce__(self) -> tuple[type[BaseProtocol], tuple[str], dict[str, str | None]]: ...

class Direct(BaseProtocol):
    """Direct connection (no proxy protocol).

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """
class HTTP(BaseProtocol):
    """HTTP forward proxy (CONNECT tunnelling and plain HTTP).

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """
    httpget: dict[str, Any]
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
class Socks5(BaseProtocol):
    """SOCKS5 proxy protocol.

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """
class SS(SSR):
    """Shadowsocks AEAD proxy protocol.

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling. All encryption and decryption
    is handled by the Rust backend.
    """
    cipher: str | None
class SSR(BaseProtocol):
    """ShadowsocksR (legacy) -- intentionally unsupported by eggress.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. ShadowsocksR is rejected with
    UnsupportedFeatureError on construction.
    """
class Trojan(BaseProtocol):
    """Trojan proxy protocol.

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """
class WS(BaseProtocol):
    """WebSocket tunnel protocol.

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """
class H2(HTTP):
    """HTTP/2 CONNECT proxy protocol.

    Note: This class is construction-only. It carries typed metadata for
    composition resolution (A2 composition cells) without implementing
    functional wire-level protocol handling.
    """
class H3(H2):
    """HTTP/3 proxy protocol -- intentionally unsupported by eggress.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. HTTP/3 is rejected with UnsupportedFeatureError
    on construction.
    """
class SSH(BaseProtocol):
    """SSH proxy protocol -- intentionally unsupported by eggress.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. SSH is rejected with UnsupportedFeatureError
    on construction.
    """
class Transparent(BaseProtocol): ...
class Redir(Transparent): ...
class Pf(Transparent): ...
class Tunnel(Transparent):
    destination: str
class Echo(Transparent): ...

MAPPINGS: dict[str, type[BaseProtocol] | str]

def get_protos(rawprotos: Sequence[str]) -> tuple[str | None, list[BaseProtocol] | None]: ...
async def accept(protos: Sequence[BaseProtocol], reader: Any, **kw: Any) -> tuple[BaseProtocol, Any, Any, Any, Any]: ...
def udp_accept(protos: Sequence[BaseProtocol], data: bytes, **kw: Any) -> tuple[BaseProtocol, Any, str, int, bytes]: ...
