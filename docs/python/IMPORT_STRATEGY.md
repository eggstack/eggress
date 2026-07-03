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

## No `import pproxy` shim

eggress does **not** install a `pproxy` module or register a top-level `pproxy`
namespace. This is a deliberate decision:

- It avoids shadowing or conflicting with the upstream `pproxy` package if
  both are installed.
- It makes the import path explicit — users see `eggress.pproxy`, not a
  fake `pproxy`.
- It prevents accidental dependency on eggress when code expects upstream
  pproxy behavior.

See the ADR at `docs/adr/` for the full rationale.

## Coexistence with upstream pproxy

Both packages can coexist in the same environment:

```python
import pproxy        # upstream pproxy (if installed)
import eggress       # eggress bindings
from eggress import pproxy as eggress_pproxy  # eggress pproxy compat layer
```

The `eggress.pproxy` namespace does not depend on or interact with the upstream
`pproxy` package. Translation is implemented entirely in Rust via
`eggress-pproxy-compat`.

## Import collision safety

- `eggress._eggress` is the only native module name. There is no top-level
  `_pproxy` or `pproxy` module installed by eggress.
- The `eggress.pproxy` submodule is a pure Python module that re-exports
  functions from `eggress._eggress`. It does not import or depend on the
  upstream `pproxy` package.
- If both `pproxy` and `eggress` are installed, they occupy separate
  namespaces and do not interfere.

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
