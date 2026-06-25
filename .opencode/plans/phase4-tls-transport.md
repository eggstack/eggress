# Phase 4 Plan: TLS Transport Layer

## Purpose

Phase 4 adds a shared, reusable TLS transport layer to eggress. Currently TLS
exists only inside `eggress-protocol-trojan` as a protocol-specific client
handshake. There is no general-purpose TLS connector, no TLS listener support,
no config model for certificates or verification, and no way to wrap arbitrary
protocols in TLS.

This plan creates that foundation so that any protocol (HTTP, SOCKS, Shadowsocks,
future protocols) can operate over TLS without duplicating crypto setup code.

## Non-goals (deferred)

- QUIC/TLS 1.3 over UDP (Phase 8)
- Certificate reload / rotation
- Mutual TLS (client certificates)
- HTTP/2 ALPN negotiation (Phase 7)
- TLS-wrapped transparent proxying (Phase 10)

## Current state

| Layer | State |
|-------|-------|
| Workspace deps | `rustls`/`tokio-rustls`/`webpki-roots` only in `eggress-protocol-trojan` |
| Core listener | Plain TCP only, returns `BoxStream` |
| Core connector | `DirectConnector` — TCP only |
| Chain executor | Passes `BoxStream` between hops, no transport wrapper concept |
| URI parser | No TLS variant in `ProtocolSpec`, no transport field on `ProxyHopSpec` |
| Config model | No TLS fields on `ListenerConfig` or `UpstreamConfig` |
| Trojan client | Builds fresh `ClientConfig` per connection, hardcoded webpki roots |

---

# Pre-work: README bug fixes

Two README items are already implemented but not checked off:

1. **`SOCKS5 UDP ASSOCIATE client`** (line 97) — Full implementation in
   `eggress-udp/src/upstream_socks5.rs` with extensive unit + integration tests.
   Check off.

2. **`UDP chain validation`** (line 154) — Full validation in
   `eggress-core/src/capability.rs`, `eggress-config/src/validate.rs`, and
   `eggress-udp/src/udp_capability.rs` with tests. Check off.

---

# Workstream 1: Shared TLS config builder

## Objective

Create a central place to build and cache `rustls::ClientConfig` and
`rustls::ServerConfig` from declarative configuration.

## New crate

```text
crates/eggress-transport-tls/
├── Cargo.toml
└── src/
    ├── lib.rs          # re-exports
    ├── client.rs       # ClientConfig builder
    ├── server.rs       # ServerConfig builder
    ├── roots.rs        # system/custom root loading
    └── error.rs        # TlsError
```

## Dependencies (workspace-level)

Add to root `Cargo.toml` `[workspace.dependencies]`:

```toml
rustls = "0.23"
tokio-rustls = "0.26"
webpki-roots = "0.26"
rustls-pemfile = "2"
```

`eggress-transport-tls` depends on all four plus `eggress-core`.

## Client config builder

```rust
pub struct TlsClientConfigBuilder {
    root_store: rustls::RootCertStore,
    alpn_protocols: Vec<Vec<u8>>,
    server_name_override: Option<String>,
    insecure: bool,
}

impl TlsClientConfigBuilder {
    pub fn new() -> Self;
    pub fn with_system_roots(self) -> Result<Self, TlsError>;
    pub fn with_custom_ca_pem(self, pem_bytes: &[u8]) -> Result<Self, TlsError>;
    pub fn with_alpn(self, protocols: Vec<Vec<u8>>) -> Self;
    pub fn with_insecure(self) -> Self;
    pub fn build(self) -> Result<Arc<rustls::ClientConfig>, TlsError>;
}
```

- `with_system_roots` loads `webpki_roots::TLS_SERVER_ROOTS` into the store.
- `with_custom_ca_pem` parses PEM and adds to the store.
- `with_insecure` installs a `danger::ServerCertVerifier` that accepts all certs.
- `build` produces `Arc<ClientConfig>` for sharing across connections.

## Server config builder

```rust
pub struct TlsServerConfigBuilder {
    cert_chain: Vec<CertificateDer<'static>>,
    key_der: PrivatePkcs8KeyDer<'static>,
    alpn_protocols: Vec<Vec<u8>>,
}

impl TlsServerConfigBuilder {
    pub fn new() -> Self;
    pub fn with_certificate_pem(self, cert_pem: &[u8]) -> Result<Self, TlsError>;
    pub fn with_key_pem(self, key_pem: &[u8]) -> Result<Self, TlsError>;
    pub fn with_alpn(self, protocols: Vec<Vec<u8>>) -> Self;
    pub fn build(self) -> Result<Arc<rustls::ServerConfig>, TlsError>;
}
```

## Root loading

```rust
pub fn load_system_roots() -> Result<rustls::RootCertStore, TlsError>;
pub fn load_pem_roots(pem_bytes: &[u8]) -> Result<rustls::RootCertStore, TlsError>;
```

## Tests

