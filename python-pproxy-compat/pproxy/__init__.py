"""Top-level pproxy compatibility namespace backed by eggress.

The namespace is supplied by a separate distribution so installing the
canonical ``eggress`` package alone never shadows upstream ``pproxy``.
"""

from __future__ import annotations

import eggress as _eggress
from eggress._pproxy_proxy import DIRECT as _DIRECT_INSTANCE

from . import cipher, proto, server
from .server import proxy_by_uri, proxies_by_uri, compile_rule

__eggress_compat__ = True
__eggress_version__ = _eggress.__version__
__pproxy_compatibility_version__ = "2.7.9"
__version__ = __eggress_version__

# In pproxy 2.7.9, Connection and Server are both aliases for proxies_by_uri.
# They are functions, not classes.
Connection = proxies_by_uri
Server = proxies_by_uri

# In pproxy 2.7.9, DIRECT is a ProxyDirect() instance (singleton).
DIRECT = _DIRECT_INSTANCE

# Rule is an alias for compile_rule (a function, not a class).
Rule = compile_rule


__all__ = [
    "Connection",
    "DIRECT",
    "Rule",
    "Server",
    "proto",
    "cipher",
    "server",
]
