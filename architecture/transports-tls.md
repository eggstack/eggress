# eggress-transport-tls -- Shared rustls Layer

The only TLS implementation in the workspace (no OpenSSL anywhere). Wraps
`BoxStream`s in TLS for listener inbound, upstream outbound, and Trojan.

## Module map

| File | Role |
|---|---|
| `src/client.rs` | `TlsClientConfigBuilder`: system/custom CA PEM, ALPN, insecure mode, server-name override, `InsecureVerifier` (test/feature-gated) |
| `src/server.rs` | `TlsServerConfigBuilder`: cert chain + key PEM (PKCS#8), ALPN, `load_cert_chain_pem`, `load_private_key_pem` |
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
| `with_alpn(protocols)` | Sets ALPN protocol list (e.g., `b"h2"`, `b"http/1.1"`) |
| `with_h2_alpn()` | Shortcut: `vec![b"h2", b"http/1.1"]` |
| `with_server_name_override(name)` | Default SNI when `tls_connect` has no explicit name |
| `with_insecure()` | Accepts any server cert. **Gated**: `#[cfg(any(test, debug_assertions, feature = "insecure-tls"))]` |
| `build()` | Returns `Arc<ClientConfig>`. Insecure mode uses `InsecureVerifier`; if feature not enabled, returns `TlsError::Handshake` |

### Server (`TlsServerConfigBuilder`)

| Method | Notes |
|---|---|
| `new()` | Empty cert chain, no key, no ALPN |
| `with_certificate_pem(cert_pem)` | Parses PEM cert chain; fails if empty |
| `with_key_pem(key_pem)` | Parses PKCS#8 private key from PEM |
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

`install_default_crypto_provider()` (`lib.rs:15-22`) calls `rustls::crypto::ring::default_provider().install_default()`. The first call succeeds; subsequent calls log a warning and return `Err` (ring provider is already active). This is safe to call multiple times.

### Client config construction

1. `TlsClientConfigBuilder::build()` branches on `self.insecure`:
   - **Insecure** (gated on `test || debug_assertions || feature = "insecure-tls"`): Uses `ClientConfig::builder().dangerous().with_custom_certificate_verifier(InsecureVerifier)`. The `InsecureVerifier` accepts any certificate and any handshake signature without validation.
   - **Secure**: Uses `ClientConfig::builder().with_root_certificates(self.root_store).with_no_client_auth()`.
2. ALPN protocols are set on the resulting config.
3. The config is wrapped in `Arc` and returned.

### Server config construction

1. `TlsServerConfigBuilder::build()` checks that `key_der` is `Some` and `cert_chain` is non-empty.
2. `ServerConfig::builder().with_no_client_auth().with_single_cert(chain, key)` is called.
3. ALPN protocols are set on the resulting config.
4. The config is wrapped in `Arc` and returned.

### TLS handshake wrapping

Both `tls_connect` and `tls_accept` use `tokio-rustls`:

- `tls_connect`: `TlsConnector::from(config).connect(domain, stream)` -- performs client-side TLS handshake over the boxed stream. Returns a `TlsStream<BoxStream>` re-boxed as `BoxStream`.
- `tls_accept`: `TlsAcceptor::from(config).accept(stream)` -- performs server-side TLS handshake. Returns a `TlsStream<BoxStream>` re-boxed as `BoxStream`.

## Consumers

| Consumer | Usage |
|---|---|
| `eggress-server` (`execute.rs`) | Upstream `+tls` hops: builds `TlsClientConfigBuilder` with system roots or custom CA, calls `tls_connect` on the box stream |
| `eggress-runtime` (`supervisor.rs`) | Listener TLS: builds `TlsServerConfigBuilder` from config, calls `tls_accept` on inbound streams |
| `eggress-protocol-trojan` (`tcp.rs`) | Trojan client: builds `TlsClientConfigBuilder` with system roots, calls `tls_connect` for the Trojan-over-TLS channel |

## Security notes

- **Insecure mode is triple-gated.** `with_insecure()` is only available under `#[cfg(any(test, debug_assertions, feature = "insecure-tls"))]`. If the feature is not enabled, `build()` returns `TlsError::Handshake("insecure TLS requires the insecure-tls feature")`. This prevents accidental use in release builds.
- **Empty PEM is an error.** `load_pem_roots(b"")` returns `PemParse("no certificates found in PEM root material")`. This prevents silently trusting everything when CA material is misconfigured.
- **No client auth.** Both client and server configs use `with_no_client_auth()`. Mutual TLS is not supported.
- **PKCS#8 only.** `load_private_key_pem` uses `PrivatePkcs8KeyDer::from_pem_slice`. Other key formats (RSA, EC) are not supported.
- **Ring provider only.** The crypto provider is hardcoded to ring. No alternative providers are supported.
- **No session resumption.** The builder does not configure session tickets or session caching.

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

- **`insecure-tls` feature escape hatch.** The `with_insecure()` method and `InsecureVerifier` are compiled only when `test || debug_assertions || feature = "insecure-tls"`. Auditing trust paths requires grepping for `insecure-tls` in `Cargo.toml` files.
- **`load_pem_certs` vs `load_pem_roots`.** `load_pem_certs` returns raw `CertificateDer` values and does NOT fail on empty input. `load_pem_roots` builds a `RootCertStore` and DOES fail on empty input. These have different error semantics for the same "empty PEM" case.
- **`with_custom_ca_pem` replaces, not extends.** It sets `builder.root_store = roots`, discarding any previously loaded roots (including system roots). Call `with_system_roots()` first if you need both.
- **No `with_client_auth`.** Both sides use `with_no_client_auth()`. If mutual TLS is needed, the builder API would need extension.
- **`install_default_crypto_provider` warning is not an error.** A warning is logged (not returned) when the provider is already installed. The `Err` is silently dropped in the `if let Err` pattern at `lib.rs:16`.
- **PEM parsing uses `CertificateDer::pem_slice_iter`.** This iterates all PEM objects in the slice. If the PEM contains non-cert objects (e.g., private keys), they are included in the iterator and may cause `RootCertStore::add` to fail with a type error.

## See also

- [protocols-tunnels.md](protocols-tunnels.md) -- WebSocket and raw tunnels (wrap in TLS for wss/secure).
- [protocols-trojan.md](protocols-trojan.md) -- Trojan client consumes TLS.
- [transports-ssh-quic-h3.md](transports-ssh-quic-h3.md) -- Alternative transport layers.
- [server.md](server.md) -- Listener lifecycle that drives TLS accept.
