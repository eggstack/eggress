# pproxy Parity Specification

Phase 7 of the pproxy parity roadmap. This document formally specifies Python
pproxy's behavior, what Eggress matches, what remains unimplemented, and what
Eggress intentionally rejects.

## 1. Scope and Version

| Item | Value |
|------|-------|
| pproxy version | 2.7.9 (Python package) |
| Source | https://github.com/nimlang/pproxy |
| CI installation | `pip install "pproxy==2.7.9"` |
| Inspection date | Phase 7 (current) |

This spec covers pproxy 2.7.9 only. Future pproxy versions may add or change
behavior. Any behavioral difference discovered during differential testing that
contradicts this spec should be treated as a bug in this document, not in
Eggress.

## Compatibility Oracle

The pinned compatibility oracle is `pproxy==2.7.9`. All differential tests
and the compatibility manifest target this specific version. The harness
verifies the installed pproxy version matches and fails loudly on mismatch
unless an override is provided.

Target configuration: `tests/compat/pproxy_target.toml`
Compatibility manifest: `tests/compat/pproxy_manifest.toml`

## 2. Local/Listener Protocols

pproxy accepts inbound connections on the following protocols:

| Protocol | URI scheme | Notes |
|----------|-----------|-------|
| HTTP CONNECT | `http://`, `https://` | Forward proxy (tunnel) mode |
| SOCKS4 | `socks4://` | SOCKS4 protocol, destination must be IPv4 |
| SOCKS4a | `socks4a://` | SOCKS4a extension, supports domain destinations |
| SOCKS5 | `socks5://` | CONNECT and UDP ASSOCIATE commands |
| Shadowsocks | `ss://` | AEAD and stream ciphers |
| Trojan | `trojan://` | TLS-based proxy protocol |
| Redir | `redir://` | Transparent proxy, Linux only (`SO_ORIGINAL_DST`) |
| Unix socket | `unix://` | Listen on a Unix domain socket path |

Eggress support status:

| Protocol | Eggress | Notes |
|----------|---------|-------|
| HTTP CONNECT | supported | `eggress-protocol-http` |
| HTTPS (HTTP+TLS) | supported | `eggress-protocol-http` + `eggress-transport-tls` |
| SOCKS4 | supported | `eggress-protocol-socks` |
| SOCKS4a | supported | `eggress-protocol-socks` (alias for SOCKS4) |
| SOCKS5 | supported | `eggress-protocol-socks` |
| Shadowsocks | supported | Explicit protocol mode only; no mixed-listener auto-detection |
| Trojan | **rejected** | No inbound listener; upstream-only |
| Redir | **supported** | Linux only; transparent TCP proxy via `SO_ORIGINAL_DST` (requires iptables/nftables REDIRECT) |
| Unix socket | **supported** | Unix only; listen on Unix domain socket path |
| SSH | **rejected** | Not in scope |

## 3. Remote/Upstream Protocols

pproxy can chain through upstream proxies using these protocols:

| Protocol | URI scheme | Notes |
|----------|-----------|-------|
| HTTP CONNECT | `http://`, `https://` | Standard HTTP tunnel |
| SOCKS4 | `socks4://` | IPv4 destination only |
| SOCKS4a | `socks4a://` | Supports domain destinations |
| SOCKS5 | `socks5://` | CONNECT and UDP ASSOCIATE |
| Shadowsocks | `ss://` | AEAD and stream ciphers |
| Trojan | `trojan://` | TLS-based proxy |
| SSH | `ssh://` | Via `direct-tcpip` channel |
| Direct | `direct://` | No proxy, connect directly to target |

Eggress support status:

| Protocol | Eggress | Notes |
|----------|---------|-------|
| HTTP CONNECT | supported | `eggress-protocol-http` client |
| HTTPS (HTTP+TLS) | supported | `eggress-protocol-http` client + TLS wrapper |
| SOCKS4/SOCKS4a | supported | `eggress-protocol-socks` client |
| SOCKS5 | supported | `eggress-protocol-socks` client |
| Shadowsocks | supported | `eggress-protocol-shadowsocks` client (AEAD methods only; standard TCP framing) |
| Trojan | supported | `eggress-protocol-trojan` client |
| SSH | **rejected** | Not in scope (SSH transport is out-of-scope for a proxy) |
| Direct | supported | `DirectConnector` |

## 4. Supported URI Schemes

pproxy URIs follow the pattern: `scheme://[user:pass@]host:port`

| Scheme | Example | Notes |
|--------|---------|-------|
| `http://` | `http://proxy:8080` | HTTP forward proxy |
| `https://` | `https://proxy:8443` | HTTP over TLS (maps to `http+tls`) |
| `socks4://` | `socks4://proxy:1080` | SOCKS4, host must be IPv4 |
| `socks4a://` | `socks4a://proxy:1080` | SOCKS4a, supports domain targets (alias for socks4) |
| `socks5://` | `socks5://proxy:1080` | SOCKS5 |
| `ss://` | `ss://aes-256-gcm:pass@:8388` | Shadowsocks, cipher:password in userinfo |
| `trojan://` | `trojan://pass@server:443` | Trojan, password is the auth token |
| `direct://` | `direct://` | Direct connection, no proxy |
| `ssh://` | `ssh://user@host:22` | SSH tunnel (not supported) |
| `unix://` | `unix:///path/to/socket` | Unix domain socket listener (Unix only) |

**Shadowsocks URI format**: `ss://method:password@host:port`
- The `method` and `password` are concatenated with `:` in the userinfo section.
- Example: `ss://aes-256-gcm:mypassword@10.0.0.1:8388`

