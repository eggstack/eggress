# Security Review

## Threat Model

Eggress is a multi-protocol proxy server that accepts client connections over TCP and UDP, routes them through upstream proxies or directly to targets, and exposes an admin HTTP interface for observability. The threat model assumes:

- **Trusted**: The eggress binary itself, its compiled configuration, and localhost-only admin access by the operator.
- **Untrusted**: All network clients (TCP/UDP), configuration file contents (TOML), upstream proxy responses, and any data arriving on listener sockets.

Adversaries may include malicious clients on the network, compromised upstream proxies, or crafted configuration files. The goal is to prevent credential leakage, denial of service, amplification attacks, and privilege escalation through the proxy.

## Trusted/Untrusted Inputs

| Input | Trust Level | Notes |
|---|---|---|
| Client TCP connections | Untrusted | Any host that can reach a listener |
| Client UDP datagrams | Untrusted | Any host that can reach the UDP socket |
| TOML configuration files | Operator-controlled | Parsed at startup and reload; malicious config could disable protections |
| Admin HTTP endpoints | Operator-controlled | Bound to `127.0.0.1` by default |
| Environment variables | Operator-controlled | Used for `EGGRESS_CONFIG` path |
| Upstream proxy responses | Untrusted | TCP/UDP data from upstream proxies |

## Reviewed Surfaces

### Credential Handling

**URI Display Redaction** (`eggress-uri/src/lib.rs:91-145`):
- `RedactedUri` replaces credentials with `****:****@` in all display contexts.
- The actual `CredentialSpec { username, password }` is never included in the formatted output.
- Existing unit test (`test_redacted_display`) verifies no credential leakage.

**Log Redaction**:
- The `RedactedUri` wrapper is used whenever proxy chains are logged or displayed.
- No code path formats raw credentials into log messages.

**Admin Endpoint Credential Exposure** (`eggress-admin/src/routes.rs`):
- `/-/status`, `/-/config`, `/-/routes`, `/-/upstreams` expose only metadata: generation, uptime, rule IDs, listener names/binds, health states, protocol names.
- Upstream URIs with credentials are not exposed; only protocol names and health status are shown.
- The `/-/config` endpoint returns summary counts (rule_count, upstream_group_count), not raw configuration.

**HTTP CONNECT Credential Validation** (`eggress-protocol-http/src/connect/client.rs:30-37`):
- `validate_credentials()` rejects any byte `< 0x20` (control chars) or `== 0x7F` (DEL).
- Applied to both username and password before Base64 encoding and transmission.
- Prevents CRLF injection and header injection attacks.

**HTTP CONNECT Server-Side** (`eggress-protocol-http/src/connect/server.rs`):
- `handle_connect()` validates Proxy-Authorization header against configured credentials.
- Returns 407 on auth failure; never logs the attempted credentials.
- Request head size limited to 32KB; header line count limited to 128.

### Network Exposure

**Admin Bind Defaults**:
- Admin server defaults to `127.0.0.1` (loopback only) in configuration.
- No authentication on admin endpoints — relies on loopback binding for access control.
- Operator must explicitly configure non-loopback bind to expose admin externally.

**Non-Loopback Listener Exposure**:
- Listeners are bound to the address specified in config; operators must be aware that `0.0.0.0` or `::` exposes the proxy to the network.
- No built-in firewall or IP-based access control on listeners.

**UDP Amplification Controls** (`eggress-udp/src/security.rs`):
- `validate_target()` rejects:
  - IPv4/IPv6 multicast addresses
  - IPv4 broadcast (`255.255.255.255`)
  - IPv4/IPv6 unspecified addresses (`0.0.0.0`, `::`)
  - Port zero on all address types
- Client address mismatch detection (`ClientAddressMismatch` error) prevents UDP reflection attacks.
- Per-listener and global association limits prevent resource exhaustion.

### Input Validation

