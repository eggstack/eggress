# Advanced Transports — SSH, QUIC, HTTP/3

Optional feature-gated transports: `eggress-transport-ssh` (`ssh` feature),
`eggress-transport-quic` + `eggress-protocol-h3` (`quic` feature). All are
stream-native adapters over the same `BoxStream` boundary.

## eggress-transport-ssh

Single file. `SshSessionCache` wraps russh:

- `SshAuth`: Password or PrivateKey.
- `SshHostKeyPolicy`: KnownHosts / KnownHostsFile / InsecureCompatibility
  (the insecure mode exists solely for pproxy parity and is classified as
  such).
- `open_tcp_channel` / `open_unix_channel` / `start_remote_tcp_forward`
  (server-side forwarding); sessions cached by `SshSessionKey` and reused
  across connections.
- Injected into the data plane via `ConnectionConfig::ssh_sessions`; chain hop
  handler lives in eggress-server's executor assembly.
- Integration test runs against real OpenSSH (`tests/openssh.rs`).

## eggress-transport-quic

Single file over quinn: `QuicClient` / `QuicListener` / `QuicConnection` /
`QuicStream`. Key surface:

- Streams are bridged to `BoxStream` — Quinn types never leak upward.
- `QuicConnection::into_h3()` and `accept_connection()` exist specifically so
  the H3 protocol crate can own stream dispatch.
- Client verification via rustls platform verifier by default; `insecure-quic`
  feature is the explicit test escape hatch.

## eggress-protocol-h3

HTTP/3 CONNECT over that QUIC transport (ALPN `h3`):

- `H3Client`: lazily-established pooled session per QUIC connection;
  multiplexed CONNECT streams with optional Basic auth (constant-time).
- Server replies: 200 OK, 407 with `Proxy-Authenticate: Basic realm="eggress"`,
  405 for non-CONNECT.
- H3 data is bridged to `BoxStream` through 64 KiB tokio duplex pipes so the
  rest of the stack stays H3-agnostic.

## Review entry points

- Verify: `cargo check -p eggress-cli --features ssh,quic`;
  `cargo test -p eggress-protocol-h3`
