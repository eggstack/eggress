# Installation

## Package name

The package is published on PyPI as `eggress`.

## Install from PyPI

```bash
pip install eggress
```

## Install from a wheel file

```bash
pip install eggress-*.whl
```

No Rust toolchain is required when installing from a pre-built wheel.

## Install from source

Requires a Rust toolchain (stable) and [maturin](https://github.com/PyO3/maturin).

```bash
cd crates/eggress-python
maturin build --release --out ../../dist
pip install ../../dist/eggress-*.whl
```

## Supported platforms

| Platform | Architecture | Wheel |
|----------|-------------|-------|
| Linux | x86_64 | Yes |
| Linux | aarch64 | Yes |
| macOS | x86_64 | Yes |
| macOS | arm64 | Yes |
| Windows | x86_64 | Yes |

## Python version

Requires Python >= 3.9.

## Import examples

```python
import eggress

from eggress import pproxy

from eggress.pproxy import Server
```

## The `eggress.pproxy` namespace

The `eggress.pproxy` namespace provides compatibility helpers for translating
pproxy-style arguments and URIs into eggress TOML configuration. It is **not**
a drop-in replacement for `import pproxy`.

The pproxy compatibility layer exposes:

- `translate_pproxy_args(args)` — translate CLI arguments to TOML
- `translate_pproxy_uri(local, remotes)` — translate URIs to TOML
- `check_pproxy_args(args)` — alias for `translate_pproxy_args`
- `describe_reverse_pproxy_uri(uri)` — inspect a reverse pproxy URI

See [MIGRATION_FROM_PPROXY.md](MIGRATION_FROM_PPROXY.md) for migration
guidance.

## See also

- [IMPORT_STRATEGY.md](IMPORT_STRATEGY.md) — canonical import paths
- [PACKAGING.md](PACKAGING.md) — wheel build matrix and module structure
- [MIGRATION_FROM_PPROXY.md](MIGRATION_FROM_PPROXY.md) — migrating from pproxy
- [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) — release procedure