**Auth in URI**: `scheme://user:pass@host:port`
- Credentials are embedded directly in the URI fragment.
- The `@` delimiter separates credentials from the endpoint.
- URI display in logs uses the redacted format (Eggress uses `****:****@`).

Eggress URI parsing matches pproxy's format for all supported schemes.
Shadowsocks and Trojan URI parsing are implemented in `eggress-uri`.

## 5. Chaining Syntax

pproxy supports proxy chaining by separating hops with `__` (double underscore):

```
socks5://hop1:1080__http://hop2:8080__direct://
```

Rules:
- Each hop is a full URI (scheme, optional credentials, host, port).
- The `__` separator is literal (not a regex or glob).
- Hops are evaluated left-to-right: the first hop is the nearest upstream, the
  last hop is the final destination.
- A single hop (no `__`) is a valid one-hop chain.

### Eggress Translation

Eggress parses `__`-separated multi-hop chains and translates them into TOML
`[[upstreams]]` entries with `__`-separated URIs. Single-hop URIs (no `__`) are
accepted as valid one-hop chains.

Per-hop protocol validation detects unsupported protocols (`ssh`, `ssr`, `unix`,
`redir`, `direct`, `h2`, `ws`, `wss`, `raw`, `tunnel`) across all hops and
emits structured diagnostics.

### Rejected Separators

Semicolon (`;`) and comma (`,`) are explicitly rejected as chain separators.
Eggress returns a structured error with a suggestion to use `__`.

### Multi-hop Backward Chains

Multi-hop backward (+in) chains are rejected as unsupported. Only forward
chaining is supported.

Example invocations:
```bash
# Two-hop chain: SOCKS5 → HTTP → direct
pproxy -l http://:8080 -r socks5://hop1:1080__http://hop2:8080__direct://

# Single-hop chain (equivalent to no chain)
pproxy -l http://:8080 -r direct://
```

Eggress uses `ProxyHopSpec` with a `Vec<ProxyHopSpec>` in the chain executor.
The `__` separator is parsed during URI/config processing. Multi-hop chains are
tested in `crates/eggress-runtime/tests/integration.rs` for up to 3 hops.
URI-level chain parsing and translation tests live in
`crates/eggress-pproxy-compat/` (14 URI tests, 8 translate tests).

## 6. Scheduler/Load-Balancing

| Feature | pproxy | Eggress |
|---------|--------|---------|
| Round-robin | Default for multiple `-r` args | Supported (`RoundRobin` scheduler) |
| Rule-based routing | `--rulefile` (regex rules) | Not translated; use TOML rules with matchers |
| Fallback | `-F` flag | `RouteActionSpec::Fallback` with group members |
| Connection reuse | `--reuse` | Not implemented; intentional non-parity because pproxy pools upstream connections across sessions while Eggress uses one upstream connection per proxy session |
| Random | Not default | Supported (`Random` scheduler) |
| Least-connections | Not available | Supported (`LeastConnections` scheduler) |
| First-available | Not available | Supported (`FirstAvailable` scheduler) |

### Scheduler Behavior Audit (Phase 12)

Detailed behavior comparison for scheduler implementations:

| Behavior | pproxy | Eggress | Notes |
|----------|--------|---------|-------|
| Round-robin default | Yes (`-s rr`) | Yes (default for groups) | Compat layer now correctly defaults to round-robin for multiple remotes; first-available for single remote |
| Round-robin state persistence | Per-connection | Global atomic cursor | Eggress cursor persists across connections (correct behavior) |
| Round-robin skips unhealthy | Implicit | Explicit health filtering | Eggress filters by health state |
| First-available | `-s fa` | `FirstAvailable` scheduler | Returns first eligible candidate |
| Random | Not default | `Random` scheduler | Pluggable RNG; deterministic variant for testing |
| Least-connections | Not available | `LeastConnections` scheduler | Uses active + in_flight count |
| Health-aware skip | Implicit via alive check | Explicit health state machine | Eggress: Unknown/Healthy/Suspect/Recovering are eligible |
| Fallback when all fail | `-F` flag (direct) | `GroupFallback` enum: Reject/Direct/UseUnhealthy | More granular control |
| Retry within group | Not documented | Not implemented (single attempt) | Eggress makes one selection per request |
| Active lease tracking | Not documented | PendingLease/ActiveLease two-phase | Precise connection accounting |

pproxy's `--rulefile` uses a line-based format with regex patterns and
destination actions. Eggress uses a TOML-based rule engine with structured
matchers (`all`, `any_of`, `not`, `cidr`, `regex`, `domain`, `port`). The
pproxy compat translator reports `--rulefile` as an unsupported feature rather
than attempting a lossy conversion.

## 7. Authentication

| Method | pproxy | Eggress | Notes |
|--------|--------|---------|-------|
| URI-embedded credentials | `user:pass@host:port` | `CredentialSpec { username, password }` | Parsed from URI |
| SOCKS5 username/password | Supported | Supported | Method 0x02 |
| HTTP Basic auth | Supported | Supported | `Proxy-Authorization: Basic ...` |
| Shadowsocks password | Supported | Supported | Password-based key derivation |
| Trojan password | Supported | Supported | SHA224 hash of password |
| Per-protocol auth config | No (URI only) | Yes (config-level) | Eggress extends beyond pproxy |

pproxy embeds credentials in the listen URI. Eggress supports both URI-embedded
credentials and config-level auth configuration with separate username/password
fields.

Auth failure behavior:

