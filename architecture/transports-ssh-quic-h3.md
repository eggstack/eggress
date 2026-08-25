# Advanced Transports — SSH, QUIC, HTTP/3

Optional feature-gated transports: `eggress-transport-ssh` (`ssh` feature),
`eggress-transport-quic` + `eggress-protocol-h3` (`quic` feature). All three
crates produce and consume `BoxStream` (`eggress_core::BoxStream`), so the
rest of the proxy stack remains transport-agnostic.

## Module map

| Crate | Root file | Lines | Role |
|---|---|---|---|
| `eggress-transport-ssh` | `src/lib.rs` | 437 | SSH client session cache, channel open, remote forward |
| `eggress-transport-quic` | `src/lib.rs` | 511 | QUIC client/listener/connection/stream over quinn |
| `eggress-protocol-h3` | `src/lib.rs` | 399 | HTTP/3 CONNECT client and server over QUIC |

---

## eggress-transport-ssh

Single-file crate. All types and logic live in `src/lib.rs`.

### Public API

| Item | Line | Description |
|---|---|---|
| `SshSessionCache` | :218 | `Arc<Mutex<HashMap<SshSessionKey, Arc<SessionHandle>>>>` cache |
| `::new()` / `::new_compatibility()` / `::with_known_hosts(path)` | :230/:238/:246 | Constructor variants |
| `::open_tcp_channel()` | :254 | Direct TCP channel; validates port != 0 |
| `::open_unix_channel()` | :273 | Unix domain socket channel; validates non-empty path |
| `::start_remote_tcp_forward()` | :297 | Server-side TCP forwarding (pproxy compat) |
| `::shutdown()` / `::invalidate(key)` | :384/:389 | Bulk clear / single session eviction |
| `SshAuth` | :44 | `Password(String)` or `PrivateKey(String)` — debug redacts both |
| `SshHostKeyPolicy` | :114 | `KnownHosts` / `KnownHostsFile(PathBuf)` / `InsecureCompatibility` |
| `SshSessionKey` | :60 | Cache key: `host`, `port`, `username`, `auth`, `hop_index` |
| `SshRemoteForward` | :177 | Session handle + `mpsc::Receiver` for forwarded channels |
| `SshRemoteForward::accept()` | :196 | Wait for next incoming connection |
| `SshRemoteForward::cancel()` | :204 | Cancel forward while retaining session |

### How it works

1. `get_or_connect()` (:334) locks cache, checks `!session.is_closed()`,
   removes dead entries, then calls `connect_authenticated_with_config()`.
2. Auth (:417-432): `Password` → `authenticate_password`; `PrivateKey` →
   `russh::keys::load_secret_key` → `PrivateKeyWithHashAlg` → `authenticate_publickey`.
3. Keepalive hardcoded: `keepalive_interval = 60s`, `keepalive_max = 3`
   (:360-363).
4. Host key verification (:126-141): `KnownHosts` → `check_known_hosts`,
   `KnownHostsFile` → `check_known_hosts_path`, `InsecureCompatibility` → `true`.
5. `CompatClient` (:81) is a private `russh::client::Handler`. Its
   `forwarded_channels` field enables `server_channel_open_forwarded_tcpip`
   (:144) for reverse-forwarded channels.
6. Remote forward (:297-332) bypasses cache — always creates a fresh
   session. Non-loopback bind emits `tracing::warn` (:304-309).

### Security notes

- `InsecureCompatibility` exists solely for pproxy parity (:119-120);
  disables host-key verification, emits `tracing::warn` per connection.
- Both `SshAuth::Debug` (:50) and `SshSessionKey::Debug` (:68) redact
  secrets with `****`.

### Reviewer gotchas

- `start_remote_tcp_forward` bypasses the session cache entirely.
- `SshSessionKey` equality includes `hop_index` — same host, different hops
  → separate cache entries.
- Auth failure (:434) returns `AuthenticationFailed`; the handle is not
  yet in cache at that point.

---

## eggress-transport-quic

Single-file crate over quinn. Exposes transport primitives without leaking
Quinn types upward.

### Public API

