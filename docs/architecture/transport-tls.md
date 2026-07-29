# eggress-transport-tls

`crates/eggress-transport-tls/`

Shared TLS transport layer using rustls. Provides client/server TLS configuration builders and convenience functions.

## Key Types

| Type | Description |
|---|---|
| `TlsClientConfigBuilder` | Builds `Arc<ClientConfig>` from system roots, custom CA, ALPN, insecure mode |
| `TlsServerConfigBuilder` | Builds `Arc<ServerConfig>` from cert chain and key PEM |
| `TlsError` | Structured error type for TLS operations |

## Key Functions

| Function | Description |
|---|---|
| `tls_connect(stream, server_name)` | Wrap `BoxStream` in client TLS |
| `tls_accept(stream, server_config)` | Wrap `BoxStream` in server TLS |
| `load_system_roots()` | Load OS root CA certificates |
| `load_pem_roots(path)` | Load root CAs from PEM file |
| `load_pem_certs(path)` | Load certificate chain from PEM file |
| `install_default_crypto_provider()` | Install rustls default crypto provider |

## TLS Configuration Options

### Client

- System root CAs (default)
- Custom CA PEM file
- ALPN protocol negotiation
- Insecure mode (skip verification)
- Server name override

### Server

- Certificate chain PEM
- Private key PEM
- Client certificate verification (optional)

## Usage in the System

TLS is applied at two points:

1. **Listener TLS** — inbound connections are TLS-unwrapped before protocol detection
2. **Upstream TLS** — outbound connections to `+tls` hops are wrapped via `ChainExecutor`

Used by:
- `eggress-runtime` — listener TLS configuration
- `eggress-server` — upstream TLS in chain execution
- `eggress-protocol-trojan` — Trojan TLS transport

## Dependencies

None — standalone TLS layer using rustls.

See [overview.md](overview.md) for context.
