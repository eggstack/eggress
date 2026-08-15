"""Bounded, Rust-backed compatibility package for ``pproxy==2.7.9``.

The distribution remains named ``eggress``.  This package intentionally owns
the import namespace when Eggress is installed; do not install it alongside
the upstream ``pproxy`` distribution in one environment.

.. note::

   ``pproxy.Connection`` and ``pproxy.Server`` are pproxy-shaped URI
   factories (aliases for ``proxies_by_uri``).  They are NOT the native
   ``eggress.pproxy.Server`` lifecycle class.  For a Rust-backed managed
   server, use ``eggress.pproxy.Server`` or ``eggress.start_pproxy()``.
"""

from eggress import __version__
from . import __doc__, cipher, cipherpy, plugin, proto, server, sysproxy, verbose
from .server import DIRECT, Rule, Server, Connection

__all__ = [
    "Connection",
    "DIRECT",
    "Rule",
    "Server",
    "cipher",
    "cipherpy",
    "plugin",
    "proto",
    "server",
    "sysproxy",
    "verbose",
]
