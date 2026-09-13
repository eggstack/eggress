# eggress-relay

> Part of [eggress](https://github.com/eggstack/eggress) — a Rust-native, embeddable, multi-protocol proxy framework targeting compatibility with Python `pproxy==2.7.9`.

Generic asynchronous bidirectional stream relay with an explicit half-close
policy. It copies bytes between two Tokio-compatible duplex streams
(`AsyncRead + AsyncWrite + Unpin`) with directional byte counts and
directional I/O errors.

## When to use this crate

Use `eggress-relay` when you only need to shuttle bytes between two
already-connected streams and do not want Eggress routing, URI parsing, TLS,
listeners, runtime, metrics, or protocol handling:

```rust
use std::num::NonZeroUsize;
use std::time::Duration;
use eggress_relay::{relay_with_options, HalfClosePolicy, RelayOptions};

async fn bridge<C, S>(client: C, server: S) -> std::io::Result<u64>
where
    C: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let options = RelayOptions {
        buffer_size: NonZeroUsize::new(65536).unwrap(),
        half_close: HalfClosePolicy::Drain,
    };
    match relay_with_options(client, server, options).await {
        Ok(report) => Ok(report.bytes_upstream + report.bytes_downstream),
        Err(failure) => Err(failure.source),
    }
}
```

The default policy (`HalfClosePolicy::Drain`) waits for the opposite
direction without an extra deadline, so a slow response after a client
half-close is delivered. Choose
`HalfClosePolicy::DrainFor(Duration::from_secs(1))` when a silent peer must
not hold the relay open.

This crate is intentionally not a proxy runtime: no protocols, routing,
`TargetAddr`, TLS/SSH/QUIC, URI parsing, configuration, listeners, metrics,
or pproxy compatibility live here. For a full embeddable proxy service, use
`eggress-embed`; `eggress-core::relay` remains the Eggress-internal
compatibility facade (64 KiB buffers, one-second bounded drain, legacy
collapsed `TerminationReason::Error`).

## Documentation

- [Workspace README](https://github.com/eggstack/eggress/blob/main/README.md)
- [Architecture](https://github.com/eggstack/eggress/blob/main/architecture/relay.md)
- [Release process](https://github.com/eggstack/eggress/blob/main/docs/release/RELEASE_PROCESS.md)

## License

MIT OR Apache-2.0
