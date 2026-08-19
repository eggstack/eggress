# eggress-core

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Core types, traits, and infrastructure shared across all eggress crates: relay abstractions, stream boundaries, error types, and protocol-level building blocks.

## When to use this crate

Use `eggress-core` directly when implementing a custom protocol handler or transport for eggress. Most users depend on this crate transitively through `eggress-server` or `eggress-embed`.

## Quick example

```rust
use eggress_core::{Relay, RelayDirection};

// Relay traits are implemented by protocol handlers
// to shuttle bytes between client and upstream.
```

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Architecture](https://github.com/eggstack/eggress/blob/main/docs/ARCHITECTURE.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
