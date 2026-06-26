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
| SOCKS4/SOCKS4a | supported | `eggress-protocol-socks` |
| SOCKS5 | supported | `eggress-protocol-socks` |
| Shadowsocks | supported | `eggress-protocol-shadowsocks` |
| Trojan | supported | `eggress-protocol-trojan` |
| Redir | **rejected** | Requires root, kernel hooks (`SO_ORIGINAL_DST`) |
| Unix socket | **rejected** | Not in scope |

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
| SOCKS4/SOCKS4a | supported | `eggress-protocol-socks` client |
| SOCKS5 | supported | `eggress-protocol-socks` client |
| Shadowsocks | supported | `eggress-protocol-shadowsocks` client |
| Trojan | supported | `eggress-protocol-trojan` client |
| SSH | **rejected** | Not in scope (SSH transport is out-of-scope for a proxy) |
| Direct | supported | `DirectConnector` |

## 4. Supported URI Schemes

pproxy URIs follow the pattern: `scheme://[user:pass@]host:port`

| Scheme | Example | Notes |
|--------|---------|-------|
| `http://` | `http://proxy:8080` | HTTP forward proxy |
| `https://` | `https://proxy:8443` | HTTP over TLS |
| `socks4://` | `socks4://proxy:1080` | SOCKS4, host must be IPv4 |
| `socks4a://` | `socks4a://proxy:1080` | SOCKS4a, supports domain targets |
| `socks5://` | `socks5://proxy:1080` | SOCKS5 |
| `ss://` | `ss://aes-256-gcm:pass@:8388` | Shadowsocks, cipher:password in userinfo |
| `trojan://` | `trojan://pass@server:443` | Trojan, password is the auth token |
| `direct://` | `direct://` | Direct connection, no proxy |
| `ssh://` | `ssh://user@host:22` | SSH tunnel |
| `unix://` | `unix:///path/to/socket` | Unix domain socket |

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
- A single hop is equivalent to a chain of length one.

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

## 6. Scheduler/Load-Balancing

| Feature | pproxy | Eggress |
|---------|--------|---------|
| Round-robin | Default for multiple `-r` args | Supported (`RoundRobin` scheduler) |
| Rule-based routing | `--rulefile` (regex rules) | TOML rules with matchers |
| Fallback | `-F` flag | `RouteActionSpec::Fallback` with group members |
| Connection reuse | `--reuse` | Supported (persistent upstream connections) |
| Random | Not default | Supported (`Random` scheduler) |
| Least-connections | Not available | Supported (`LeastConnections` scheduler) |
| First-available | Not available | Supported (`FirstAvailable` scheduler) |

pproxy's `--rulefile` uses a line-based format with regex patterns and
destination actions. Eggress uses a TOML-based rule engine with structured
matchers (`all`, `any_of`, `not`, `cidr`, `regex`, `domain`, `port`).

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
| UDP relay listen | `-ul` / `--udp` flag | SOCKS5 UDP ASSOCIATE listener |
| UDP framing | Custom pproxy protocol | SOCKS5 UDP ASSOCIATE header (RFC 1928) |
| UDP upstream | `-ur` flag or chained upstreams | SOCKS5 upstream UDP ASSOCIATE |
| UDP ASSOCIATE (server) | Not supported as server | Supported (full RFC 1928) |
| UDP ASSOCIATE (client) | Supported when connecting through upstream | Supported |
| Direct UDP forwarding | `-r direct` | Supported |

pproxy's UDP relay protocol:
- Listen on a separate UDP socket specified by `-ul`.
- Clients send datagrams with a custom header: `[RSV:2][FRAG:1][ATYP:1][ADDR:N][PORT:2][DATA:M]`.
- This header format is identical to the SOCKS5 UDP ASSOCIATE datagram format.
- However, pproxy does **not** require a SOCKS5 TCP control connection to set up
  the UDP relay. The `-ul` socket is independent.

Eggress's UDP relay:
- SOCKS5 UDP ASSOCIATE requires a TCP control connection that sends the
  `UDP ASSOCIATE` command (0x03). The server replies with a relay address.
- UDP datagrams use the standard SOCKS5 UDP ASSOCIATE framing.
- This is a meaningful protocol difference: pproxy's UDP relay is a standalone
  relay, while Eggress's is tied to a SOCKS5 session.

**Differential test result**: Both relay UDP successfully. The wire framing is
the same for the datagram payload, but the setup mechanism differs (standalone
vs. SOCKS5 session). See `differential_socks5_udp_associate` test.

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
| `-ul` | UDP listen address | `[[listeners]]` with `protocol = "socks5"` + UDP config |
| `-ur` | UDP remote address | UDP upstream config |
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

