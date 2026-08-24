# Import Strategy

## Canonical import

```python
import eggress
```

This loads the `eggress` package and its native extension (`_eggress`). No
services are started, no ports are bound, and no logging is initialized at
import time.

## pproxy compatibility imports

```python
from eggress import pproxy

# or

import eggress.pproxy
```

The `eggress.pproxy` module provides translation helpers (`translate_pproxy_args`,
`translate_pproxy_uri`, `check_pproxy_args`, `describe_reverse_pproxy_uri`).

To start a service from pproxy-style arguments:

```python
from eggress.pproxy import Server
from eggress import start_pproxy
```

## Top-level `import pproxy`

The optional `eggress-pproxy-compat` distribution installs a real, bounded
`pproxy` package. It provides the documented public factories and the `proto`,
`server`, and `cipher` modules. The canonical `eggress` wheel does not install
this top-level namespace.

- It is a replacement namespace, not a private-internals clone.
- It keeps the published canonical distribution name `eggress` separate from
  the opt-in compatibility distribution.

Installing upstream `pproxy` and Eggress together is unsupported because both
distributions provide the same namespace. Uninstall upstream pproxy first.
Code using only translation helpers may continue using `from eggress import
pproxy`.

## Replacement behavior

In a clean Eggress environment:

```python
import pproxy
proxy = pproxy.Connection("socks5://proxy:1080")
```

The top-level adapter uses Eggress's Rust-owned transport and does not copy
pproxy's private networking implementation.

## Import collision safety

- `eggress._eggress` is the only native module name. There is no top-level
  `_pproxy` or `pproxy` module installed by eggress.
- The `eggress.pproxy` submodule is a pure Python module that re-exports
  functions from `eggress._eggress`. It does not import or depend on the
  upstream `pproxy` package.
- The optional `eggress-pproxy-compat` distribution owns the top-level `pproxy`
  namespace; the canonical Eggress wheel does not install it.
- Upstream pproxy and the compatibility distribution must not be installed together.

## Import examples

```python
# Standard usage
import eggress
from eggress import EggressService, EggressConfig

# pproxy compat
from eggress import pproxy
result = pproxy.translate_pproxy_args(["-l", "socks5://:1080"])

# Convenience
from eggress import start_pproxy
with start_pproxy(["-l", "socks5://:1080"]) as handle:
    pass
```

## See also

- [INSTALLATION.md](INSTALLATION.md) — installation methods
- [PACKAGING.md](PACKAGING.md) — module structure and wheel contents
- [MIGRATION_FROM_PPROXY.md](MIGRATION_FROM_PPROXY.md) — migrating from pproxy
