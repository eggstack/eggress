"""Public protocol helpers backed by Eggress' compatibility implementations."""

import asyncio
import base64
import hashlib
import hmac
import io
import os
import re
import socket
import struct
import time
import urllib

from eggress.protocol import *  # noqa: F401,F403
from eggress.protocol import (
    HTTP_LINE, MAPPINGS, BaseProtocol, Direct, Echo, H2, H3, HTTP, HTTPOnly,
    Pf, Redir, SS, SSH, SSR, Socks4, Socks5, Transparent, Trojan, Tunnel, WS,
    accept, get_protos, netloc_split, packstr, udp_accept,
)

SOL_IPV6 = getattr(__import__("socket"), "IPV6_TRANSPARENT", 75)
SO_ORIGINAL_DST = 80

def socks_address(reader, n):
    if n == 1:
        host = socket.inet_ntoa(reader.read(4))
    elif n == 3:
        host = reader.read(reader.read(1)[0]).decode()
    elif n == 4:
        host = socket.inet_ntop(socket.AF_INET6, reader.read(16))
    else:
        raise ValueError(f"unsupported address type: {n}")
    return host, int.from_bytes(reader.read(2), "big")

def socks_address_stream(reader):
    async def _read():
        atyp = (await reader.readexactly(1))[0]
        if atyp == 1:
            raw = bytes([atyp]) + await reader.readexactly(6)
        elif atyp == 3:
            length = (await reader.readexactly(1))[0]
            raw = bytes([atyp, length]) + await reader.readexactly(length + 2)
        elif atyp == 4:
            raw = bytes([atyp]) + await reader.readexactly(18)
        else:
            raise ValueError(f"unsupported address type: 0x{atyp:02x}")
        host, port, _ = __import__("eggress.protocol", fromlist=["decode_socks_address"]).decode_socks_address(raw)
        return host, port
    return _read()

def sslwrap(reader, writer, sslcontext=None, server_side=False):
    from eggress.pproxy import UnsupportedPProxyFeature
    raise UnsupportedPProxyFeature(
        "sslwrap",
        alternative="TLS stream wrapping is owned by the Eggress Rust transport",
    )