| Protocol | pproxy response | Eggress response |
|----------|----------------|-----------------|
| SOCKS5 | Method `0xFF` (no acceptable method) | Method `0xFF` |
| HTTP | `407 Proxy Authentication Required` | `407 Proxy Authentication Required` |
| Shadowsocks | Connection reset / decrypt failure | Connection reset |
| Trojan | TLS alert / connection reset | TLS alert / connection reset |

## 8. UDP Behavior

| Feature | pproxy | Eggress |
|---------|--------|---------|
| UDP relay listen | `-ul` / `--udp` flag | SOCKS5 UDP ASSOCIATE + standalone UDP (`mode = "standalone_pproxy_udp"`) |
| UDP framing | Custom pproxy protocol | SOCKS5-compatible datagram header |
| UDP upstream | `-ur` flag or chained upstreams | Direct, SOCKS5 upstream, Shadowsocks upstream |
| UDP ASSOCIATE (server) | Not supported as server | Supported (full RFC 1928) |
| UDP ASSOCIATE (client) | Supported when connecting through upstream | Supported |
| Direct UDP forwarding | `-r direct` | Supported |
| Shadowsocks UDP upstream | supported | Supported (standard AEAD; single-hop) |
| Standalone UDP mode | `-ul` without TCP control | Supported (`mode = "standalone_pproxy_udp"`) |

pproxy's UDP relay protocol:
- Listen on a separate UDP socket specified by `-ul`.
- Clients send datagrams with a custom header: `[RSV:2][FRAG:1][ATYP:1][ADDR:N][PORT:2][DATA:M]`.
- This header format is identical to the SOCKS5 UDP ASSOCIATE datagram format.
- However, pproxy does **not** require a SOCKS5 TCP control connection to set up
  the UDP relay. The `-ul` socket is independent.

Eggress's UDP relay:
- SOCKS5 UDP ASSOCIATE requires a TCP control connection that sends the
  `UDP ASSOCIATE` command (0x03). The server replies with a relay address.
- Standalone UDP mode (`mode = "standalone_pproxy_udp"`) accepts
  SOCKS5-framed UDP datagrams directly on the UDP socket without requiring
  a SOCKS5 TCP control connection, matching pproxy's `-ul` behavior.
- UDP datagrams use the standard SOCKS5 UDP ASSOCIATE framing in both modes.
- The standalone mode is fully compatible with pproxy's standalone UDP relay.

**Differential test result**: Both relay UDP successfully. The wire framing is
the same for the datagram payload. With standalone mode enabled, the setup
mechanism is also compatible (no TCP control connection required).
See `differential_socks5_udp_associate` test.

## 9. Encryption

| Feature | pproxy | Eggress |
|---------|--------|---------|
| Shadowsocks AEAD | `aes-128-gcm`, `aes-256-gcm`, `chacha20-ietf-poly1305` | Supported |
| Shadowsocks stream | `aes-128-ctr`, `aes-192-ctr`, `aes-256-ctr`, etc. | **Rejected** (insecure, deprecated) |
| Trojan over TLS | rustls/ssl | Supported (rustls) |
| HTTPS wrapping | Supported | Planned (Phase 6) |
| TLS certificate options | `--ssl certfile[,keyfile]` | `tls` field in `ProxyHopSpec` |
| TLS verification | Configurable | System root store by default; insecure mode API-only |

Shadowsocks AEAD ciphers (all supported by both):

| Cipher | Key length | Nonce length | Tag length |
|--------|-----------|-------------|-----------|
| `aes-128-gcm` | 16 bytes | 12 bytes | 16 bytes |
| `aes-256-gcm` | 32 bytes | 12 bytes | 16 bytes |
| `chacha20-ietf-poly1305` | 32 bytes | 12 bytes | 16 bytes |

Shadowsocks stream ciphers (pproxy supports, Eggress rejects):

| Cipher | Status | Reason |
|--------|--------|--------|
| `aes-128-ctr` | Rejected | No authentication, vulnerable to bit-flipping |
| `aes-192-ctr` | Rejected | No authentication |
| `aes-256-ctr` | Rejected | No authentication |
| `aes-128-cfb` | Rejected | No authentication |
| `aes-192-cfb` | Rejected | No authentication |
| `aes-256-cfb` | Rejected | No authentication |
| `rc4-md5` | Rejected | Known weaknesses |
| `chacha20-ietf` | Rejected | No authentication |
| `xchacha20-ietf-poly1305` | Rejected | Not in AEAD standard set |

## 10. CLI Flags

Common invocation forms:

```bash
# HTTP forward proxy
pproxy -l http://:8080 -r direct

# SOCKS5 proxy chaining through HTTP upstream
pproxy -l socks5://:1080 -r http://proxy:8080

# Shadowsocks server
pproxy -l ss://aes-256-gcm:pass@:8388 -r direct

# UDP relay alongside SOCKS5
pproxy -l http://:8080 -ul socks5://:1080

# Rule-based routing
pproxy --rulefile rules.txt

# Multi-hop chain
pproxy -l http://:8080 -r socks5://hop1:1080__direct://

# With auth
pproxy -l socks5://user:pass@:1080 -r direct
```

