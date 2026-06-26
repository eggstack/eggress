# TLS Transport Layer — Completion Record

## Summary

This phase adds a shared, reusable TLS transport layer to eggress. A new crate
`eggress-transport-tls` provides TLS configuration builders, transport wrappers,
and root certificate loading. This enables any protocol (HTTP, SOCKS, Shadowsocks,
Trojan) to operate over TLS without duplicating crypto setup code.

## What was delivered

### New crate: `eggress-transport-tls`

- `TlsClientConfigBuilder` — builds `Arc<ClientConfig>` with system/custom roots, ALPN, insecure mode, server name override
- `TlsServerConfigBuilder` — builds `Arc<ServerConfig>` from PEM cert/key, ALPN
- `tls_connect` / `tls_accept` — wraps `BoxStream` in TLS
- `load_system_roots` / `load_pem_roots` / `load_pem_certs` — root certificate loading
- `TlsError` — structured error type
- 17 unit tests

### Config model extensions

- `ListenerTlsConfig` — cert, key, alpn fields on listeners
- `ProxyHopSpec.tls: bool` — marks hops for TLS wrapping
- `ProxyHopSpec.server_name: Option<String>` — SNI override
- `+tls` URI suffix — `socks5+tls://proxy.example:1080`
- Config validation: PEM parsed at startup, missing files rejected
- 4 config tests (accepted, missing cert, invalid PEM, +tls URI)

### Listener TLS integration

- `CompiledListenerTlsConfig` stores raw PEM data
- Supervisor accept loop wraps stream in TLS when `tls_config` is present
- TLS handshake failure logged and connection dropped cleanly
- 4 integration tests (accepts HTTPS, rejects plaintext, mixed listeners, wrong server name)

### Upstream TLS integration

- `TlsWrapper` type on `ChainExecutor` — applies TLS before hop handshakes
- `hop.tls == true` triggers TLS wrapping using `hop.server_name` or endpoint host
- Default wrapper uses system root certificates
- 5 chain executor tests (wrapper called/not called, server name, fallback, failure propagation)

### Trojan refactor

- `trojan_connect` now accepts optional `Arc<ClientConfig>` parameter
- Falls back to system roots via `TlsClientConfigBuilder` when `None` (backward compatible)
- Removed `webpki-roots` direct dependency from `eggress-protocol-trojan`
- Chain executor builds shared `Arc<ClientConfig>` and passes it to Trojan

## Test coverage

| Area | Tests |
|------|-------|
| Transport TLS (builders, roots, transport) | 17 |
| Chain executor TLS wrapping | 5 |
| Config TLS parsing and validation | 4 |
| Runtime TLS listener integration | 4 |
| **Total new TLS tests** | **30** |

Total workspace tests: 1000 (up from 967).

## What's deferred

- Certificate reload / rotation
- Mutual TLS (client certificates)
- HTTP/2 ALPN negotiation
- QUIC/TLS 1.3 over UDP

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
```

All clean. 1000 tests pass, 0 clippy warnings, 0 deny violations.