**Oversized HTTP Headers/Status Lines** (`eggress-protocol-http/src/connect/client.rs:8-25`):
- `HttpConnectLimits`: `max_status_line: 1024`, `max_headers_bytes: 32768`, `max_header_count: 100`.
- Server-side: `MAX_HEAD_SIZE: 32KB`, `MAX_HEADER_LINES: 128`.
- Returns `HeaderTooLarge` or `TooManyHeaders` errors on overflow.

**Oversized SOCKS Domain Fields**:
- SOCKS5 codec decodes domain names from the wire; no explicit size limit beyond the datagram framing.
- Domain names are validated as valid UTF-8 before use.

**Trojan Password and Server-Name Handling**:
- Trojan protocol uses password as the sole authentication token.
- Server-name is used as TLS SNI; no injection risk as it is a parsed hostname.
- Passwords are never logged or displayed.

**Route Expression Complexity** (`eggress-config/src/lib.rs`):
- Regex patterns in routing rules are validated at config load time.
- Invalid regex is rejected with a clear error message.
- Recursive matchers (`all`, `any_of`, `not`) are supported but no depth limit is enforced.

**Configuration Validation** (`eggress-config/src/validate.rs`):
- Duplicate listener names, upstream IDs, and group IDs are rejected.
- Unknown group references in rules are rejected.
- Unknown member references in groups are rejected.
- Invalid CIDR notation and regex patterns are rejected.
- Invalid duration strings are rejected.
- UDP config without SOCKS5 is rejected.
- UDP upstream capability is validated (only SOCKS5 upstreams allowed for UDP).

### Protocol Safety

**TLS Insecure Mode** (`eggress-transport-tls/src/client.rs:49-51`):
- `with_insecure()` creates an `InsecureVerifier` that accepts any certificate.
- Documented as "for testing only — never use in production."
- Not exposed in configuration; only available via programmatic API.

**Unsupported Protocol/Transport Combinations** (`eggress-core/src/capability.rs`):
- `classify_upstream_chain()` explicitly reports `UnsupportedProtocol` or `UnsupportedChain` for:
  - Multi-protocol hops
  - Multi-hop chains for UDP
  - HTTP and SOCKS4 for UDP
- Config validation rejects UDP listeners when no UDP-capable upstreams exist.

**Shadowsocks TCP Security Properties**:
- Full AEAD stream encryption (not just header encryption); each direction is independently encrypted.
- Nonces are per-direction counters starting at 1, preventing nonce reuse.
- Per-connection subkeys derived via HKDF-SHA256 from the shared secret and a random salt.
- Only AEAD methods are supported (`aes-128-gcm`, `aes-256-gcm`, `chacha20-ietf-poly1305`); legacy stream ciphers are rejected.
- Password is never logged; URI display uses the redacted `****:****@` format.

**Shadowsocks UDP Security Properties** (`eggress-protocol-shadowsocks/src/udp.rs`):
- Standard AEAD UDP format: `salt + encrypted(address + payload)` per datagram.
- Per-connection subkeys derived via HKDF-SHA256 from the shared secret and a random salt (same derivation as TCP).
- Each datagram uses a fresh random salt; no nonce reuse across datagrams.
- Only AEAD methods are supported (same set as TCP); legacy stream ciphers are rejected.
- Payload length is authenticated via AEAD tag, preventing truncation or extension attacks.
- Client address is authenticated inside the encrypted envelope, preventing address spoofing.

**Protocol Detection Ordering** (`eggress-core/src/detect.rs`):
- `ProtocolDetector` trait with ordered detection.
- `NeedMore` result prevents premature protocol selection.
- No fallback to a default protocol if detection fails; connections are rejected.

### Resource Management

**Task Leaks on Malformed Clients**:
- Each accepted connection spawns a task; malformed connections that fail early should complete the task.
- No explicit per-connection timeout for the initial protocol detection phase (relies on TCP keepalive).
- Admin server spawns per-connection tasks; connection errors are logged at debug level.