| Flag | Description | Eggress equivalent |
|------|-------------|-------------------|
| `-l` | Listen address (URI) | `[[listeners]]` in TOML config |
| `-r` | Remote/upstream address (URI) | `[[upstreams]]` in TOML config |
| `-ul` | UDP listen address | `[[listeners]]` with `mode = "standalone_pproxy_udp"` + UDP config |
| `-ur` | UDP remote address | UDP upstream config with transport-matching rule |
| `-b` | Block regex rules (filter hostnames) | Not supported; use eggress TOML routing rules |
| `-s` | Scheduling algorithm (`fa`, `rr`, `rc`, `lc`) | `scheduler` in upstream group TOML |
| `-a` | Alive check interval (seconds) | Health probe config in TOML |
| `-v` | Verbose logging | `RUST_LOG=debug` environment variable |
| `--ssl` | TLS cert/key file (`certfile[,keyfile]`) | TLS config in eggress TOML |
| `--daemon` | Run as daemon | Not supported (use systemd/supervisord) |
| `--log` | Log file path | Not supported (use tracing-subscriber) |
| `--pac` | PAC file path | PAC serving via admin API |
| `--sys` | Set system proxy (mac/windows) | Not supported |
| `--test` | Test all remote proxies and exit | `eggress route test` command |

## 11. Python Library Usage

pproxy can be used as a Python library:

```python
import pproxy

# Create server from URI
server = pproxy.Server('socks5://:1080')

# Create server from multiple URIs (chaining)
server = pproxy.Server(['socks5://:1080', 'http://proxy:8080'])

# Start serving
import asyncio
loop = asyncio.get_event_loop()
loop.run_until_complete(pproxy.serve(['socks5://:1080']))

# Protocol handler registration (internal)
# pproxy registers protocol handlers via its server object
```

Key API surface:
- `pproxy.Server(uri)` — creates a server from a URI string or list.
- `pproxy.serve([uri])` — convenience function to start a server.
- Protocol handlers are registered internally by the server based on URI scheme.

Eggress exposes a Python library API via the `eggress` package (PyO3 bindings
wrapping `eggress-embed`). This provides `EggressConfig`, `EggressService`,
`EggressHandle`, pproxy translation helpers (`translate_pproxy_args`,
`translate_pproxy_uri`), and convenience APIs (`start_pproxy`,
`from_pproxy_args`). The Python API is not a 1:1 match for pproxy's
`pproxy.Server()` — it uses explicit lifecycle management (start/shutdown)
rather than asyncio server objects.

## 12. Error and Failure Behavior

| Error condition | pproxy behavior | Eggress behavior |
|----------------|----------------|-----------------|
| Connection refused (upstream) | SOCKS5 reply `0x05` (connection refused) | SOCKS5 reply `0x05` |
| DNS resolution failure | SOCKS5 reply `0x04` (host unreachable) | SOCKS5 reply `0x04` |
| Auth failure (SOCKS5) | Method `0xFF` (no acceptable method) | Method `0xFF` |
| Auth failure (HTTP) | `407 Proxy Authentication Required` | `407 Proxy Authentication Required` |
| Upstream failure (HTTP) | `502 Bad Gateway` | `502 Bad Gateway` |
| Timeout | Connection reset | Connection reset |
| Invalid SOCKS version | Connection reset | Connection reset |
| Oversized request | Connection reset | Connection reset (bounded buffers) |

pproxy error behavior is generally standard-compliant. Eggress matches the
observed behavior for all tested error conditions.

## 13. Observed Behaviors (from Differential Tests)

The following behaviors are confirmed by the 7 differential tests in
`crates/eggress-cli/tests/differential_pproxy.rs`:

### 13.1 SOCKS5 CONNECT (TCP echo)

- **Test**: `differential_socks5_connect_tcp_echo`
- **Result**: Byte-exact payload match.
- **Details**: Both pproxy and Eggress relay the same payload through SOCKS5
  CONNECT to a TCP echo server. The echoed payload is identical.

### 13.2 HTTP CONNECT (TCP echo)

- **Test**: `differential_http_connect_tcp_echo`
- **Result**: Byte-exact payload match.
- **Details**: Both pproxy and Eggress relay the same payload through HTTP
  CONNECT to a TCP echo server. The echoed payload is identical.

### 13.3 SOCKS5 UDP ASSOCIATE

- **Test**: `differential_socks5_udp_associate`
- **Result**: Both relay UDP successfully; compatible in standalone mode.
- **Details**: pproxy uses its own UDP relay protocol (standalone `-ul` socket),
  while Eggress supports both SOCKS5 UDP ASSOCIATE (TCP-controlled) and
  standalone mode (`mode = "standalone_pproxy_udp"`). Both deliver the
  UDP payload to the echo server and receive the echo back. The datagram
  framing (header bytes) is compatible. In standalone mode, the setup
  mechanism is also compatible (no TCP control connection required).

### 13.4 SOCKS5 → HTTP Chain

- **Test**: `differential_socks5_through_http_upstream`
- **Result**: Payload matches direct-through-proxy.
- **Details**: Eggress chains SOCKS5 inbound through pproxy as HTTP upstream.
  The echoed payload matches sending directly through pproxy's HTTP interface.

### 13.5 SOCKS5 → SOCKS5 Chain

- **Test**: `differential_socks5_through_socks5_upstream`
- **Result**: Payload matches direct-through-proxy.
- **Details**: Eggress chains SOCKS5 inbound through pproxy as SOCKS5 upstream.
  The echoed payload matches sending directly through pproxy's SOCKS5 interface.

### 13.6 SOCKS5 Auth Failure

- **Test**: `differential_socks5_auth_failure`
- **Result**: Both reject unauthenticated connections.
- **Details**: pproxy with auth configured rejects connections without valid
  credentials. Eggress with `auth_required: true` rejects the same. Both
  produce connection-level failures.

### 13.7 HTTP Auth Failure

- **Test**: `differential_http_auth_failure`
- **Result**: Both reject unauthenticated connections.
- **Details**: pproxy with auth configured rejects HTTP connections without valid
  credentials. Eggress with `auth_required: true` rejects the same.

## 14. Behaviors Eggress Will Intentionally Reject

