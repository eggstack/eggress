# Advanced Transports — SSH, QUIC, HTTP/3

Optional feature-gated transports: `eggress-transport-ssh` (`ssh` feature),
`eggress-transport-quic` + `eggress-protocol-h3` (`quic` feature). All three
crates produce and consume `BoxStream` (`eggress_core::BoxStream`), so the
rest of the proxy stack remains transport-agnostic.

## Module map

| Crate | Root file | Lines | Role |
|---|---|---|---|
| `eggress-transport-ssh` | `src/lib.rs` | 463 | SSH client session cache, channel open, remote forward |
| `eggress-transport-quic` | `src/lib.rs` | 593 | QUIC client/listener/connection/stream over quinn |
| `eggress-protocol-h3` | `src/lib.rs` | 478 | HTTP/3 CONNECT client and server over QUIC |

---

## eggress-transport-ssh

Single-file crate. All types and logic live in `src/lib.rs`.

### Public API

| Item | Line | Description |
|---|---|---|
| `SshSessionCache` | :244 | `Arc<Mutex<HashMap<SshSessionKey, Arc<SessionHandle>>>>` cache |
| `::new()` / `::new_compatibility()` / `::with_known_hosts(path)` | :256/:264/:272 | Constructor variants |
| `::open_tcp_channel()` | :280 | Direct TCP channel; validates port != 0 |
| `::open_unix_channel()` | :299 | Unix domain socket channel; validates non-empty path |
| `::start_remote_tcp_forward()` | :323 | Server-side TCP forwarding (pproxy compat) |
| `::shutdown()` / `::invalidate(key)` | :410/:415 | Bulk clear / single session eviction |
| `SshAuth` | :44 | `Password(String)` or `PrivateKey(String)` — debug redacts both |
| `SshHostKeyPolicy` | :114 | `KnownHosts` / `KnownHostsFile(PathBuf)` / `InsecureCompatibility` |
| `SshSessionKey` | :60 | Cache key: `host`, `port`, `username`, `auth`, `hop_index` |
| `SshRemoteForward` | :203 | Session handle + `mpsc::Receiver` for forwarded channels |
| `SshRemoteForward::accept()` | :222 | Wait for next incoming connection |
| `SshRemoteForward::cancel()` | :230 | Cancel forward while retaining session |

### How it works

1. `get_or_connect()` (:360) locks cache, checks `!session.is_closed()`,
   removes dead entries, then calls `connect_authenticated_with_config()`.
2. Auth (:443-460): `Password` → `authenticate_password`; `PrivateKey` →
   `russh::keys::load_secret_key` → `PrivateKeyWithHashAlg` → `authenticate_publickey`.
3. Keepalive hardcoded: `keepalive_interval = 60s`, `keepalive_max = 3`
   (:387-388).
4. Host key verification (:132-165): `KnownHosts` → `check_known_hosts`,
   `KnownHostsFile` → `check_known_hosts_path`, `InsecureCompatibility` → `true`.
5. `CompatClient` (:81) is a private `russh::client::Handler`. Its
   `forwarded_channels` field enables `server_channel_open_forwarded_tcpip`
   (:170) for reverse-forwarded channels.
6. Remote forward (:323-358) bypasses cache — always creates a fresh
   session. Non-loopback bind emits `tracing::warn` (:331-336).

### Security notes

- `InsecureCompatibility` exists solely for pproxy parity (:120);
  disables host-key verification, emits `tracing::warn` per connection.
- Both `SshAuth::Debug` (:49) and `SshSessionKey::Debug` (:68) redact
  secrets with `****`.

### Reviewer gotchas

- `start_remote_tcp_forward` bypasses the session cache entirely.
- `SshSessionKey` equality includes `hop_index` — same host, different hops
  → separate cache entries.
- Auth failure (:460) returns `AuthenticationFailed`; the handle is not
  yet in cache at that point.

---

## eggress-transport-quic

Single-file crate over quinn. Exposes transport primitives without leaking
Quinn types upward.

### Public API

