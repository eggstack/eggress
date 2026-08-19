# eggress-python

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

PyO3 bindings crate producing the `_eggress` Python extension module. Not typically consumed directly — install via PyPI: `pip install eggress`.

## When to use this crate

This crate is built by maturin to produce the Python wheel. Python users should install the `eggress` package from PyPI rather than depending on this crate directly.

## Feature flags

- `ssh` — Enable SSH upstream transport in the Python bindings.
- `quic` — Enable QUIC/H3 transport in the Python bindings.
- `legacy-crypto` — Enable legacy Shadowsocks ciphers in the Python bindings.
- `pproxy-daemon` — Enable pproxy daemon mode in the Python bindings.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Python bindings](https://github.com/eggstack/eggress/blob/main/docs/PYTHON_BINDINGS.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
