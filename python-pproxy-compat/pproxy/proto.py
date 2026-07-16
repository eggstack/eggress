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


def sslwrap(*args, **kwargs):
    raise UnsupportedFeatureError(
        "pproxy sslwrap is not exposed by the certified Eggress stream API"
    )


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
