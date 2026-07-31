"""Bounded, Rust-backed compatibility package for ``pproxy==2.7.9``.

The distribution remains named ``eggress``.  This package intentionally owns
the import namespace when Eggress is installed; do not install it alongside
the upstream ``pproxy`` distribution in one environment.
"""

from eggress import __version__
from . import cipher, plugin, proto, server
from .server import DIRECT, Rule, Server, Connection

__all__ = ["Connection", "DIRECT", "Rule", "Server", "cipher", "plugin", "proto", "server"]
