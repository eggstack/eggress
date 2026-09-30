# Advanced Transports — SSH, QUIC, HTTP/3

Optional feature-gated transports: `eggress-transport-ssh` (consumed via the
`ssh` feature on downstream crates — the transport crate itself exposes only a
`pproxy-compat` marker feature), `eggress-transport-quic` + `eggress-protocol-h3`
(`quic` feature). All three crates produce and consume `BoxStream`
(`eggress_core::BoxStream`), so the rest of the proxy stack remains
transport-agnostic.

## Module map

| Crate | Root file | Lines | Role |
|---|---|---|---|
| `eggress-transport-ssh` | `src/lib.rs` | 572 | SSH client session cache, channel open, remote forward |
| `eggress-transport-quic` | `src/lib.rs` | 609 | QUIC client/listener/connection/stream over quinn |
| `eggress-protocol-h3` | `src/lib.rs` | 492 | HTTP/3 CONNECT client and server over QUIC |

---

## eggress-transport-ssh

Single-file crate. All types and logic live in `src/lib.rs`.

### Public API

| Item | Line | Description |
|---|---|---|
| `SshSessionCache` | :250 | `Arc<Mutex<HashMap<SshSessionKey, Arc<SessionHandle>>>>` cache |
| `::new()` / `::new_compatibility()` / `::with_known_hosts(path)` | :262/:279/:287 | Constructor variants |
| `::policy()` | :270 | Return the cache's `SshHostKeyPolicy` |
| `::open_tcp_channel()` | :295 | Direct TCP channel; validates port != 0 |
| `::open_tcp_channel_fresh()` | :315 | Unpooled TCP channel for nested hops (fresh session over the supplied prefix stream) |
| `::open_unix_channel()` | :362 | Unix domain socket channel; validates non-empty path |
| `::open_unix_channel_fresh()` | :338 | Unpooled Unix channel for nested hops |
| `::start_remote_tcp_forward()` | :386 | Server-side TCP forwarding (pproxy compat) |
| `::shutdown()` / `::invalidate(key)` | :473/:478 | Bulk clear / single session eviction |
| `SshStream` | :15 | Boxed SSH channel stream alias |
| `SshTransportError` | — | Transport error taxonomy |
| `SshRemoteForward::address()` / `::port()` | :218/:223 | Bound forward address introspection |
| `SshAuth` | :44 | `Password(String)` or `PrivateKey(String)` — the latter is a filesystem *path* loaded via `russh::keys::load_secret_key`; debug redacts both |
| `SshHostKeyPolicy` | :120 | `KnownHosts` / `KnownHostsFile(PathBuf)` / `InsecureCompatibility` |
| `SshSessionKey` | :64 | Cache key: `host`, `port`, `username`, `auth`, `hop_index`, `policy` |
| `SshRemoteForward` | :209 | Session handle + `mpsc::Receiver` for forwarded channels |
| `SshRemoteForward::accept()` | :228 | Wait for next incoming connection |
| `SshRemoteForward::cancel()` | :236 | Cancel forward while retaining session |

### How it works

1. `get_or_connect()` (:423) locks cache, checks `!session.is_closed()`,
   removes dead entries, then calls `connect_authenticated_with_config()`.
2. Auth (:554-569): `Password` → `authenticate_password`; `PrivateKey` →
   `russh::keys::load_secret_key` → `PrivateKeyWithHashAlg` → `authenticate_publickey`.
3. Keepalive hardcoded: `keepalive_interval = 60s`, `keepalive_max = 3`
   (:450-451).
4. Host key verification (:137-171): `KnownHosts` → `check_known_hosts`,
   `KnownHostsFile` → `check_known_hosts_path`, `InsecureCompatibility` → `true`.
5. `CompatClient` (:87) is a private `russh::client::Handler`. Its
   `forwarded_channels` field enables `server_channel_open_forwarded_tcpip`
   (:176) for reverse-forwarded channels.
6. Remote forward (:386+) bypasses cache — always creates a fresh
   session (mpsc channel depth 128). Non-loopback bind emits `tracing::warn`
   (:394-397).

### Security notes

