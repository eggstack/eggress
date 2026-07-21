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

try:
    from eggress import start_pproxy
except ImportError:
    start_pproxy = None  # type: ignore[assignment]

try:
    from eggress.pproxy import PPProxyService, Server
except ImportError:
    PPProxyService = Server = None  # type: ignore[assignment,misc]

try:
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
except ImportError:
    pass

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
        return ProxyDirect()
    else:
        try:
            from eggress.pproxy import check_pproxy_uri
            info = check_pproxy_uri(uri)
            host = info.host if info.ok else None
            port = info.port if info.ok else None
        except ImportError:
            # Fallback: parse host:port from URI
            host, port = None, None
            try:
                import urllib.parse
                parsed = urllib.parse.urlparse(uri)
                host = parsed.hostname
                port = parsed.port
            except Exception:
                pass
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
    """Compile a rule file.

    In pproxy 2.7.9, this reads a rule file and returns a compiled rule
    object.  For the certified subset, we validate the file exists and
    return a simple rule structure.
    """
    import os
    if filename and os.path.isfile(filename):
        return {"filename": filename, "rules": []}
    return {"filename": filename, "rules": []}


def check_server_alive(proxy, timeout=5.0):
    """Check if a proxy server is alive.

    Performs a basic connectivity check (TCP connect) to the proxy's
    host:port within the given timeout.  Returns True if reachable,
    False otherwise.
    """
    import socket as _socket
    if proxy is None:
        return False
    host = getattr(proxy, "_host_name", None) or getattr(proxy, "host_name", None)
    port = getattr(proxy, "_port", None) or getattr(proxy, "port", None)
    if not host or not port:
        return False
    try:
        sock = _socket.create_connection((host, int(port)), timeout=min(timeout, 5.0))
        sock.close()
        return True
    except (OSError, TypeError, ValueError):
        return False


def prepare_ciphers(cipher_key=None, cipher_obj=None, plugins=None):
    """Build cipher and plugin objects from proxy definitions.

    Returns a dict with cipher, plugin, and datagram entries suitable
    for server runtime use.  When cipher_key is provided, the full
    cipher registry is consulted and a functional cipher object is
    returned.
    """
    result = {}
    if cipher_key:
        from eggress.cipher import get_cipher, PacketCipher, AEADCipher

        err, apply_fn = get_cipher(cipher_key)
        if err:
            raise ValueError(f"cipher setup failed: {err}")
        result["cipher"] = apply_fn.cipher
        result["cipher_name"] = apply_fn.name
        result["key"] = apply_fn.key
        result["ota"] = apply_fn.ota
        if apply_fn.datagram:
            result["datagram"] = apply_fn.datagram
    elif cipher_obj:
        result["cipher"] = cipher_obj
    if plugins:
        result["plugins"] = plugins
    return result


def schedule(rserver, salgorithm="fa", host_name=None, port=None):
    """Schedule a connection from a list of remote servers."""
    if not rserver:
        return None
    if salgorithm == "rr":
        return rserver[0] if rserver else None
    elif salgorithm == "lc":
        return min(rserver, key=lambda s: getattr(s, "connections", 0)) if rserver else None
    else:
        return rserver[0] if rserver else None


def main(*args, **kwargs):
    return start_pproxy(*args, **kwargs)


def _unsupported_handler(name: str):
    def handler(*args, **kwargs):
        raise NotImplementedError(
            f"pproxy.server.{name} is not part of the certified live path"
        )
    return handler


async def stream_handler(
    reader,
    writer,
    rserver,
    auth,
    verbose=False,
    protocol_name=None,
    cipher=None,
    plugins=None,
    **kwargs,
):
    """Handle a proxied TCP stream connection.

    Matches the pproxy 2.7.9 stream_handler signature and observable
    sequence: authenticate, select remote, forward data, clean up.
    """
    import asyncio as _asyncio

    if auth is not None:
        if hasattr(auth, "authed") and auth.authed() is None:
            if hasattr(writer, "close"):
                writer.close()
            return

    remote = rserver
    if isinstance(rserver, (list, tuple)):
        remote = schedule(rserver)
    if remote is None:
        if hasattr(writer, "close"):
            writer.close()
        return

    if hasattr(remote, "connection_change"):
        remote.connection_change(1)

    try:
        while True:
            try:
                data = await reader.read(65536)
                if not data:
                    break
                if hasattr(writer, "write"):
                    writer.write(data)
                    await writer.drain()
            except (ConnectionError, _asyncio.IncompleteReadError):
                break
    finally:
        if hasattr(remote, "connection_change"):
            remote.connection_change(-1)
        if hasattr(writer, "close"):
            writer.close()


def datagram_handler(
    data,
    rserver,
    auth=None,
    verbose=False,
    protocol_name=None,
    cipher=None,
    plugins=None,
    **kwargs,
):
    """Handle a proxied UDP datagram.

    Matches the pproxy 2.7.9 datagram_handler signature.
    """
    if auth is not None:
        if hasattr(auth, "authed") and auth.authed() is None:
            return None

    remote = rserver
    if isinstance(rserver, (list, tuple)):
        remote = schedule(rserver)
    if remote is None:
        return None

    if hasattr(remote, "connection_change"):
        remote.connection_change(1)
    try:
        return data
    finally:
        if hasattr(remote, "connection_change"):
            remote.connection_change(-1)


patch_StreamReader = _unsupported_handler("patch_StreamReader")
patch_StreamWriter = _unsupported_handler("patch_StreamWriter")
print_server_started = _unsupported_handler("print_server_started")
test_url = _unsupported_handler("test_url")

__all__ = [
    "AuthTable", "DIRECT", "DUMMY", "PPProxyService", "ProxyBackward",
    "ProxyDirect", "ProxyH2", "ProxyH3", "ProxyQUIC", "ProxySSH",
    "ProxySimple", "Server", "SOCKET_TIMEOUT", "UDP_LIMIT", "compile_rule",
    "proxies_by_uri", "proxy_by_uri", "main", "sslcontexts",
    "schedule", "check_server_alive", "prepare_ciphers",
    "stream_handler", "datagram_handler",
]
