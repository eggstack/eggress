# Reverse Proxy Development

## When to use
Use when implementing or modifying reverse/backward proxy functionality for NAT traversal, pproxy-compatible raw-relay, or control-channel based proxying.

## Architecture overview
- pproxy-compatible raw-relay protocol for NAT traversal (backward proxy model)
- Server (acceptor) binds a control listener + external listener; client (control) connects to the server and relays traffic to local targets
- Each control connection carries exactly one proxy session (pproxy backward model)
- When the session ends, the client reconnects with exponential backoff
- Native control channel supports opt-in server-authenticated TLS with optional mTLS via the shared rustls transport (`[[reverse_servers.tls]]` / `[[reverse_clients.tls]]`); `pproxy_compat` wire remains plaintext

## Key crates
- `eggress-protocol-reverse` — wire format, auth handshake, bidirectional relay
  - `src/lib.rs` — constants (`HANDSHAKE_ACCEPT`/`REJECT`), auth parsing/redaction, `relay_bidirectional()`, `relay_bidirectional_boxed()` (BoxStream for TLS), `ControlState` enum
  - `src/tls.rs` — `ReverseServerTlsConfig` / `ReverseClientTlsConfig` (PEM validation, shared-transport builders, redacted Debug)
  - `src/server.rs` — `ReverseServer` acceptor: control connection pool, external client dispatch, `allow_bind` enforcement, defense-in-depth `validate()` (incl. TLS), optional `tls` (TLS before auth, `tls_accept`)
  - `src/client.rs` — `ReverseClient` control client: auto-reconnect with backoff, reused `Arc<ClientConfig>` + SNI, `TargetResolver` trait, `TargetResolution` enum
  - `src/metrics.rs` — `ReverseMetrics` (Prometheus counters/gauges) and `ReverseMetricsSnapshot`
- `eggress-runtime/src/reverse.rs` — `RouteEngineTargetResolver` adapter bridging the route engine to `TargetResolver`
- `eggress-runtime/src/supervisor.rs` — spawns reverse servers/clients, manages lifecycle
- `eggress-config/src/model.rs` — `ReverseServerConfig`, `ReverseClientConfig` TOML models
- `eggress-config/src/compile.rs` — `CompiledReverseServerConfig`, `CompiledReverseClientConfig`

## Wire format
1. Client connects to server control port
2. Client sends raw auth: `user:pass\n` (pproxy format)
3. Server responds with 1-byte handshake: `0x01` (accept) or `0x00` (reject)
4. After handshake, the server pairs an external client with the control connection
5. Bidirectional TCP relay (`relay_bidirectional()`) between external client and control stream
6. Session ends when either side closes; client reconnects

## Config model

### `[[reverse_servers]]` (acceptor side)
```toml
[[reverse_servers]]
id = "rev-1"
control_bind = "0.0.0.0:8443"      # Control listener (reverse clients connect here)
external_bind = "0.0.0.0:9000"     # External listener (proxy clients connect here)
auth_username = "user"              # Optional auth
auth_password = "pass"
# auth_password_env = "REV_PASS"   # Alternative: read password from env
max_streams = 1024                  # Max concurrent streams per control client
heartbeat_interval = "30s"

[reverse_servers.tls]
cert = "/etc/eggress/rev-cert.pem"
key = "/etc/eggress/rev-key.pem"
# client_ca = "/etc/eggress/rev-client-ca.pem"
# require_client_cert = true
```

### `[[reverse_clients]]` (control side)
```toml
[[reverse_clients]]
id = "rev-client-1"
server_addr = "1.2.3.4:8443"       # Server to connect to
auth_username = "user"
auth_password = "pass"
reconnect_initial = "1s"           # Initial reconnect backoff
reconnect_max = "30s"              # Max reconnect backoff
heartbeat_interval = "30s"
parallel_connections = 1            # Number of parallel control connections
default_target_host = "127.0.0.1"  # Fallback target host
default_target_port = 80           # Fallback target port

[reverse_clients.tls]
ca = "/etc/eggress/rev-ca.pem"     # Optional; defaults to system roots
server_name = "rev.example.com"    # Required SNI/verification name
# client_cert = "/etc/eggress/rev-client-cert.pem"
# client_key = "/etc/eggress/rev-client-key.pem"
```