The following pproxy features are explicitly out of scope for Eggress. These
are not gaps to be filled — they are deliberate exclusions based on security
policy, architecture, or scope.

| Feature | pproxy support | Reason for rejection |
|---------|---------------|---------------------|
| macOS PF transparent proxy | `redir://` on macOS | Not implemented. Use pfctl with a standard listener instead. Linux transparent proxy via `SO_ORIGINAL_DST` is supported. |
| Shadowsocks stream ciphers | `aes-*-ctr`, `aes-*-cfb`, `rc4-md5`, etc. | No authentication. Vulnerable to bit-flipping and replay attacks. Deprecated by the Shadowsocks community. Produces `LegacyMethodUnsupported` error with a message suggesting AEAD methods. |
| ShadowsocksR (SSR) | Supported in some forks | Non-standard extension. No RFC. Conflicts with upstream Shadowsocks design. SSR URIs are parsed by the pproxy compat layer and produce `UnsupportedFeature` diagnostics. See ADR at `docs/adr/ADR_legacy_shadowsocks_ssr_compatibility.md`. |
| HTTP/2 CONNECT | pproxy h2 scheme | **Supported** — synthetic tests. H2 CONNECT server and client implemented. |
| WebSocket tunnels | pproxy ws/wss schemes | **Supported** — synthetic tests. WS/WSS tunnel server and client implemented. |
| Raw tunnels | pproxy raw/tunnel schemes | **Supported** — synthetic tests. Fixed-target TCP tunnel implemented. |
| QUIC transport | Not in pproxy | **Deferred** — ADR at docs/adr/ADR_quic_h3_pproxy_parity.md. pproxy behavior experimental, dependency significant. |
| HTTP/3 | Not in pproxy | **Deferred** — ADR at docs/adr/ADR_quic_h3_pproxy_parity.md. |
| SSH transport | `ssh://` | Intentional non-parity. SSH is a general-purpose encrypted tunnel, not a proxy protocol. URIs recognized for clean diagnostics. See ADR at `docs/adr/ADR_ssh_upstream_parity.md`. |
| Reverse/backward proxying | pproxy `bind`, `listen`, backward URI forms | **Supported** — reverse control channel with raw-relay control channel (Phase 27). TCP only; one session per control channel; no multiplexing. |
| Plugin system | pproxy has plugin hooks | Out of scope. Eggress uses a fixed protocol set with TOML configuration. |
| Malformed input leniency | pproxy may accept some malformed inputs | Eggress rejects malformed inputs strictly. Security over compatibility. |
| Insecure TLS defaults | `--insecure` flag | Eggress requires TLS verification by default. Insecure mode is API-only, not configurable via TOML. |

## 14.2 Transparent Proxy (Phase 25)

Eggress supports transparent TCP proxying on Linux via `SO_ORIGINAL_DST`. This
retrieves the original destination of a connection redirected by iptables/nftables
REDIRECT rules, enabling interception without client-side proxy configuration.

### Requirements

- **Platform**: Linux only (kernel 2.4+ for `SO_ORIGINAL_DST`)
- **Privileges**: Requires `CAP_NET_ADMIN` capability or root
- **Firewall**: iptables REDIRECT rule or nftables equivalent
- **macOS PF**: Not implemented; use pfctl with a standard listener

### Configuration

```toml
[[listeners]]
name = "transparent-in"
protocols = ["http", "socks5"]

[listeners.transparent]
enabled = true
protocol = "redir"
```

### iptables/nftables setup

```bash
# iptables REDIRECT
iptables -t nat -A PREROUTING -p tcp --dport 80 -j REDIRECT --to-ports 8080
iptables -t nat -A PREROUTING -p tcp --dport 443 -j REDIRECT --to-ports 8080

# nftables equivalent
nft add rule ip nat PREROUTING tcp dport { 80, 443 } redirect to :8080
```

### Security considerations

- Original destination is extracted from kernel socket options; trusted kernel input
- Loop prevention: connections destined to the proxy's own listen address are rejected
- No special handling for localhost redirects

## 14.3 Unix Domain Socket Listeners (Phase 25)

Eggress supports listening on Unix domain sockets for local-only deployments.

### Requirements

- **Platform**: Unix only (Linux, macOS, BSDs)
- **Not available**: Windows

### Configuration

```toml
[[listeners]]
name = "unix-in"
protocols = ["http", "socks5"]

[listeners.unix]
path = "/run/eggress/proxy.sock"
unlink_existing = true
mode = 0o660
```

### Socket management

- `unlink_existing`: removes an existing socket file before binding (prevents stale socket errors)
- `mode`: Unix file permissions for the socket (default `0o660`)
- Socket file ownership follows the process user/group
- On shutdown, the socket file is not removed (operator-managed)

### Security considerations

- Socket file permissions control who can connect
- No built-in ACL beyond filesystem permissions
- Loop prevention: same as TCP listeners (reject connections to own address)

## 14.5 Remaining Protocol Audit

Phase 11 classified every remaining pproxy protocol/scheme. The complete audit is in `docs/PARITY_MATRIX.md` under "Remaining Protocol Audit".

### Summary