Eggress does not expose a Python library API. It is a standalone Rust binary
with a TOML configuration interface. This is an intentional architectural
difference: Eggress targets production deployments with config-driven operation,
not scripting/embedding.

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
- **Result**: Both relay UDP successfully; framing differs.
- **Details**: pproxy uses its own UDP relay protocol (standalone `-ul` socket),
  while Eggress uses SOCKS5 UDP ASSOCIATE (TCP-controlled). Both deliver the
  UDP payload to the echo server and receive the echo back. The datagram
  framing (header bytes) is compatible, but the setup mechanism differs.

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
| Transparent/redir proxy | `redir://` (Linux) | Requires root privileges and kernel hooks (`SO_ORIGINAL_DST`). Not portable. |
| Shadowsocks stream ciphers | `aes-*-ctr`, `aes-*-cfb`, `rc4-md5`, etc. | No authentication. Vulnerable to bit-flipping and replay attacks. Deprecated by the Shadowsocks community. |
| ShadowsocksR (SSR) | Supported in some forks | Non-standard extension. No RFC. Conflicts with upstream Shadowsocks design. |
| QUIC transport | Not in pproxy (but mentioned) | Out of scope. HTTP/3 and QUIC are transport-layer concerns, not proxy protocol features. |
| HTTP/3 | Not in pproxy (but mentioned) | Out of scope. Requires QUIC transport and different connection semantics. |
| WebSocket tunnels | Not in pproxy | Out of scope. WebSocket is a transport wrapper; not a proxy protocol. |
| SSH transport | `ssh://` | Out of scope. SSH is a general-purpose encrypted tunnel, not a proxy protocol. Adds significant dependency weight. |
| Reverse/backward proxying | Not in pproxy | Eggress is a forward proxy only. Reverse proxy is a different product category. |
| Unix domain sockets | `unix://` | Out of scope for current release. May be reconsidered for local-only deployments. |
| Plugin system | pproxy has plugin hooks | Out of scope. Eggress uses a fixed protocol set with TOML configuration. |
| Malformed input leniency | pproxy may accept some malformed inputs | Eggress rejects malformed inputs strictly. Security over compatibility. |
| Insecure TLS defaults | `--insecure` flag | Eggress requires TLS verification by default. Insecure mode is API-only, not configurable via TOML. |

## 15. Open Items and Probes

The following items need further investigation or testing to confirm behavior:

| Item | Status | Notes |
|------|--------|-------|
| pproxy SOCKS5 BIND command support | `needs-probe` | pproxy may support BIND (0x02) — not tested |
| pproxy UDP ASSOCIATE as SOCKS5 server | `needs-probe` | pproxy does not appear to support UDP ASSOCIATE as a SOCKS5 server command; uses standalone `-ul` instead |
| pproxy Shadowsocks AEAD key derivation | `needs-probe` | Salt, key derivation function, and key size need confirmation for each cipher |
| pproxy Trojan password hashing | `needs-probe` | Whether pproxy uses SHA224 (standard Trojan) or a variant |
| pproxy HTTP forward proxy (non-CONNECT) | `needs-probe` | Whether pproxy supports plain HTTP forwarding (GET through proxy) vs. only CONNECT tunnels |
| pproxy multi-hop chain behavior | `needs-probe` | Behavior for chains longer than 2 hops (error handling, protocol negotiation) |
| pproxy connection reuse semantics | `needs-probe` | How `--reuse` interacts with chained upstreams and health state |
| pproxy `--rulefile` format details | `needs-probe` | Exact syntax for rule-file entries and regex patterns |
| pproxy SOCKS4a domain resolution | `needs-probe` | Whether pproxy resolves domains at the SOCKS4a server or forwards them |

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
mapping (e.g., `--daemon`, `--ssl`, `-b`, `-ul`, `-ur`) emit a warning or
unsupported feature diagnostic. Unknown flags emit an `unknown-flag` warning.
Translation exits nonzero when unsupported features are present.

### Migration Guidance

For detailed migration instructions, see [`docs/PPROXY_MIGRATION.md`](./PPROXY_MIGRATION.md).

## 17. References

- [pproxy GitHub repository](https://github.com/nimlang/pproxy)
- [RFC 1928 — SOCKS Protocol Version 5](https://datatracker.ietf.org/doc/html/rfc1928)
- [RFC 7231 — HTTP/1.1 Semantics (CONNECT)](https://datatracker.ietf.org/doc/html/rfc7231)
- [Shadowsocks AEAD Ciphers](https://shadowsocks.org/en/spec/AEAD-Ciphers.html)
- [Trojan Protocol](https://trojan-gfw.github.io/trojan/)
- [Eggress Parity Matrix](./PARITY_MATRIX.md)
- [Eggress Differential Tests](../crates/eggress-cli/tests/differential_pproxy.rs)
- [pproxy Migration Guide](./PPROXY_MIGRATION.md)