## pproxy URI schemes
The reverse proxy supports these pproxy-compatible URI schemes:
- `bind://` — bind a local port and relay through a reverse server
- `listen://` — listen for incoming connections and relay through a reverse server
- `backward://` — backward/reverse proxy mode
- `rebind://` — rebind connections through a reverse server
- `+in` modifier — enables parallel inbound connections (maps to `parallel_connections`)

## Security
- **Plaintext by default; native TLS opt-in** — use `[[reverse_servers.tls]]` / `[[reverse_clients.tls]]` (server-authenticated, optional mTLS via shared rustls transport) when control traffic leaves a trusted network; `pproxy_compat` + TLS is rejected
- **Defense-in-depth validation** — `ReverseServerConfig::validate()` rejects unsafe configs:
  - Non-loopback `external_bind` requires both `auth_username`/`auth_password` AND a non-empty `allow_bind` allowlist
  - Loopback bind is always allowed without auth
  - TLS: `require_client_cert` without `client_ca` fails; client cert without key fails; missing/invalid `server_name` fails; malformed PEM fails at config compile
- **`allow_bind` allowlist** — restricts which external addresses the server will bind; enforced at startup before binding
- **Auth required for non-loopback** — server refuses to start if non-loopback external bind lacks auth + allowlist
- **Credentials never logged** — `redact_auth()` returns `user:****` form
- **Auth payload capped** at 4 KiB to prevent unbounded memory growth

## Route integration
- `LeafMatcher.reverse_listener` field — matches reverse listener by name in the routing rule engine
- `RouteEngineTargetResolver` in `eggress-runtime/src/reverse.rs` bridges the route engine to the `TargetResolver` trait, enabling dynamic target resolution through standard routing rules

## Limitations
- **TCP only** — no UDP support through reverse tunnels
- **No multiplexing** — one session per control connection (pproxy backward model)
- **No pproxy-wire TLS** — native TLS does not imply pproxy interop; `pproxy_compat` wire stays plaintext
- **No jump chains through reverse** — reverse proxy clients/servers are leaf endpoints, not chain hops
- **No multiplexed streams** — each control connection carries exactly one proxy session

## Testing
```bash
# Unit tests
cargo test -p eggress-protocol-reverse

# TLS/mTLS control-channel tests
cargo test -p eggress-protocol-reverse --test tls
cargo test -p eggress-transport-tls

# Integration tests
cargo test -p eggress-protocol-reverse --test integration

# Runtime tests (route engine adapter)
cargo test -p eggress-runtime reverse

# pproxy compat reverse tests
cargo test -p eggress-pproxy-compat --lib reverse

# Reverse interop tests (un-gated subset)
cargo test -p eggress-runtime --test reverse_interop

# Reverse runtime tests
cargo test -p eggress-runtime --test reverse_runtime

# Soak tests (gated, requires EGRESS_REQUIRE_SOAK=1)
EGRESS_REQUIRE_SOAK=1 cargo test -p eggress-runtime --test reverse_soak -- --ignored --test-threads=1

# Interop tests (gated, requires pproxy on PATH)
EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test -p eggress-runtime --test reverse_interop -- --ignored

# Always verify
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Common pitfalls
- Never expose reverse server on non-loopback without both auth AND `allow_bind` — the `validate()` call will reject it
- Each control connection carries exactly one session — do not attempt to multiplex
- The client reconnects automatically on session end or failure — do not spawn additional reconnect loops
- Auth payload is newline-delimited (`user:pass\n`) — do not modify the wire format
- `allow_bind` compares by port + IP (v4/v4 or v6/v6 only) — mixed-family comparisons always fail
