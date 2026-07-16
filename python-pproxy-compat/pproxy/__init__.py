"""Top-level pproxy compatibility namespace backed by eggress.

The namespace is supplied by a separate distribution so installing the
canonical ``eggress`` package alone never shadows upstream ``pproxy``.
"""

from __future__ import annotations

import eggress as _eggress
from eggress.pproxy import Server as _Server
from eggress.pproxy_connection import ProxyConnection as Connection

from . import cipher, proto, server

__eggress_compat__ = True
__eggress_version__ = _eggress.__version__
__pproxy_compatibility_version__ = "2.7.9"
__version__ = __eggress_version__

Server = _Server


class _DirectSentinel:
    """Marker for code that only uses pproxy's direct sentinel identity."""

    name = "direct"

    def __repr__(self) -> str:
        return "DIRECT"


DIRECT = _DirectSentinel()


class Rule:
    """Small compatibility record for rule-file based applications.

    Live routing uses Eggress TOML matchers. The record preserves construction
    and introspection for programs that pass a rule filename to a factory.
    """

    def __init__(self, filename: str | None = None) -> None:
        self.filename = filename

    def __repr__(self) -> str:
        return f"Rule({self.filename!r})"


__all__ = [
    "Connection",
    "DIRECT",
    "Rule",
    "Server",
    "proto",
    "cipher",
    "server",
]