- **Implemented as compatible**: HTTP, HTTPS (HTTP+TLS), SOCKS4, SOCKS4a, SOCKS5, HTTP forward proxy (persistent sessions), Shadowsocks upstream and inbound listener (AEAD), Trojan upstream, direct upstream, standalone UDP (`-ul`/`-ur`)
- **Implemented as supported**: Transparent TCP proxy (Linux, `redir://`), Unix domain socket listeners (Unix, `unix://`)
- **Implemented as supported (Phase 26)**: HTTP/2 CONNECT, WebSocket tunnels, Raw fixed-target tunnels, TLS ALPN negotiation
- **Implemented as supported (Phase 27)**: Reverse/backward proxying (raw-relay control channel, `bind://`/`listen://`/`backward://`/`rebind://` URI forms, `+in` modifier, auth, reconnect with backoff)
- **Deferred**: QUIC, HTTP/3 (ADR at `docs/adr/ADR_quic_h3_pproxy_parity.md`)
- **Intentional non-parity**: SSH, macOS PF transparent proxy, Shadowsocks stream ciphers, ShadowsocksR, `--daemon`, `--ssl` listener, `-b` block rules, `--rulefile`, `--reuse`, `--log`, `--sys`, multi-hop UDP
- **Partial**: Trojan inbound listener

### Diagnostic behavior

When an unsupported protocol or feature is encountered in pproxy compat mode, eggress produces structured `UnsupportedFeature` or `CompatError` diagnostics. See `PARITY_MATRIX.md` for the complete diagnostic table.

## 14.6 Reverse/Backward Proxying (Phase 27)

pproxy supports reverse proxying via `bind`, `listen`, `backward`, and `rebind`
URI forms. In a reverse proxy topology, a **control client** establishes an
outbound connection to a **reverse acceptor**, and the acceptor dispatches
externally-accepted connections back through the control channel for the control
client to handle. This enables a proxy behind NAT to serve remote clients
without port forwarding.

### Supported URI Forms

| URI scheme | Role | Description |
|------------|------|-------------|
| `bind://` | Acceptor | Listen on a port and accept connections |
| `listen://` | Acceptor | Alias for bind |
| `backward://` | Control client | Dial out to acceptor; receive streams |
| `rebind://` | Control client | Alias for backward |

The `+in` modifier on any protocol scheme activates reverse/backward mode:

```
scheme+in://[auth@]host:port
```

Multiple `+in` tokens stack to create parallel control connections:

```
socks5+in+in://acceptor:1080    # 2 parallel backward connections
socks5+in+in+in://acceptor:1080 # 3 parallel backward connections
```

### Authentication

Authentication is optional and specified in the URI fragment:

```
socks5+in://user:pass@acceptor:1080
```

The control client sends raw `user:pass` bytes in the initial handshake. The
acceptor compares these bytes against its configured credentials. No challenge-
response or hashing is performed. TLS (`+ssl`) is available but not default.

### Control Channel Protocol

pproxy's reverse control protocol uses a minimal byte-exchange handshake
followed by raw TCP relay:

1. Control client connects to acceptor via TCP
2. Control client sends auth credentials (raw bytes, no framing)
3. Acceptor sends 1-byte response (`0x00` = reject, else = accept)
4. If accepted, the channel becomes a raw TCP bidirectional relay

**There is no stream multiplexing.** Each backward connection carries exactly
one proxy session. When that session ends, the control connection closes and the
backward client reconnects. To handle concurrent clients, multiple `+in` tokens
create parallel control connections.

### Limitations

| Limitation | Notes |
|-----------|-------|
| TCP only | No UDP reverse proxying; UDP listeners operate independently |
| No multiplexing | One proxy session per control connection; use `+in` stacking for concurrency |
| No TLS by default | Control channel is plaintext; TLS available via `+ssl` |
| No flow control | No per-stream backpressure |
| No private network restrictions | Accepts any target address by default |

### Configuration Model

Eggress extends the pproxy reverse proxy model with TOML configuration:

```toml
# Reverse server (acceptor side)
[[reverse_servers]]
bind = "0.0.0.0:8443"
protocol = "http"              # or "socks5", "socks4"
allow_bind = ["127.0.0.1", "::1"]
password = "secret"            # optional

# Reverse client (control/initiator side)
[[reverse_clients]]
remote = "example.com:8443"
password = "secret"
tls = false                   # true for TLS-wrapped control channel
backoff_initial_ms = 1000
backoff_max_ms = 30000
```

### Reconnect Behavior

When the control connection drops, the backward client automatically reconnects
with exponential backoff (initial 1s, max 30s, capped). On reconnect, the
client re-authenticates and the control channel is re-established. There is no
explicit listener re-registration — the acceptor accepts the new control
connection and resumes dispatching.

### Security Considerations

| Property | Default | Notes |
|----------|---------|-------|
| Encryption | None | Control channel is plaintext TCP |
| Authentication | Optional | Raw `user:pass` in URI, compared as bytes |
| Listener bind | Configurable | Acceptor restricts bind addresses via `allow_bind` |
| Private network | Not restricted | No ACL on target addresses by default |
| Stream limit | Unbounded | No per-control limit on concurrent sessions |

### References

- `docs/protocols/REVERSE_PROXYING.md` — Full protocol specification
- `docs/adr/ADR_reverse_backward_proxying.md` — Design decision record
- `crates/eggress-protocol-reverse/` — Implementation

## 15. Open Items and Probes

The following items need further investigation or testing to confirm behavior:

| Item | Status | Notes |
|------|--------|-------|
| pproxy SOCKS5 BIND command support | `needs-probe` | pproxy may support BIND (0x02) — not tested |
| pproxy UDP ASSOCIATE as SOCKS5 server | `needs-probe` | pproxy does not appear to support UDP ASSOCIATE as a SOCKS5 server command; uses standalone `-ul` instead |
| pproxy Trojan password hashing | `needs-probe` | Whether pproxy uses SHA224 (standard Trojan) or a variant |
| pproxy HTTP forward proxy (non-CONNECT) | resolved | pproxy supports plain HTTP forwarding with persistent connections; eggress now matches (Phase 19) |
| pproxy multi-hop chain behavior | `needs-probe` | Behavior for chains longer than 2 hops (error handling, protocol negotiation) |
| pproxy connection reuse semantics | `needs-probe` | How `--reuse` interacts with chained upstreams and health state |
| pproxy `--rulefile` format details | `needs-probe` | Exact syntax for rule-file entries and regex patterns |
| pproxy SOCKS4a domain resolution | resolved | pproxy resolves domains at the SOCKS4a server; eggress matches (Phase 19) |