- `InsecureCompatibility` exists solely for pproxy parity (:125);
  disables host-key verification, emits `tracing::warn` per connection.
- `SshSessionCache::new()` retains verified native known-hosts behavior;
  `eggress-embed::outbound::OutboundConnector` selects it for TOML chains and
  selects `new_compatibility()` only for its explicitly pproxy-compatible
  constructor.
- The outbound chain handler reuses SSH sessions only at hop zero. Nested SSH
  hops authenticate over the supplied prefix stream with a fresh session,
  retained by the returned channel until it closes.
- An explicit hop-zero `local_bind` uses a fresh SSH session so a cached
  transport cannot bypass its source-address policy.
- Both `SshAuth::Debug` (:49) and `SshSessionKey::Debug` (:68) redact
  secrets with `****`.

### Reviewer gotchas

- `start_remote_tcp_forward` bypasses the session cache entirely.
- `SshSessionKey` equality includes `hop_index`, but position does not identify
  an arbitrary route prefix. Nested outbound hops bypass the cache instead of
  relying on that field for cross-chain isolation.
- Auth failure (:569) returns `AuthenticationFailed`; the handle is not
  yet in cache at that point.

---

## eggress-transport-quic

Single-file crate over quinn. Exposes transport primitives without leaking
Quinn types upward.

### Public API

| Item | Line | Description |
|---|---|---|
| `QuicClient::connect(host, port, config)` | :227 | DNS resolve, bind ephemeral UDP, TLS setup |
| `QuicClient::open_stream()` | :318 | Bi-stream; reconnects once on dead cached connection |
| `QuicClient::get_connection()` | :331 | Cached `QuicConnection` for H3 integration |
| `QuicClient::reset_connection()` | :339 | Drop the cached connection; the H3 retry path calls this |
| `QuicClient::close()` | :344 | Stop endpoint and all connections |
| `QuicListener::bind(addr, config)` | :356 | Bind UDP with TLS certificate material |
| `QuicListener::local_addr()` | :364 | Local UDP socket address of the listener |
| `QuicListener::run(cancel, handler)` | :371 | Accept loop: each bi-stream dispatched independently |
| `QuicListener::accept_connection(cancel)` | :437 | Accept one connection for H3 (no stream dispatch) |
| `QuicConnection::remote_address()` | :180 | Peer's UDP socket address |
| `QuicConnection::open_stream()` / `::accept_stream()` | :190/:200 | Open / accept one bi-stream as `BoxStream` |
| `QuicConnection::into_h3()` | :211 | Convert to `h3_quinn::Connection` for protocol layer |
| `QuicConnection::close(reason)` | :185 | Close with QUIC error code `0u32` |
| `QuicStream` | :126 | `tokio::io::Join<RecvStream, SendStream>` → AsyncRead+AsyncWrite |
| `QuicClientConfig` | :53 | `server_name`, `insecure`, `idle_timeout`, `max_concurrent_streams`, `alpn_protocols`, caller-supplied TLS policy |
| `QuicServerConfig` | :85 | `certificate_pem`, `private_key_pem`, `idle_timeout`, `max_concurrent_streams`, `alpn_protocols` |

### How it works

1. Client TLS (:251-269): `insecure` + feature flag → `InsecureVerifier`;
   otherwise `rustls_platform_verifier`. ALPN from config.
2. Connection caching (:291): single `Mutex<Option<QuicConnection>>`.
   `open_stream()` (:318) retries once after clearing a dead connection.
3. Server accept loop (:371+): each connection spawns a task looping
   on `accept_bi()`; each bi-stream is spawned independently to handler.
4. `accept_connection()` (:437-452) accepts without spawning a stream loop —
   the H3 crate owns stream dispatch.
5. Server config (:95+): PEM cert/key → `rustls::ServerConfig` with
   `with_no_client_auth()`, ALPN, bidi-stream limits, idle timeout.
6. Constants: `DEFAULT_IDLE_TIMEOUT = 60s`, `DEFAULT_MAX_STREAMS = 1024`
   (:20-21). Back-pressure bounds: `MAX_CONCURRENT_CONNECTION_TASKS = 1024`
   (:27), `MAX_CONCURRENT_STREAM_TASKS = 4096` (:32) behind
   `QuicListener::run`.