| Item | Line | Description |
|---|---|---|
| `QuicClient::connect(host, port, config)` | :210 | DNS resolve, bind ephemeral UDP, TLS setup |
| `QuicClient::open_stream()` | :281 | Bi-stream; reconnects once on dead cached connection |
| `QuicClient::get_connection()` | :293 | Cached `QuicConnection` for H3 integration |
| `QuicClient::close()` | :298 | Stop endpoint and all connections |
| `QuicListener::bind(addr, config)` | :310 | Bind UDP with TLS certificate material |
| `QuicListener::run(cancel, handler)` | :325 | Accept loop: each bi-stream dispatched independently |
| `QuicListener::accept_connection(cancel)` | :371 | Accept one connection for H3 (no stream dispatch) |
| `QuicConnection::into_h3()` | :194 | Convert to `h3_quinn::Connection` for protocol layer |
| `QuicStream` | :109 | `tokio::io::Join<RecvStream, SendStream>` → AsyncRead+AsyncWrite |
| `QuicClientConfig` | :42 | `server_name`, `insecure`, `idle_timeout`, `max_concurrent_streams`, `alpn_protocols` |
| `QuicServerConfig` | :68 | `certificate_pem`, `private_key_pem`, `idle_timeout`, `max_concurrent_streams`, `alpn_protocols` |

### How it works

1. Client TLS (:222-248): `insecure` + feature flag → `InsecureVerifier`;
   otherwise `rustls_platform_verifier`. ALPN from config.
2. Connection caching (:263-278): single `Mutex<Option<QuicConnection>>`.
   `open_stream()` retries once after clearing a dead connection.
3. Server accept loop (:325-368): each connection spawns a task looping
   on `accept_bi()`; each bi-stream is spawned independently to handler.
4. `accept_connection()` (:371-386) accepts without spawning a stream loop —
   the H3 crate owns stream dispatch.
5. Server config (:79-105): PEM cert/key → `rustls::ServerConfig` with
   `with_no_client_auth()`, ALPN, bidi-stream limits, idle timeout.
6. Constants: `DEFAULT_IDLE_TIMEOUT = 60s`, `DEFAULT_MAX_STREAMS = 1024`
   (:20-21).

### Reviewer gotchas

- `insecure` is gated by `#[cfg(any(test, debug_assertions, feature = "insecure-quic"))]` (:223).
  Without the feature, `insecure: true` returns a runtime error.
- `QuicClient::connect` binds `0.0.0.0:0` — ephemeral endpoint.
- `QuicConnection::close` uses QUIC error code `0u32` (:169).

---

## eggress-protocol-h3

HTTP/3 CONNECT protocol layer over the QUIC transport.

### Public API

| Item | Line | Description |
|---|---|---|
| `H3Client::new(quic, auth)` | :60 | Optional `(username, password)` Basic auth |
| `H3Client::connect(target)` | :86 | Multiplexed CONNECT stream |
| `H3Client::close()` | :124 | Drop session and close QUIC endpoint |
| `serve_connection(conn, cancel, auth, handler)` | :146 | Server-side: accept all H3 CONNECT requests |
| `H3Request::target()` | :41 | Parse authority into `TargetAddr` |

### How it works

1. Session pooling (:68-83): lazy `h3::client::SendRequest` creation via
   `h3::client::new(connection.into_h3())`. Driver spawned with
   `driver.wait_idle().await`.
2. Client CONNECT (:86-122): `CONNECT https://{authority}/`, optional
   `Proxy-Authorization: Basic {base64}`, only `200 OK` accepted.
3. Server (:146-218): checks `Method::CONNECT` (else `405`), parses
   authority, verifies Basic auth via `parse_basic_authorization` (:220).
   Auth failure → `407 Proxy Authentication Required` with
   `Proxy-Authenticate: Basic realm="eggress"` (:136-143).
4. Auth (:188-204): `subtle::ConstantTimeEq` for username and password.
   `unwrap_u8() == 1` ensures constant-time semantics (:198).
5. Duplex bridging (:230-316): both `bridge_client_stream` and
   `bridge_server_stream` create 64 KiB `tokio::io::duplex`, spawn two
   tasks shuttling data between H3 stream and duplex. Application side
   is `BoxStream`.

### Security notes

- Constant-time auth comparison prevents timing side-channels.
- Server realm `"eggress"` is static; no config details leaked.

### Reviewer gotchas

- `bridge_client_stream` (:230) and `bridge_server_stream` (:274) are
  nearly identical — differs only in H3 stream type parameters.
- H3 driver task (:77-79) silently discards `wait_idle` errors (normal).
- `serve_connection` returns `Ok(())` on cancel or normal connection end;
  request-level errors are logged and skipped.

---

## Review entry points

- SSH: `cargo test -p eggress-transport-ssh --test openssh`
- QUIC: `cargo test -p eggress-transport-quic`
- H3: `cargo test -p eggress-protocol-h3`

## See also

- [server.md](server.md) — chain hop handler wiring
- [embed.md](embed.md) — `OutboundConnector` chain execution
- [cli.md](cli.md) — `--features ssh,quic` build flags
