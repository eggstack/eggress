# eggress-protocol-raw

`crates/eggress-protocol-raw/`

Raw TCP tunnel protocol — direct stream passthrough with no protocol overhead.

## Purpose

Raw/tunnel (`raw://`, `tunnel://`) serves as a pass-through hop in proxy chains. It performs no handshake — the prior-hop stream is returned directly.

This crate is TCP-only. The bounded pproxy-compatible UDP fixed-target mode is
implemented by [`eggress-udp`](udp.md) and is selected explicitly by the
compatibility translator; it is not a raw stream hop or general UDP tunnel.

## Usage in Chains

`RawHopHandler` is a no-op handler in the chain executor:
```rust
// RawHopHandler passes through the prior-hop stream directly
fn handshake(self, stream: BoxStream, ...) -> BoxStream {
    stream // no transformation
}
```

This enables chains like `socks5://...__raw://target:port` where the raw hop acts as a transparent TCP tunnel.

## Modules

| Module | Description |
|---|---|
| `error` | Error types |
| `tunnel` | TCP tunnel implementation |

## Dependencies

Minimal — no workspace crate dependencies.

See [overview.md](overview.md) for context.