## 16. pproxy Compatibility CLI Layer

Eggress provides a pproxy compatibility CLI layer that translates pproxy-style
command-line arguments into equivalent TOML configuration. This allows users
familiar with pproxy to migrate incrementally without rewriting their mental
model.

### Available Commands

| Command | Description |
|---------|-------------|
| `eggress pproxy translate` | Convert pproxy CLI args to TOML configuration |
| `eggress pproxy check` | Validate translated configuration |
| `eggress pproxy run` | Run eggress with pproxy-style arguments (translated internally) |

### Supported Flags

The compat layer translates: `-l`, `-r`, `-s`, `-v`, `-a`.

Flags with direct mapping produce TOML configuration. Flags without a direct
mapping (e.g., `--daemon`, `--ssl`, `-b`) emit a warning or
unsupported feature diagnostic. Unknown flags emit an `unknown-flag` warning.
Translation exits nonzero when unsupported features are present.
The `-ul` and `-ur` flags are now supported and generate standalone UDP
configuration with `mode = "standalone_pproxy_udp"`.

### Migration Guidance

For detailed migration instructions, see [`docs/PPROXY_MIGRATION.md`](./PPROXY_MIGRATION.md).

## 16.1 CLI Compatibility: Exit Codes, JSON, and Diagnostics (Phase 28)

### Exit Codes

pproxy uses a single exit code (`1`) for all error conditions. Eggress
provides granular exit codes to enable scripted error handling:

| Code | Name | When produced |
|------|------|---------------|
| 0 | `success` | Command succeeded |
| 1 | `runtime_failure` | Internal runtime error |
| 2 | `cli_parse_error` | Unknown flags or bad argument syntax |
| 3 | `config_validation` | Translated config failed validation |
| 4 | `bind_failure` | Could not bind to listen address |
| 5 | `unsupported_feature` | Unsupported pproxy feature encountered |
| 6 | `platform_missing` | OS-specific capability not available |
| 7 | `external_dependency` | Required external tool not found |
| 130 | `interrupted_by_sigint` | SIGINT received |
| 143 | `terminated_by_sigterm` | SIGTERM received |

`eggress pproxy check` always exits 0 regardless of findings — it reports
parity tiers without failing. This differs from `translate` and `run` which
use exit code 5 when unsupported features are present.

### JSON Output (`--json`)

`pproxy check --json` emits machine-readable output with the following
structure:

```json
{
  "tier": "supported",
  "diagnostics": [
    {
      "code": "unsupported_flag",
      "feature_id": "daemon",
      "tier": "unsupported",
      "message": "--daemon mode is not supported",
      "suggestion": "use systemd or process manager"
    }
  ],
  "features": [
    {
      "name": "daemon",
      "tier": "unsupported",
      "diagnostic_code": "unsupported_flag"
    }
  ],
  "raw_args": ["-l", "socks5://127.0.0.1:1080"],
  "parsed_uris": {
    "listeners": ["socks5://****:****@127.0.0.1:1080"],
    "remotes": []
  }
}
```

The `tier` field is computed from the aggregate of warnings and unsupported
features:
- `compatible` — no warnings, no unsupported features
- `supported` — warnings present but no unsupported features
- `unsupported` — one or more unsupported features

Credentials in `parsed_uris` are always redacted.

### Structured Diagnostics Taxonomy

The `StructuredDiagnostic` type carries stable `DiagnosticCode` values
designed for JSON output, test assertions, and documentation cross-references.
Each code corresponds to a class of translation issue:

| DiagnosticCode | Meaning | Example triggers |
|----------------|---------|------------------|
| `unsupported_protocol` | Protocol/scheme not implemented | `ssh://`, unknown scheme |
| `unsupported_flag` | Flag not mappable to eggress | `--daemon`, `--reuse`, `-b`, unknown flags |
| `unsupported_platform` | OS-specific capability missing | `unix://` on Windows |
| `unsupported_transport_wrapper` | Transport wrapper not supported | `+tls` in wrong context |
| `unsupported_security_sensitive_legacy_feature` | Insecure legacy feature | SSR URIs, obfs plugins |
| `invalid_uri_syntax` | Malformed URI or arguments | Bad host, missing port |
| `invalid_chainComposition` | Invalid protocol chain | Conflicting protocols |
| `missing_target` | Required address missing | No `-l` argument |
| `missing_credential` | Required password/key missing | URI without password |
| `invalid_cipher_method` | Insecure cipher specified | `aes-128-ctr`, `rc4-md5` |
| `bind_failure` | Listen address bind failed | Port in use |
| `privilege_capability_missing` | OS capability required | `SO_ORIGINAL_DST` without root |
| `external_dependency_missing` | External tool required | `ssserver` not found |

Each diagnostic also carries optional `feature_id`, `tier`, `message`, and
`suggestion` fields. Diagnostics are serializable to JSON and produced by
the `eggress-pproxy-compat` crate.

### CLI Inventory

pproxy exposes 14 CLI flags/options. The eggress compat layer maps 7 of them:

