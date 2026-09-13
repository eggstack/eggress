# eggress-core

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Core types, traits, and infrastructure shared across all eggress crates: stream boundaries, error types, protocol-level building blocks, and the relay compatibility facade.

## When to use this crate

Use `eggress-core` directly when implementing a custom protocol handler or transport for eggress. Most users depend on this crate transitively through `eggress-server` or `eggress-embed`.

## Quick example

```rust
use eggress_core::relay::{relay, RelayResult, TerminationReason};

// Bidirectional copy between two boxed streams with historical Eggress
// behavior (64 KiB buffers, one-second bounded post-half-close drain).
// Directional I/O failures collapse to `TerminationReason::Error`.
async fn bridge(client: eggress_core::BoxStream, server: eggress_core::BoxStream) -> RelayResult {
    relay(client, server).await
}
```

For raw Tokio duplex streams outside the Eggress `BoxStream` architecture —
generic `AsyncRead + AsyncWrite + Unpin` with an explicit half-close policy
and rich directional errors — use `eggress-relay` directly instead.

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Architecture](https://github.com/eggstack/eggress/blob/main/docs/ARCHITECTURE.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
