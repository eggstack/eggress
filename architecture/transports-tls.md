# eggress-transport-tls -- Shared rustls Layer

The only TLS implementation in the workspace (no OpenSSL anywhere). Wraps
`BoxStream`s in TLS for listener inbound, upstream outbound, and Trojan.

## Module map

| File | Role |
|---|---|
| `src/client.rs` | `TlsClientConfigBuilder`: system/custom CA PEM, ALPN, insecure mode, server-name override, `InsecureVerifier` (test/feature-gated), process-shared default verified/H2 accessors |
| `src/server.rs` | `TlsServerConfigBuilder`: cert chain + key PEM (PKCS#8), ALPN (PEM loaders are private helpers, not exported) |
| `src/roots.rs` | `load_system_roots` (webpki-roots), `load_pem_roots` (PEM -> RootCertStore), `load_pem_certs` (PEM -> Vec<CertificateDer>). Empty PEM is an error in `load_pem_roots` |
| `src/transport.rs` | `tls_connect(stream, config, server_name)` / `tls_accept(stream, config)`: BoxStream in, TLS-wrapped BoxStream out |
| `src/lib.rs` | Re-exports, `install_default_crypto_provider()` (ring, once), test helper `self_signed_cert()` |
| `src/error.rs` | `TlsError` enum |

## Public API surface

### Client (`TlsClientConfigBuilder`)

| Method | Notes |
|---|---|
| `new()` | Empty root store, no ALPN, no override, not insecure |
| `with_system_roots()` | Extends root store from `webpki_roots::TLS_SERVER_ROOTS` |
| `with_custom_ca_pem(pem_bytes)` | Replaces root store with parsed PEM CA certs |
| `with_client_cert_pem(cert_pem, key_pem)` | mTLS client identity (both required; malformed PEM fails at `build`) |
| `with_alpn(protocols)` | Sets ALPN protocol list (e.g., `b"h2"`, `b"http/1.1"`) |
| `with_h2_alpn()` | Shortcut: `vec![b"h2", b"http/1.1"]` |
| `with_server_name_override(name)` | Default SNI when `tls_connect` has no explicit name |
| `with_insecure()` | Accepts any server cert. **Gated**: `#[cfg(any(test, feature = "insecure-tls"))]` |
| `build()` | Returns `Arc<ClientConfig>`. Insecure mode uses `InsecureVerifier`; if feature not enabled, returns `TlsError::Handshake` |

### ALPN-preserving adapter

| Function | Notes |
|---|---|
| `client_config_with_alpn(&Arc<ClientConfig>, Option<Vec<Vec<u8>>>) -> Arc<ClientConfig>` | Returns the same `Arc` when no ALPN change is needed (no allocation). Otherwise clones the underlying `ClientConfig` via `ClientConfig::clone()` and only mutates `alpn_protocols`. Every other field — trust roots, custom CA store, mTLS client identity, custom verifier — is preserved by `ClientConfig::clone()` and is therefore retained across the adaptation. This is the single authority for ALPN adaptation of an existing `ClientConfig`; callers must never rebuild a fresh system-roots configuration as a fallback when an override is already present. |

### Server (`TlsServerConfigBuilder`)

| Method | Notes |
|---|---|
| `new()` | Empty cert chain, no key, no ALPN |
| `with_certificate_pem(cert_pem)` | Parses PEM cert chain; fails if empty |
| `with_key_pem(key_pem)` | Parses PKCS#8 private key from PEM |
| `with_client_ca_pem(ca_pem)` | mTLS trust roots for client certs (verified when presented) |
| `with_require_client_cert(bool)` | Require a valid client cert; fails at `build` without client CA |
| `with_alpn(protocols)` | Sets ALPN protocol list |
| `with_h2_alpn()` | Shortcut: `vec![b"h2", b"http/1.1"]` |
| `build()` | Returns `Arc<ServerConfig>`. Fails if missing key or empty cert chain |

### Roots (`roots.rs`)

| Function | Notes |
|---|---|
| `load_system_roots()` | Returns `RootCertStore` from webpki-roots |
| `load_pem_roots(pem)` | Parses PEM, adds each cert to `RootCertStore`. **Empty input is an error** (returns `PemParse`) |
| `load_pem_certs(pem)` | Returns `Vec<CertificateDer>` from PEM. Does not fail on empty input (unlike `load_pem_roots`) |

### Transport (`transport.rs`)

| Function | Signature | Notes |
|---|---|---|
| `tls_connect` | `(BoxStream, Arc<ClientConfig>, &str) -> Result<BoxStream, TlsError>` | Client-side handshake; `server_name` parsed into `ServerName` |
| `tls_accept` | `(BoxStream, Arc<ServerConfig>) -> Result<BoxStream, TlsError>` | Server-side handshake |

### Error (`TlsError`)

| Variant | When |
|---|---|
| `Handshake(msg)` | TLS handshake failed, or insecure mode without feature |
| `PemParse(msg)` | PEM decoding failed |
| `NoCertificatesFound` | Empty cert chain in server builder |
| `NoPrivateKeyFound` | No PKCS#8 key in PEM data |
| `MissingPrivateKey` | `build()` called without key |
| `MissingCertificateChain` | `build()` called with empty cert chain |
| `RootStore(msg)` | `add()` failed on `RootCertStore` |
| `InvalidServerName(name)` | SNI parse failed |
| `Io(e)` | Underlying I/O error |

`From<rustls::Error>` is implemented, mapping to `Handshake(e.to_string())`.

## How it works

### Crypto provider installation

`install_default_crypto_provider()` (`lib.rs:17-24`) calls `rustls::crypto::ring::default_provider().install_default()`. The first call succeeds; subsequent calls log a warning via `tracing::warn!` and return `()` (unit — safe to call multiple times).

### Client config construction

The ordinary system-root configurations are process-shared through `OnceLock`:
`default_client_config()` and `default_h2_client_config()` return cloned
`Arc`s to immutable verified configurations. Feature-gated insecure and H2
variants use separate caches. Custom CA, mTLS, ALPN, and caller-provided
overrides continue through the builder and are never inserted into these
caches.

1. `TlsClientConfigBuilder::build()` branches on `self.insecure`:
   - **Insecure** (gated on `test || feature = "insecure-tls"`): Uses `ClientConfig::builder().dangerous().with_custom_certificate_verifier(InsecureVerifier)`. The `InsecureVerifier` accepts any certificate and any handshake signature without validation.
   - **Secure**: Uses `ClientConfig::builder().with_root_certificates(self.root_store).with_no_client_auth()`.
2. ALPN protocols are set on the resulting config.
3. The config is wrapped in `Arc` and returned.

### ALPN adaptation of an existing `ClientConfig`

`client_config_with_alpn(&Arc<ClientConfig>, Option<Vec<Vec<u8>>>) -> Arc<ClientConfig>`
returns the same `Arc` when no ALPN change is needed (no allocation), and
otherwise clones the underlying `rustls::ClientConfig` via
`ClientConfig::clone()` and only mutates `alpn_protocols`. Every other
field — trust roots, custom CA store, mTLS client identity, custom verifier,
signature schemes, resumption — is preserved by `ClientConfig::clone()` and
is therefore retained across the adaptation.

This is the single internal authority for adapting an existing
`rustls::ClientConfig` to a new ALPN list. Callers must never build a fresh
system-roots configuration as a fallback when a caller-supplied `Arc<ClientConfig>`
is already present, because doing so discards the caller's trust/identity
policy. The outbound TLS wrapper (`crates/eggress-outbound/src/executor.rs`)
uses this helper to apply H2 ALPN to an executor's shared `tls_override`.

### Server config construction

1. `TlsServerConfigBuilder::build()` checks that `key_der` is `Some` and `cert_chain` is non-empty.
2. With a client CA it uses `with_client_cert_verifier(verifier)` (`WebPkiClientVerifier`, `server.rs:92-111`); `ServerConfig::builder().with_no_client_auth().with_single_cert(chain, key)` is only the no-CA path (`server.rs:107-111`).
3. ALPN protocols are set on the resulting config.
4. The config is wrapped in `Arc` and returned.

### TLS handshake wrapping

Both `tls_connect` and `tls_accept` use `tokio-rustls`:

- `tls_connect`: `TlsConnector::from(config).connect(domain, stream)` -- performs client-side TLS handshake over the boxed stream. Returns a `TlsStream<BoxStream>` re-boxed as `BoxStream`.
- `tls_accept`: `TlsAcceptor::from(config).accept(stream)` -- performs server-side TLS handshake. Returns a `TlsStream<BoxStream>` re-boxed as `BoxStream`.

## Consumers

| Consumer | Usage |
|---|---|
| `eggress-outbound` (`executor.rs`) | Upstream `+tls` hops: builds `TlsClientConfigBuilder` with system roots or custom CA, calls `tls_connect` on the box stream |
| `eggress-runtime` (`supervisor/connection.rs`) | Listener TLS: prepares one `Arc<ServerConfig>` per listener generation, then `wrap_tls_server()` only calls `tls_accept` on inbound streams (shared by standard/transparent/Unix paths) |
| `eggress-protocol-trojan` (`tcp.rs`) | Trojan client: builds `TlsClientConfigBuilder` with system roots, then performs the handshake directly via `tokio_rustls::TlsConnector` (`tcp.rs:258-266`; does not call `tls_connect`) |
| `eggress-protocol-reverse` (`tls.rs`, `server.rs`, `client.rs`) | Native reverse control TLS/mTLS: server builds once via `TlsServerConfigBuilder` (+ optional client CA/require), client builds once via `TlsClientConfigBuilder` (+ optional client cert/key) and reuses `Arc` across reconnects; `tls_accept`/`tls_connect` wrap control TCP before reverse framing |

## Security notes

- **Insecure mode is dual-gated.** `with_insecure()` is only available under `#[cfg(any(test, feature = "insecure-tls"))]`. If the feature is not enabled, `build()` returns `TlsError::Handshake("insecure TLS requires the insecure-tls feature")`. This prevents accidental use in release builds.
- **Empty PEM is an error.** `load_pem_roots(b"")` returns `PemParse("no certificates found in PEM root material")`. This prevents silently trusting everything when CA material is misconfigured.
- **Client auth defaults to none; mTLS is opt-in.** See above for the explicit builders.
- **PKCS#8 only.** `load_private_key_pem` uses `PrivatePkcs8KeyDer::from_pem_slice`. Other key formats (RSA, EC) are not supported.
- **Ring provider only.** The crypto provider is hardcoded to ring. No alternative providers are supported.
- **No session resumption.** The builder does not configure session tickets or session caching.
- **mTLS is explicit.** Server `require_client_cert` without client CA fails at `build`; client cert without key fails at `build`. Reverse `pproxy_compat` + TLS is rejected at config compile (wire must stay plaintext).
- **ALPN adaptation preserves caller policy.** `client_config_with_alpn` clones the underlying `rustls::ClientConfig` via `ClientConfig::clone()` and only mutates `alpn_protocols`. Custom CA stores, mTLS client identity, custom verifiers, and other caller-supplied `ClientConfig` state are preserved by `ClientConfig::clone()` and survive the adaptation. Callers must never rebuild a fresh system-roots configuration as a fallback when an override is already present; the outbound TLS wrapper in `eggress-outbound` enforces this invariant.

## Concurrency and lifecycle

- Both `Arc<ClientConfig>` and `Arc<ServerConfig>` are shared across connections. The `Arc` wrapping means config construction is one-time; the resulting configs are immutable and safe to share across tasks.
- `install_default_crypto_provider()` is process-global. Calling it concurrently from multiple tasks is safe (the `install_default` method on ring is internally synchronized).
- `tls_connect` and `tls_accept` are async and take ownership of the `BoxStream`. The returned `TlsStream<BoxStream>` is `Unpin + Send` and can be held across `.await` points.

## Test coverage map

### Unit tests (`client.rs`)

| Test | What it covers |
|---|---|
| `builder_default` | Empty root store, no ALPN, not insecure |
| `builder_system_roots` | System roots loaded successfully |
| `builder_insecure` | Insecure mode builds config |
| `builder_with_server_name_override` | Override stored and retrievable |
| `builder_with_custom_ca_pem` | Custom CA PEM parsed into root store |
| `builder_with_alpn` | ALPN protocols set correctly |
| `insecure_connects_to_self_signed_server` | End-to-end: self-signed cert + insecure client = successful TLS echo |
| `default_verified_configs_are_shared_and_h2_is_distinct` | Process-shared default verified configs; H2 config is distinct (`client.rs:420`) |
| `default_insecure_configs_are_shared_and_isolated_from_verified` | Shared insecure defaults isolated from verified ones; requires `--features insecure-tls` (`client.rs:437`) |
| `client_config_with_alpn_returns_same_arc_when_alpn_unchanged` | No-allocation fast path: same `Arc` returned when ALPN matches or is `None` |
| `client_config_with_alpn_clones_when_alpn_differs` | Differing ALPN list produces a new `Arc`; original ALPN list on the input `Arc` is preserved |
| `client_config_with_alpn_preserves_trust_policy` | Custom-CA-backed `ClientConfig` clones its trust store across ALPN adaptation via `ClientConfig::clone()` |
| `client_config_with_alpn_preserves_mtls_identity` | mTLS client identity is structurally preserved by `ClientConfig::clone()` through ALPN adaptation |

### Unit tests (`server.rs`)

| Test | What it covers |
|---|---|
| `builder_default` | Empty state |
| `builder_missing_key_fails` | `build()` without key returns `MissingPrivateKey` |
| `builder_round_trip` | Self-signed cert + key builds successfully |

### Unit tests (`roots.rs`)

| Test | What it covers |
|---|---|
| `system_roots_not_empty` | System root store has certificates |
| `load_pem_roots_round_trip` | Self-signed cert loaded as root |
| `load_pem_roots_invalid_data` | Non-PEM input returns `PemParse` |
| `load_pem_roots_empty_input_is_error` | Empty PEM is rejected |
| `load_pem_certs_round_trip` | PEM parsed to `CertificateDer` list |

### Unit tests (`transport.rs`)

| Test | What it covers |
|---|---|
| `round_trip_tls_handshake` | Full client-server TLS echo |
| `wrong_server_name_fails` | SNI mismatch + untrusted cert = error |
| `plaintext_to_tls_server_fails` | Non-TLS data to TLS server = error |

## Reviewer gotchas

- **`insecure-tls` feature escape hatch.** The `with_insecure()` method and `InsecureVerifier` are compiled only when `test || feature = "insecure-tls"`. Auditing trust paths requires grepping for `insecure-tls` in `Cargo.toml` files.
- **`load_pem_certs` vs `load_pem_roots`.** `load_pem_certs` returns raw `CertificateDer` values and does NOT fail on empty input. `load_pem_roots` builds a `RootCertStore` and DOES fail on empty input. These have different error semantics for the same "empty PEM" case.
- **`with_custom_ca_pem` replaces, not extends.** It sets `builder.root_store = roots`, discarding any previously loaded roots (including system roots). Call `with_system_roots()` first if you need both.
- **No `with_client_auth`.** Both sides default to `with_no_client_auth()`; use `with_client_ca_pem`/`with_require_client_cert` (server) and `with_client_cert_pem` (client) for mutual TLS.
- **`install_default_crypto_provider` returns unit, not `Result`.** A warning is logged (not returned) when the provider is already installed.
- **PEM parsing uses `CertificateDer::pem_slice_iter`.** This iterates all PEM objects in the slice. If the PEM contains non-cert objects (e.g., private keys), they are included in the iterator and may cause `RootCertStore::add` to fail with a type error.

## See also

- [protocols-tunnels.md](protocols-tunnels.md) -- WebSocket and raw tunnels (wrap in TLS for wss/secure).
- [protocols-trojan.md](protocols-trojan.md) -- Trojan client consumes TLS.
- [transports-ssh-quic-h3.md](transports-ssh-quic-h3.md) -- Alternative transport layers.
- [server.md](server.md) -- Listener lifecycle that drives TLS accept.
