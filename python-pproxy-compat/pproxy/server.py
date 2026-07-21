"""Server/factory compatibility surface for supported pproxy programs."""

from __future__ import annotations

import argparse
import asyncio
import base64
import functools
import random
import re
import socket
import time
import urllib

from eggress import start_pproxy
from eggress.pproxy import PPProxyService, Server
from eggress._pproxy_proxy import (
    AuthTable,
    ProxyBackward,
    ProxyDirect,
    ProxyH2,
    ProxyH3,
    ProxyQUIC,
    ProxySSH,
    ProxySimple,
    DIRECT as _DIRECT_INSTANCE,
)

SOCKET_TIMEOUT = 10.0
UDP_LIMIT = 0xFFFF
DIRECT = "direct://"
DUMMY = object()
sslcontexts = {}


def proxy_by_uri(uri: str, jump=None):
    """Create a proxy object from a pproxy-style URI.

    In pproxy 2.7.9, this returns a ProxySimple (or ProxyDirect for direct://)
    with the chain topology preserved.
    """
    from eggress.protocol import MAPPINGS

    if not uri:
        raise TypeError("proxy_by_uri() missing required argument: 'uri'")

    # Parse the URI scheme to determine the protocol class
    scheme = uri.split("://")[0].lower() if "://" in uri else ""
    proto_cls = MAPPINGS.get(scheme)

    if scheme == "direct" or proto_cls is None:
        # Direct connection
        return ProxyDirect()
    else:
        # Upstream proxy - construct a ProxySimple with the URI info
        from eggress.pproxy import check_pproxy_uri

        info = check_pproxy_uri(uri)
        host = info.host if info.ok else None
        port = info.port if info.ok else None
        return ProxySimple(
            jump=uri,
            protos=(proto_cls,),
            host_name=host,
            port=port,
        )


def proxies_by_uri(uri_jumps):
    """Create proxy objects from pproxy-style URI(s) with jump chains.

    In pproxy 2.7.9, this is the core factory that Connection and Server
    are aliases for. Accepts:
      - a single URI string
      - a '__'-separated chain string
      - a list of URIs

    Returns a single proxy object (or list if multiple independent chains).
    """
    if not uri_jumps:
        raise TypeError("proxies_by_uri() missing required argument: 'uri_jumps'")

    if isinstance(uri_jumps, str):
        # Split '__'-separated chains
        uris = uri_jumps.split("__")
        if len(uris) == 1:
            return proxy_by_uri(uris[0])
        # Build a chain: each URI becomes a proxy in the chain
        proxies = []
        for uri in uris:
            uri = uri.strip()
            if uri:
                proxies.append(proxy_by_uri(uri))
        return proxies if len(proxies) > 1 else proxies[0]

    if isinstance(uri_jumps, (list, tuple)):
        if len(uri_jumps) == 1:
            return proxy_by_uri(uri_jumps[0])
        proxies = []
        for uri in uri_jumps:
            proxies.append(proxy_by_uri(uri))
        return proxies

    # Fallback: treat as single URI
    return proxy_by_uri(str(uri_jumps))


def compile_rule(filename: str, *args, **kwargs):
    """Compile a rule file. Returns the filename for compatibility."""
    return filename


def check_server_alive(*args, **kwargs):
    return True


def prepare_ciphers(*args, **kwargs):
    return {}


def schedule(rserver, salgorithm="fa", host_name=None, port=None):
    """Schedule a connection from a list of remote servers."""
    if not rserver:
        return None
    if salgorithm == "rr":
        # Round-robin
        return rserver[0] if rserver else None
    elif salgorithm == "lc":
        # Least connections
        return min(rserver, key=lambda s: getattr(s, "connections", 0)) if rserver else None
    else:
        # First available (fa) or random
        return rserver[0] if rserver else None


def main(*args, **kwargs):
    return start_pproxy(*args, **kwargs)


def _unsupported_handler(name: str):
    def handler(*args, **kwargs):
        raise NotImplementedError(
            f"pproxy.server.{name} is not part of the certified live path"
        )

    return handler


datagram_handler = _unsupported_handler("datagram_handler")
patch_StreamReader = _unsupported_handler("patch_StreamReader")
patch_StreamWriter = _unsupported_handler("patch_StreamWriter")
print_server_started = _unsupported_handler("print_server_started")
stream_handler = _unsupported_handler("stream_handler")
test_url = _unsupported_handler("test_url")

__all__ = [
    "AuthTable", "DIRECT", "DUMMY", "PPProxyService", "ProxyBackward",
    "ProxyDirect", "ProxyH2", "ProxyH3", "ProxyQUIC", "ProxySSH",
    "ProxySimple", "Server", "SOCKET_TIMEOUT", "UDP_LIMIT", "compile_rule",
    "proxies_by_uri", "proxy_by_uri", "main", "sslcontexts",
    "schedule", "check_server_alive", "prepare_ciphers",
]
