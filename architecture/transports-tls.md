# eggress-transport-tls — Shared rustls Layer

The only TLS implementation in the workspace (no OpenSSL anywhere). Wraps
`BoxStream`s in TLS for listener inbound, upstream outbound, and Trojan.

## Module map

| File | Role |
|---|---|
| `src/client.rs` | `TlsClientConfigBuilder`: system roots or custom CA PEM, ALPN, insecure mode, server-name override → `Arc<ClientConfig>` |
| `src/server.rs` | `TlsServerConfigBuilder`: cert chain + key PEM → `Arc<ServerConfig>` |
| `src/roots.rs` | `load_system_roots`, `load_pem_roots`, `load_pem_certs` (empty PEM is an error, not silently-trusting) |
| `src/transport.rs` | `tls_connect(stream, config, server_name)` / `tls_accept(stream, config)` — BoxStream in, TLS-wrapped BoxStream out |
| `src/lib.rs` | Installs the `ring` crypto provider exactly once |
| `src/error.rs` | `TlsError` |

## Notes

- Feature `insecure-tls` enables a verifier bypass; it is a named,
  explicitly-opt-in escape hatch — grep for it when auditing trust paths.
- Consumers: eggress-server (upstream `+tls` hops), eggress-runtime (listener
  TLS), eggress-protocol-trojan (client TLS).

## Review entry points

- Verify: `cargo test -p eggress-transport-tls`
