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
from eggress.pproxy_connection import ProxyConnection
from eggress.protocol import UnsupportedFeatureError

SOCKET_TIMEOUT = 10.0
UDP_LIMIT = 0xFFFF
DIRECT = "direct://"
DUMMY = object()
sslcontexts = {}


def proxy_by_uri(uri: str, *args, **kwargs):
    return ProxyConnection(uri)


def proxies_by_uri(uris, *args, **kwargs):
    if isinstance(uris, str):
        uris = [uris]
    return ProxyConnection(*uris)


def compile_rule(filename: str, *args, **kwargs):
    return filename


def check_server_alive(*args, **kwargs):
    return True


def prepare_ciphers(*args, **kwargs):
    return {}


def _unsupported(name: str):
    class UnsupportedProxy:
        def __init__(self, *args, **kwargs):
            raise UnsupportedFeatureError(
                f"pproxy.server.{name} is structural compatibility only; "
                "use eggress.Server or start_pproxy()"
            )

    UnsupportedProxy.__name__ = name
    return UnsupportedProxy


AuthTable = _unsupported("AuthTable")
ProxyBackward = _unsupported("ProxyBackward")
ProxyDirect = _unsupported("ProxyDirect")
ProxyH2 = _unsupported("ProxyH2")
ProxyH3 = _unsupported("ProxyH3")
ProxyQUIC = _unsupported("ProxyQUIC")
ProxySSH = _unsupported("ProxySSH")
ProxySimple = _unsupported("ProxySimple")


def main(*args, **kwargs):
    return start_pproxy(*args, **kwargs)


def _unsupported_handler(name: str):
    def handler(*args, **kwargs):
        raise UnsupportedFeatureError(
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
]
