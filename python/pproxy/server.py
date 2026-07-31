"""pproxy server/proxy factories backed by Eggress protocol adapters."""

from __future__ import annotations

import argparse
import random
import re

from eggress._pproxy_proxy import (
    AuthTable, DIRECT, ProxyBackward, ProxyDirect, ProxyH2, ProxyH3,
    ProxyQUIC, ProxySSH, ProxySimple,
)
from eggress.cipher import get_cipher
from eggress.protocol import get_protos, netloc_split

SOCKET_TIMEOUT = 60
UDP_LIMIT = 64
DUMMY = object()


def compile_rule(filename):
    if filename.startswith("{") and filename.endswith("}"):
        return re.compile(filename[1:-1]).match
    with open(filename, encoding="utf-8") as handle:
        patterns = [
            line.strip()
            for line in handle
            if line.strip() and not line.startswith("#")
        ]
    if not patterns:
        return re.compile(r"(?!)").match
    return re.compile("(:?" + "|".join(patterns) + ")$").match


def schedule(rserver, salgorithm, host_name, port):
    candidates = [p for p in rserver if p.alive and p.match_rule(host_name, port)]
    if salgorithm == "fa":
        return candidates[0] if candidates else None
    if salgorithm == "rr":
        if not candidates:
            return None
        selected = candidates[0]
        rserver.remove(selected)
        rserver.append(selected)
        return selected
    if salgorithm == "rc":
        return random.choice(candidates) if candidates else None
    if salgorithm == "lc":
        return min(candidates, key=lambda p: p.connections, default=None)
    raise ValueError(f"Unknown scheduling algorithm: {salgorithm}")


def _proxy_by_uri(uri, jump):
    scheme, _, rest = uri.partition("://")
    if not _:
        raise argparse.ArgumentTypeError(f"invalid URI: {uri!r}")
    url = __import__("urllib.parse", fromlist=["urlparse"]).urlparse("s://" + rest)
    raw = [part.lower() for part in scheme.split("+")]
    err, protos = get_protos(raw)
    if err:
        raise argparse.ArgumentTypeError(err)
    path, _, _plugins = url.path.partition(",")
    path, _, lbind = path.partition("@")
    cipher, _, location = url.netloc.rpartition("@")
    host, port = (
        netloc_split(location, default_port=22 if "ssh" in raw else 8080)
        if location
        else (None, None)
    )
    auth = url.fragment.encode() if url.fragment else None
    users = [line for line in auth.split(b"\n")] if auth else None
    cipher_apply = None
    if cipher:
        error, cipher_apply = get_cipher(cipher)
        if error:
            raise argparse.ArgumentTypeError(error)
    if "direct" in [p.name for p in protos]:
        return ProxyDirect(lbind=lbind or None)
    params = dict(
        jump=jump,
        protos=protos,
        cipher=cipher_apply,
        users=users,
        rule=url.query or None,
        bind=location or path,
        host_name=host,
        port=port,
        unix=not location,
        lbind=lbind or None,
    )
    if "h2" in raw:
        return ProxyH2(**params)
    if "ssh" in raw:
        return ProxySSH(**params)
    if "quic" in raw:
        return ProxyQUIC(**params)
    if "h3" in [p.name for p in protos]:
        return ProxyH3(**params)
    proxy = ProxySimple(**params)
    if "in" in raw:
        proxy = ProxyBackward(proxy, raw.count("in"), **params)
    return proxy


def proxy_by_uri(uri, jump=None):
    return _proxy_by_uri(uri, jump if jump is not None else DIRECT)


def proxies_by_uri(uri_jumps):
    jump = DIRECT
    for uri in reversed(uri_jumps.split("__")):
        jump = _proxy_by_uri(uri, jump)
    return jump


Connection = proxies_by_uri
Server = proxies_by_uri
Rule = compile_rule


async def check_server_alive(interval, rserver, verbose):
    import asyncio
    while True:
        await asyncio.sleep(interval)


async def prepare_ciphers(cipher, reader, writer, bind=None, server_side=True):
    if cipher is None:
        return None, None
    return reader, writer


async def datagram_handler(writer, data, addr, protos, urserver, block, cipher, salgorithm,
                           verbose=lambda *args: None, **kwargs):
    raise NotImplementedError("UDP listener handling is owned by Eggress")


async def stream_handler(reader, writer, unix, lbind, protos, rserver, cipher, sslserver,
                         debug=0, authtime=2592000, block=None, salgorithm="fa",
                         verbose=lambda *args: None, modstat=lambda *args: None, **kwargs):
    raise NotImplementedError("server stream handling is owned by Eggress")


def print_server_started(*args, **kwargs):
    return None


def test_url(*args, **kwargs):
    raise NotImplementedError("URL testing is not exposed by the Eggress adapter")

sslcontexts = []
compile_rule.__module__ = __name__

__all__ = ["AuthTable", "DIRECT", "DUMMY", "ProxyBackward", "ProxyDirect", "ProxyH2",
           "ProxyH3", "ProxyQUIC", "ProxySSH", "ProxySimple", "Rule", "Connection",
           "Server", "compile_rule", "proxy_by_uri", "proxies_by_uri", "schedule"]