### Reviewer gotchas

- `insecure` is gated by `#[cfg(feature = "insecure-quic")]` (:251).
  Without the feature, `insecure: true` returns a runtime error. Unlike TLS's
  `insecure-tls` (available in `#[cfg(test)]`), QUIC's bypass requires the
  explicit compile-time feature even in tests.
- `QuicClient::connect` binds `0.0.0.0:0` — ephemeral endpoint.
- `QuicConnection::close` uses QUIC error code `0u32` (:186).

---

## eggress-protocol-h3

HTTP/3 CONNECT protocol layer over the QUIC transport.

### Public API

| Item | Line | Description |
|---|---|---|
| `H3Client::new(quic, auth)` | :73 | Optional `(username, password)` Basic auth |
| `H3Client::connect(target)` | :113 | Multiplexed CONNECT stream |
| `H3Client::close()` | :163 | Drop session and close QUIC endpoint |
| `serve_connection(conn, cancel, auth, handler)` | :185 | Server-side: accept all H3 CONNECT requests |
| `H3Request::target()` | :54 | Parse authority into `TargetAddr` |

### How it works

1. Session pooling (:73+): lazy `h3::client::SendRequest` creation via
   `h3::client::new(connection.into_h3())`. Driver spawned with
   `driver.wait_idle().await`.
2. Client CONNECT (:113+): `CONNECT https://{authority}/`, optional
   `Proxy-Authorization: Basic {base64}`, only `200 OK` accepted.
3. Server (:185+): checks `Method::CONNECT` (:212, else `405`), parses
   authority, verifies Basic auth via `parse_basic_authorization` (:267).
   Auth failure → `407 Proxy Authentication Required` with
   `Proxy-Authenticate: Basic realm="eggress"` (`h3_auth_required_response`, :175).
4. Auth (:228-237): `subtle::ConstantTimeEq` for username and password.
   `unwrap_u8() == 1` ensures constant-time semantics (:237).
   Per-connection request bound: `MAX_ACTIVE_REQUESTS_PER_CONNECTION = 256`
   (:18).
5. Duplex bridging (:285-340): both `bridge_client_stream` and
   `bridge_server_stream` create 64 KiB `tokio::io::duplex`, spawn two
   tasks shuttling data between H3 stream and duplex. Application side
   is `BoxStream`.

### Security notes

- Constant-time auth comparison prevents timing side-channels.
- Server realm `"eggress"` is static; no config details leaked.

### Reviewer gotchas

- `bridge_client_stream` (:285) and `bridge_server_stream` (:332) are
  nearly identical — differs only in H3 stream type parameters.
- H3 driver task (:93-95) logs `wait_idle` errors at debug level (normal, including clean shutdown).
- `serve_connection` returns `Ok(())` on cancel or normal connection end;
  request-level errors are logged and skipped.

---

## Review entry points

- SSH: `cargo test -p eggress-transport-ssh --test openssh`
- QUIC: `cargo test -p eggress-transport-quic --features insecure-quic` (round-trip tests are `#[cfg(all(test, feature = "insecure-quic"))]`; plain `cargo test` runs unit/config tests only)
- H3: `cargo test -p eggress-protocol-h3 --features insecure-quic` (same gating as QUIC)

The public embed boundary has a separate required runtime gate because its
regression is ownership of the listener-free `OutboundConnector` executor:

```bash
EGRESS_REQUIRE_OPENSSH_TESTS=1 cargo test -p eggress-embed --locked \
  --no-default-features --features ssh,pproxy-compat \
  --test ssh -- --nocapture
```

That fixture may skip only for missing OpenSSH tools in optional local runs;
once tools are found, setup and readiness errors fail the test. The Ubuntu CI
job installs `openssh-server` before running the gate.

## See also

- [server.md](server.md) — chain hop handler wiring
- [embed.md](embed.md) — `OutboundConnector` chain execution
- [cli.md](cli.md) — `--features ssh,quic` build flags
