# Rust Proxy Development

## When to use
Use when implementing new proxy protocols, transport wrappers, or modifying core relay/chain behavior.

## Key conventions
- Edition 2021, MSRV 1.75, `unsafe_code = "forbid"` everywhere
- Async runtime: Tokio. Errors: `thiserror`. CLI: `clap` derive.
- Streams are boxed at protocol/transport boundaries (`BoxStream`) — never propagate generic stream types
- No C deps, no OpenSSL, no `build.rs` files

## Adding a new protocol

### 1. Protocol detection
Add a `ProtocolDetector` implementation in `eggress-core/src/detect.rs`. Detectors run in order — the first match wins. Mixed-protocol listeners are the norm.

### 2. Server handler
Create the protocol module under `crates/eggress-protocol-<name>/`:
- `src/lib.rs` — module re-exports
- `src/detect.rs` — protocol detection
- `src/server.rs` — server-side handshake (accept inbound connection, produce `AcceptedSession`)
- `src/client.rs` — client-side handshake (connect to upstream, produce `BoxStream`)
- `src/error.rs` — error types

Follow the pattern in `eggress-protocol-socks/` or `eggress-protocol-http/`.

### 3. Chain integration
The chain executor in `eggress-core/src/chain.rs` folds over hops with protocol-specific handlers. You must:
- Validate chain capabilities (`UdpRelayCapability` for UDP, similar for other protocols)
- Implement the hop handler that takes a stream to the hop and produces a stream to the next target

### 4. Registration
- Add the protocol variant to `ProtocolId` enum in `eggress-core/src/detect.rs`
- Register the detector in the appropriate listener setup
- Add URI scheme handling in `eggress-uri/`

## Testing
- Unit tests in the protocol crate
- Integration tests in `crates/eggress-runtime/tests/`
- Interoperability tests in `crates/eggress-cli/tests/`
- Always run: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`

## Verification checklist
- [ ] `cargo check --workspace` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo fmt --all -- --check` clean
- [ ] No new `unsafe` code
- [ ] Credentials never logged (use redacted Display)
- [ ] Bounded parsers/handshake timeouts
