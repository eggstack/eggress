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
    verbose=None,
    **kwargs,
):
    """Wrap a reader/writer pair with TLS, matching pproxy's sslwrap API.

    Returns a (ssl_reader, ssl_writer) pair where ssl_reader is an
    ``asyncio.StreamReader`` yielding decrypted data and ssl_writer is a
    thin adapter that writes through the SSL transport.  Uses
    ``asyncio.sslproto.SSLProtocol`` internally, mirroring the upstream
    pproxy 2.7.9 implementation.
    """
    if ssl_context is None:
        return reader, writer

    ssl_reader = asyncio.StreamReader()

    class _SSLProtocol(asyncio.Protocol):
        def data_received(self, data):
            ssl_reader.feed_data(data)

        def eof_received(self):
            ssl_reader.feed_eof()

        def connection_lost(self, exc):
            ssl_reader.feed_eof()

    ssl_proto = asyncio.sslproto.SSLProtocol(
        asyncio.get_event_loop(),
        _SSLProtocol(),
        ssl_context,
        None,
        server_side,
        server_hostname,
        False,
    )

    class _SSLTransport(asyncio.Transport):
        _paused = False

        def __init__(self, extra=None):
            self._extra = extra or {}
            self.closed = False

        def write(self, data):
            if data and not self.closed:
                writer.write(data)

        def close(self):
            self.closed = True
            writer.close()

        def _force_close(self, exc):
            if not self.closed:
                if verbose is not None:
                    verbose(f"{exc} from {writer.get_extra_info('peername')[0]}")
                ssl_proto._app_transport._closed = True
                self.close()

        def abort(self):
            self.close()

    ssl_proto.connection_made(_SSLTransport())

    async def _channel():
        read_size = 65536
        buffer = None
        if hasattr(ssl_proto, "get_buffer"):
            buffer = ssl_proto.get_buffer(read_size)
        try:
            while not reader.at_eof() and not ssl_proto._app_transport._closed:
                data = await reader.read(read_size)
                if not data:
                    break
                if buffer is not None:
                    data_len = len(data)
                    buffer[:data_len] = data
                    ssl_proto.buffer_updated(data_len)
                else:
                    ssl_proto.data_received(data)
        except Exception:
            pass
        finally:
            ssl_proto.eof_received()

    asyncio.ensure_future(_channel())

    class _SSLWriter:
        def get_extra_info(self, key):
            return writer.get_extra_info(key)

        def write(self, data):
            ssl_proto._app_transport.write(data)

        def drain(self):
            return writer.drain()

        def is_closing(self):
            return ssl_proto._app_transport._closed

        def close(self):
            if not ssl_proto._app_transport._closed:
                ssl_proto._app_transport.close()

    return ssl_reader, _SSLWriter()


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