| pproxy flag | eggress mapping | Status |
|-------------|----------------|--------|
| `-l` | `[[listeners]]` in TOML | Mapped |
| `-r` | `[[upstreams]]` in TOML | Mapped |
| `-ul` | standalone UDP listener | Mapped |
| `-ur` | UDP upstream config | Mapped |
| `-s` | `scheduler` in group TOML | Mapped |
| `-v` | `RUST_LOG=debug` | Mapped |
| `-a` | Health probe config | Mapped |
| `--daemon` | rejected | Intentional non-parity |
| `--ssl` | TLS config in TOML | Intentional non-parity |
| `-b` | rejected | Intentional non-parity |
| `--rulefile` | rejected | Intentional non-parity |
| `--reuse` | rejected | Intentional non-parity |
| `--log` | rejected | Intentional non-parity |
| `--sys` | rejected | Intentional non-parity |

For the complete inventory with diagnostic codes, see
[`docs/PPROXY_MIGRATION.md`](./PPROXY_MIGRATION.md).

### Logging and Verbosity (Section 16.2)

pproxy provides two logging controls:

| Flag | pproxy behavior | Eggress handling |
|------|----------------|-----------------|
| `-v` / `--verbose` | Enable verbose/debug output to stderr | **Partial** — compat layer emits a warning suggesting `RUST_LOG=debug` |
| `--log FILE` | Write log output to a file | **Not supported** — eggress logs to stderr only; redirect via shell if needed |

Eggress uses `tracing-subscriber` with `EnvFilter` for log control:

| Feature | Implementation |
|---------|---------------|
| Default log level | `info` (when `RUST_LOG` is unset) |
| Verbosity control | `RUST_LOG=<level>` environment variable (`debug`, `trace`, etc.) |
| Log format | `--log-format` flag: `pretty` (default), `json`, `compact` |
| Log destination | stderr only; no built-in file output |
| Credential redaction | `****:****@` format at all log levels |

**pproxy `-v` mapping**: The compat layer parses `-v` as a known flag and
emits a `verbose-mode` warning with migration guidance. It does not
generate TOML configuration — the user must set `RUST_LOG=debug` in their
environment. This is an intentional non-parity because eggress uses the
standard Rust/tracing ecosystem for log control rather than a binary
verbose flag.

**pproxy `--log FILE` mapping**: The compat layer parses `--log` as a known
flag but does not produce a diagnostic — the flag is silently dropped. Users
who need file-based logging should redirect stderr:
```bash
RUST_LOG=info eggress --config config.toml > access.log 2>&1
```

## 17. References

- [pproxy GitHub repository](https://github.com/nimlang/pproxy)
- [RFC 1928 — SOCKS Protocol Version 5](https://datatracker.ietf.org/doc/html/rfc1928)
- [RFC 7231 — HTTP/1.1 Semantics (CONNECT)](https://datatracker.ietf.org/doc/html/rfc7231)
- [Shadowsocks AEAD Ciphers](https://shadowsocks.org/en/spec/AEAD-Ciphers.html)
- [Trojan Protocol](https://trojan-gfw.github.io/trojan/)
- [Eggress Parity Matrix](./PARITY_MATRIX.md)
- [Eggress Differential Tests](../crates/eggress-cli/tests/differential_pproxy.rs)
- [pproxy Migration Guide](./PPROXY_MIGRATION.md)

## 18. Scheduler Semantics (Phase 12)

### Round-Robin

pproxy uses round-robin as the default scheduler when multiple `-r` arguments are
provided. The scheduler cycles through upstreams in order.

Eggress implements round-robin with a global atomic cursor that persists across
connections. Each `select()` call advances the cursor and returns the next eligible
upstream. Ineligible upstreams (disabled or unhealthy) are skipped.

Key difference: pproxy resets its scheduling state on reload; eggress preserves
cursor state across config reloads for unchanged upstream groups.

The eggress pproxy compat layer now correctly defaults to round-robin when
translating multiple `-r` arguments (previously defaulted to first-available).

### First-Available

pproxy supports first-available via `-s fa`. The first healthy upstream in the
list is used.

Eggress matches this behavior with `FirstAvailableScheduler`, which returns the
first candidate passing eligibility checks (enabled + healthy/suspect/recovering/unknown).

### Least-Connections

pproxy does not support least-connections scheduling.

Eggress implements `LeastConnectionsScheduler` which selects the upstream with
the minimum `current_load()` (active connections + in-flight connections). Ties
are broken by earlier position in the candidate list.

### Health-Aware Filtering

pproxy performs alive checks (`-a` flag) and removes failed upstreams from
rotation temporarily.

Eggress uses a state machine with hysteresis:
- Unknown → Healthy (after N consecutive successes)
- Healthy → Suspect → Unhealthy (after M consecutive failures)
- Unhealthy → Recovering → Healthy (after successes)
- Disabled is terminal (ignores probes)

Only Unknown, Healthy, Suspect, and Recovering states are eligible for selection.
Unhealthy and Disabled states are filtered out.

### Fallback Behavior

pproxy uses `-F` flag to enable direct fallback when all upstreams fail.

Eggress provides three fallback modes via `GroupFallback`:
- `Reject`: Return error if no eligible upstream (default)
- `Direct`: Fall back to direct connection
- `UseUnhealthy`: Include unhealthy-but-enabled members as last resort

### Retry Behavior

pproxy does not document explicit retry behavior for failed connections within
a group. Based on observation, pproxy makes a single connection attempt per
request.

Eggress matches this behavior: a single upstream is selected per request. If
the connection fails, the error is returned to the client. No automatic retry
across upstreams is performed. This avoids amplifying load during upstream
outages and keeps behavior predictable.