- Builder creates valid config from system roots.
- Builder creates valid config from custom PEM.
- Insecure mode verifier accepts self-signed cert.
- PEM parsing rejects invalid data.
- Round-trip: build server config, connect with client config, verify handshake.

---

# Workstream 2: TLS transport wrappers

## Objective

Provide `tls_connect` (client-side) and `tls_accept` (server-side) functions
that wrap a `BoxStream` in TLS, returning a new `BoxStream`.

## client.rs

```rust
pub async fn tls_connect(
    stream: BoxStream,
    config: Arc<rustls::ClientConfig>,
    server_name: &str,
) -> Result<BoxStream, TlsError>;
```

- Uses `TlsConnector::from(config)`.
- Sets SNI from `server_name`.
- Returns boxed `TlsStream<BoxStream>`.

## server.rs

```rust
pub async fn tls_accept(
    stream: BoxStream,
    config: Arc<rustls::ServerConfig>,
) -> Result<BoxStream, TlsError>;
```

- Uses `TlsAcceptor::from(config)`.
- Performs server-side TLS handshake.
- Returns boxed `TlsStream<BoxStream>`.

## Tests

- Server + client round-trip over loopback.
- Self-signed cert handshake succeeds.
- Wrong server name fails with appropriate error.
- Tampered certificate fails verification.
- Both sides produce valid `BoxStream` (can read/write after handshake).

---

# Workstream 3: Config model extensions

## Objective

Extend the TOML config model with TLS fields so operators can configure TLS
for listeners and upstreams declaratively.

## Listener TLS config

Add to `ListenerConfig` in `eggress-config/src/model.rs`:

```rust
pub struct ListenerTlsConfig {
    pub cert: String,           // path to PEM cert chain
    pub key: String,            // path to PEM private key
    pub alpn: Option<Vec<String>>,  // e.g. ["h2", "http/1.1"]
}
```

On `ListenerConfig`:
```rust
pub tls: Option<ListenerTlsConfig>,
```

## Upstream TLS config

TLS for upstreams is currently implicit in the protocol (e.g., `trojan://`
implies TLS). For explicit TLS wrapping of other protocols, add an optional
transport field to `ProxyHopSpec` in `eggress-uri/src/lib.rs`:

```rust
pub struct ProxyHopSpec {
    pub protocols: Vec<ProtocolSpec>,
    pub endpoint: EndpointSpec,
    pub credentials: Option<CredentialSpec>,
    pub rule: Option<String>,
    pub local_bind: Option<String>,
    pub tls: bool,                    // NEW: wrap this hop in TLS
    pub server_name: Option<String>,  // NEW: SNI override (defaults to endpoint host)
}
```

## TOML examples

```toml
# TLS listener
[[listeners]]
name = "https-proxy"
bind = "0.0.0.0:8443"
protocols = ["http"]
[tls]
cert = "/path/to/cert.pem"
key = "/path/to/key.pem"

# TLS-wrapped upstream
[[upstreams]]
id = "tls-socks"
uri = "socks5+tls://proxy.example:1080"

# SOCKS5 upstream with explicit TLS wrapping
[[upstreams]]
id = "socks-over-tls"
uri = "socks5://proxy.example:1080"
# TLS config in hop spec (parsed from URI +__+ syntax or TOML)
```

## URI syntax

The `+tls` suffix in protocol names indicates TLS wrapping:

```text
socks5+tls://proxy.example:1080
http+tls://proxy.example:443
```

This maps to `protocols: [Socks5]` with `tls: true` on the hop spec.

## Config compile

In `eggress-config/src/compile.rs`:
- `compile_listener_tls`: reads cert/key files, builds `Arc<ServerConfig>`.
- `compile_upstream_tls`: for hops with `tls: true`, builds `Arc<ClientConfig>`.
- Store compiled TLS configs alongside listener/upstream configs.

## Tests

- Parse TOML with listener TLS config.
- Parse URI with `+tls` suffix.
- Reject missing cert or key file at config validation time.
- Reject invalid PEM at config validation time.
- Config reload picks up new TLS cert/key.

---

# Workstream 4: Listener TLS integration

## Objective

Make `TcpListener` optionally perform TLS termination before protocol detection.

## Approach

Rather than modifying `TcpListener` directly, add a `TlsAcceptorLayer` that
wraps the accepted TCP stream:

```rust
pub async fn accept_tls_stream(
    stream: TcpStream,
    config: Arc<rustls::ServerConfig>,
) -> Result<BoxStream, TlsError> {
    let tls_acceptor = TlsAcceptor::from(config);
    let tls_stream = tls_acceptor.accept(stream).await?;
    Ok(Box::new(tls_stream))
}
```

In the supervisor's accept loop, after `TcpListener::accept()` returns a raw
`TcpStream`, check if the listener has TLS config. If so, wrap the stream
through `accept_tls_stream` before passing to protocol detection.

## Protocol detection ordering

TLS detection must happen before protocol detection:

1. TCP accept → raw `TcpStream`
2. If listener has TLS config → TLS accept → `TlsStream<TcpStream>`
3. Protocol sniffing on the unwrapped stream
4. Route to appropriate protocol handler

## Tests

- TLS listener accepts HTTPS connection.
- TLS listener rejects invalid certificate.
- Non-TLS client connecting to TLS listener gets TLS handshake error.
- Mixed TLS/non-TLS listeners on different ports.
- TLS listener with ALPN negotiation.

---

# Workstream 5: Upstream TLS integration

## Objective

Allow the chain executor to apply TLS wrapping between hops when configured.

## Approach

Add a `TlsTransportWrapper` that can be applied in the chain execution flow:

```rust
pub struct TlsTransportWrapper {
    config: Arc<rustls::ClientConfig>,
    server_name: String,
}

impl TlsTransportWrapper {
    pub async fn wrap(&self, stream: BoxStream) -> Result<BoxStream, TlsError>;
}
```

In `ChainExecutor::execute`, after connecting to the first hop and before
each protocol handshake, check if the hop has `tls: true`. If so, apply
`TlsTransportWrapper::wrap` to the stream.

The modified flow:

```text
1. DirectConnector.connect(first_hop_addr) → BoxStream (TCP)
2. For each hop:
   a. If hop.tls: apply TlsTransportWrapper → BoxStream (TLS)
   b. handler.handshake(stream, target, credentials) → BoxStream (protocol)
```

## Refactor Trojan to use shared TLS

`trojan_connect` currently builds its own `ClientConfig`. After the shared
TLS layer exists:

- `trojan_connect` accepts an `Arc<ClientConfig>` parameter.
- The chain executor builds the `ClientConfig` via `TlsClientConfigBuilder`
  and passes it to `trojan_connect`.
- Remove `webpki-roots` dependency from `eggress-protocol-trojan` (use shared).

## Tests

- TCP → TLS → SOCKS5 handshake succeeds.
- TCP → TLS → HTTP CONNECT succeeds.
- TCP → TLS → Shadowsocks succeeds.
- TCP → TLS → Trojan uses shared config (no duplicate config building).
- Wrong server name fails at TLS layer, not protocol layer.
- TLS failure is categorized as `ConnectError::TlsHandshake`.

---

# Workstream 6: Documentation

## Update

- `docs/ARCHITECTURE.md` — add TLS transport layer section
- `docs/ROADMAP.md` — mark Phase 4 items as done
- `EGGRESS_ROADMAP.md` — update Phase 4 exit criteria
- `README.md` — check off Phase 4 items as they land
- `AGENTS.md` — add `eggress-transport-tls` to project structure

## README items to check off

After all workstreams complete:

- [x] rustls server transport
- [x] System root certificates
- [x] Custom CA roots
- [x] SNI
- [x] ALPN
- [x] Secure certificate verification default
- [x] Explicit insecure compatibility mode
- [ ] Certificate reload (deferred — not in this plan)
- [x] HTTPS proxy server (via TLS listener + HTTP protocol)
- [x] HTTPS proxy client (via TLS upstream wrapping)
- [x] TLS-wrapped SOCKS
- [x] TLS-wrapped custom protocols

---

# Verification

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
```

Focused checks:

```bash
cargo test -p eggress-transport-tls
cargo test -p eggress-config tls
cargo test -p eggress-runtime tls
```

---

# Dependency graph

```text
eggress-transport-tls
    ├── eggress-core (BoxStream, ConnectError, AsyncStream)
    ├── rustls
    ├── tokio-rustls
    ├── webpki-roots
    └── rustls-pemfile

eggress-config (adds TLS model types)
    └── eggress-transport-tls (for config-time validation)

eggress-protocol-trojan (refactored)
    └── eggress-transport-tls (shared ClientConfig)

eggress-server (chain executor TLS wrapping)
    └── eggress-transport-tls (TlsTransportWrapper)

eggress-runtime (listener TLS accept)
    └── eggress-transport-tls (TlsAcceptor)
```

No circular dependencies. `eggress-transport-tls` depends only on `eggress-core`
and external crates. Downstream crates consume TLS via this single module.

---

# Commit sequence

1. **README fixes** — check off SOCKS5 UDP ASSOCIATE client + UDP chain validation
2. **Workspace deps** — add rustls/tokio-rustls/webpki-roots/rustls-pemfile to workspace
3. **TLS crate scaffold** — new `eggress-transport-tls` with builders, roots, error
4. **Transport wrappers** — `tls_connect`, `tls_accept`, round-trip tests
5. **Config model** — listener TLS fields, hop `tls: bool`, URI `+tls` parsing
6. **Config compile** — compile TLS configs, validate cert/key at startup
7. **Listener TLS** — accept-loop TLS wrapping, protocol detection after TLS
8. **Upstream TLS** — chain executor TLS wrapping, `TlsTransportWrapper`
9. **Trojan refactor** — use shared `Arc<ClientConfig>` instead of per-connection build
10. **Docs and completion** — update README, architecture, roadmap, completion doc
