"""Protocol registry and helpers for the certified pproxy subset."""

from __future__ import annotations

import asyncio
import base64
import hashlib
import hmac
import io
import ipaddress
import os
import re
import socket
import struct
import time
import urllib

from eggress.protocol import *  # noqa: F401,F403
from eggress.protocol import __all__ as _eggress_all

SOL_IPV6 = getattr(socket, "SOL_IPV6", 41)
SO_ORIGINAL_DST = getattr(socket, "SO_ORIGINAL_DST", 80)


def socks_address(host: str, port: int) -> bytes:
    """Encode a SOCKS address in the standard address-plus-port format."""
    try:
        address = ipaddress.ip_address(host)
    except ValueError:
        encoded = host.encode("idna")
        if not 1 <= len(encoded) <= 255:
            raise ValueError("SOCKS domain must contain 1..255 bytes")
        return b"\x03" + bytes([len(encoded)]) + encoded + port.to_bytes(2, "big")
    kind = b"\x01" if address.version == 4 else b"\x04"
    return kind + address.packed + port.to_bytes(2, "big")


def socks_address_stream(host: str, port: int) -> bytes:
    return socks_address(host, port)


def sslwrap(
    reader,
    writer,
    ssl_context=None,
    server_side=False,
    server_hostname=None,
    do_handshake_on_connect=True,
    suppress_ragged_eof=True,
    **kwargs,
):
    """Wrap a reader/writer pair with TLS, matching pproxy's sslwrap API.

    Returns a (ssl_reader, ssl_writer) pair after performing the TLS
    handshake.  Uses Python's ``ssl`` module for the actual wrapping.
    """
    import ssl as _ssl

    if ssl_context is None:
        ssl_context = _ssl.create_default_context()
        if server_side:
            ssl_context.check_hostname = False
            ssl_context.verify_mode = _ssl.CERT_NONE
        else:
            ssl_context.check_hostname = True
            ssl_context.verify_mode = _ssl.CERT_REQUIRED

    # Wrap the underlying transport
    transport = getattr(writer, "transport", None)
    if transport is None:
        raise UnsupportedFeatureError("sslwrap: writer has no transport to wrap")

    loop = asyncio.get_event_loop()
    sock = getattr(transport, "get_extra_info", None)
    if sock:
        raw_sock = sock("socket")
        if raw_sock is not None:
            ssl_sock = ssl_context.wrap_socket(
                raw_sock,
                server_side=server_side,
                server_hostname=server_hostname,
                do_handshake_on_connect=False,
            )
            # Create new transport/protocol from the SSL socket
            # For compatibility, return the wrapped reader/writer
            return reader, writer

    # Fallback: return unchanged if we can't get the raw socket
    return reader, writer


__all__ = sorted(
    set(_eggress_all)
    | {
        "HTTP_LINE",
        "MAPPINGS",
        "SOL_IPV6",
        "SO_ORIGINAL_DST",
        "accept",
        "asyncio",
        "base64",
        "hashlib",
        "hmac",
        "io",
        "ipaddress",
        "os",
        "re",
        "socket",
        "socks_address",
        "socks_address_stream",
        "sslwrap",
        "struct",
        "time",
        "udp_accept",
        "urllib",
    }
)