| Item | Line | Description |
|---|---|---|
| `QuicClient::connect(host, port, config)` | :222 | DNS resolve, bind ephemeral UDP, TLS setup |
| `QuicClient::open_stream()` | :302 | Bi-stream; reconnects once on dead cached connection |
| `QuicClient::get_connection()` | :315 | Cached `QuicConnection` for H3 integration |
| `QuicClient::close()` | :328 | Stop endpoint and all connections |
| `QuicListener::bind(addr, config)` | :340 | Bind UDP with TLS certificate material |
| `QuicListener::run(cancel, handler)` | :355 | Accept loop: each bi-stream dispatched independently |
| `QuicListener::accept_connection(cancel)` | :421 | Accept one connection for H3 (no stream dispatch) |
| `QuicConnection::into_h3()` | :206 | Convert to `h3_quinn::Connection` for protocol layer |
| `QuicStream` | :121 | `tokio::io::Join<RecvStream, SendStream>` → AsyncRead+AsyncWrite |
| `QuicClientConfig` | :53 | `server_name`, `insecure`, `idle_timeout`, `max_concurrent_streams`, `alpn_protocols` |
| `QuicServerConfig` | :80 | `certificate_pem`, `private_key_pem`, `idle_timeout`, `max_concurrent_streams`, `alpn_protocols` |

### How it works

1. Client TLS (:234-260): `insecure` + feature flag → `InsecureVerifier`;
   otherwise `rustls_platform_verifier`. ALPN from config.
2. Connection caching (:217): single `Mutex<Option<QuicConnection>>`.
   `open_stream()` (:302) retries once after clearing a dead connection.
3. Server accept loop (:355-418): each connection spawns a task looping
   on `accept_bi()`; each bi-stream is spawned independently to handler.
4. `accept_connection()` (:421-436) accepts without spawning a stream loop —
   the H3 crate owns stream dispatch.
5. Server config (:90-117): PEM cert/key → `rustls::ServerConfig` with
   `with_no_client_auth()`, ALPN, bidi-stream limits, idle timeout.
6. Constants: `DEFAULT_IDLE_TIMEOUT = 60s`, `DEFAULT_MAX_STREAMS = 1024`
   (:20-21).

### Reviewer gotchas

- `insecure` is gated by `#[cfg(feature = "insecure-quic")]` (:235).
  Without the feature, `insecure: true` returns a runtime error. Unlike TLS's
  `insecure-tls` (available in `#[cfg(test)]`), QUIC's bypass requires the
  explicit compile-time feature even in tests.
- `QuicClient::connect` binds `0.0.0.0:0` — ephemeral endpoint.
- `QuicConnection::close` uses QUIC error code `0u32` (:181).

---

## eggress-protocol-h3

HTTP/3 CONNECT protocol layer over the QUIC transport.

### Public API

| Item | Line | Description |
|---|---|---|
| `H3Client::new(quic, auth)` | :65 | Optional `(username, password)` Basic auth |
| `H3Client::connect(target)` | :105 | Multiplexed CONNECT stream |
| `H3Client::close()` | :155 | Drop session and close QUIC endpoint |
| `serve_connection(conn, cancel, auth, handler)` | :177 | Server-side: accept all H3 CONNECT requests |
| `H3Request::target()` | :46 | Parse authority into `TargetAddr` |

### How it works

1. Session pooling (:73-96): lazy `h3::client::SendRequest` creation via
   `h3::client::new(connection.into_h3())`. Driver spawned with
   `driver.wait_idle().await`.
2. Client CONNECT (:105-153): `CONNECT https://{authority}/`, optional
   `Proxy-Authorization: Basic {base64}`, only `200 OK` accepted.
3. Server (:177-257): checks `Method::CONNECT` (else `405`), parses
   authority, verifies Basic auth via `parse_basic_authorization` (:259).
   Auth failure → `407 Proxy Authentication Required` with
   `Proxy-Authenticate: Basic realm="eggress"` (:168-170).
4. Auth (:220-231): `subtle::ConstantTimeEq` for username and password.
   `unwrap_u8() == 1` ensures constant-time semantics (:229).
5. Duplex bridging (:277-363): both `bridge_client_stream` and
   `bridge_server_stream` create 64 KiB `tokio::io::duplex`, spawn two
   tasks shuttling data between H3 stream and duplex. Application side
   is `BoxStream`.

### Security notes

- Constant-time auth comparison prevents timing side-channels.
- Server realm `"eggress"` is static; no config details leaked.

### Reviewer gotchas

- `bridge_client_stream` (:277) and `bridge_server_stream` (:321) are
  nearly identical — differs only in H3 stream type parameters.
- H3 driver task (:84-89) logs `wait_idle` errors at debug level (normal, including clean shutdown).
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