**Metrics Label Cardinality** (`eggress-metrics`):
- Metrics use fixed-label sets derived from config names and protocol IDs.
- No user-controlled data enters metric labels directly.
- Route rule IDs are validated as non-empty strings.
- Upstream failure metrics use bounded reason labels (`dns`,
  `connection_refused`, `timeout`, `handshake`, `auth_failed`, `io`, etc.)
  derived from structured error variants, never raw error strings or
  client/target addresses.

**Connection Limits**:
- Per-listener `connection_limit` field available in config (not enforced by default).
- UDP association limits (`max_associations`, `max_targets_per_association`) are enforced.
- No global connection limit across all listeners.

## Mitigations Already Implemented

1. **Credential redaction**: `RedactedUri` replaces credentials with `****:****@` in all display contexts.
2. **HTTP header injection prevention**: `validate_credentials()` rejects control characters in usernames and passwords.
3. **UDP amplification prevention**: `validate_target()` rejects broadcast, multicast, and unspecified addresses.
4. **UDP client pinning**: Prevents address spoofing for UDP association ownership.
5. **Input size limits**: HTTP request/response heads are bounded (32KB, 128 headers).
6. **Config validation**: Duplicate names, invalid references, and incompatible protocol/transport combos are rejected at load time.
7. **Capability classification**: Explicit `UnsupportedProtocol`/`UnsupportedChain` results prevent silent fallback to unsupported modes.
8. **TLS certificate verification**: System root store by default; insecure mode is API-only with documentation warnings.
9. **Admin loopback default**: Admin server binds to `127.0.0.1` unless explicitly configured otherwise.
10. **No `unsafe` code**: Workspace-wide `unsafe_code = "forbid"` prevents memory safety issues.
11. **No OpenSSL dependency**: Uses `rustls` with `ring` crypto provider, eliminating C FFI attack surface.
12. **Atomic config reload**: `ArcSwap<Router>` for lock-free reads; only hot-reloadable fields are swapped.

## Residual Risks

1. **No admin authentication**: Admin endpoints have no auth; access control relies entirely on network-level loopback binding. If operator binds admin to `0.0.0.0`, it is exposed without auth.
2. **No per-connection timeout for protocol detection**: A client that connects but sends no data will hold a connection indefinitely (until TCP keepalive or OS timeout).
3. **No global connection limit**: Only per-listener limits are configurable; no cross-listener cap.
4. **Route expression DoS**: Complex regex or deeply nested matchers could cause high CPU usage during evaluation. No regex timeout is enforced.
5. **UDP datagram size not validated on receive**: `max_datagram_size` is enforced on send, but malformed oversized datagrams from clients may be partially processed before rejection.
6. **No rate limiting**: No request rate limiting on any protocol or admin endpoint.
7. **Logging level sensitivity**: At `debug` level, connection metadata is logged; operators should be cautious about log retention in sensitive environments.
8. **No credential rotation**: Credentials are static in config; no support for dynamic credential rotation without restart/reload.

## Deferred Items

1. **mTLS for admin server**: Mutual TLS authentication for admin endpoints when exposed beyond loopback.
2. **Per-connection timeout for protocol detection**: Configurable idle timeout before protocol detection completes.
3. **Global connection limit**: Cross-listener connection cap.
4. **Regex evaluation timeout**: Bounded CPU time for regex-based routing rules.
5. **Admin endpoint rate limiting**: Request throttling for admin HTTP API.
6. **Dynamic credential sources**: Integration with external secret managers (Vault, AWS Secrets Manager).
7. **Connection-level metrics with user-controlled labels**: Ensure no label injection in future metric expansions.

## Release Blockers

No high-severity findings that block release. The implemented mitigations address the primary attack surfaces for a proxy server at this stage. Residual risks are acknowledged and appropriate for the current release scope (single-operator, controlled-network deployments).
