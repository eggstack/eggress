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
- Recursive matchers (`all`, `any_of`, `not`) are supported with a maximum
  depth of 10 and a maximum of 100 matcher nodes per expression.

**pproxy Compat Regex Validation** (`eggress-pproxy-compat/src/regex_compat.rs`):
- `-b` and `--rulefile` regex patterns validated at parse time via `CompatRegex`.
- Dual backend: fast `regex` first, `fancy_regex` fallback for lookahead/lookbehind/backreferences.
- Pattern length limit: 4096 characters (`MAX_PATTERN_LEN`).
- Rulefile entry limit: 10,000 (`MAX_RULE_ENTRIES`).
- Fancy regex usage emits `FancyRegexBackend` diagnostic.

**Configuration Validation** (`eggress-config/src/validate.rs`):
- Duplicate listener names, upstream IDs, and group IDs are rejected.
- Unknown group references in rules are rejected.
- Unknown member references in groups are rejected.
- Invalid CIDR notation and regex patterns are rejected.
- Invalid duration strings are rejected.
- UDP config without SOCKS5 is rejected.
- UDP upstream capability is validated (only SOCKS5 upstreams allowed for UDP).

### Protocol Safety

**H2 CONNECT Stream Handling** (`eggress-protocol-http/src/h2_connect.rs`):
- H2 CONNECT accepts only `CONNECT` method; other methods receive `PROTOCOL_ERROR` reset.
- Authority is validated as non-empty; missing authority triggers `H2` error.
- Per-stream relay spawns independent tasks; one stream failure does not affect others.
- Flow control is respected via `h2::SendStream::reserve_capacity` and `poll_capacity`.
- GOAWAY and RST_STREAM are handled by the `h2` crate's connection driver.

**WebSocket Tunnel Handling** (`eggress-protocol-websocket/src/lib.rs`):
- Binary frames only; text frames are logged and skipped (no data processing).
- `max_message_size` (default 16MB) enforced on inbound frames; oversized frames trigger IO error.
- Close frames yield EOF to the reader; no data from close frames is processed.
- Ping/pong frames are silently consumed; no application-level pong response.
- `WebSocketStreamAdapter` implements `AsyncRead`/`AsyncWrite` — no direct buffer exposure.
- WSS uses existing TLS transport layer; no separate TLS handling.

**Raw Tunnel Handling** (`eggress-protocol-raw/src/tunnel.rs`):
- No protocol negotiation or authentication; target is fixed by config at startup.
- Target validation at config compile time (rejects missing target).
- `copy_bidirectional` used for relay; no custom framing or buffer manipulation.
- One connection per accepted TCP stream; no multiplexing.
- **Warning**: Raw tunnels have no authentication or encryption. Target must be trusted. Network-level access control is the operator's responsibility.

**Reverse / Backward Proxy Security** (`eggress-protocol-reverse/`):

- **Plaintext control channel by default**: The reverse protocol uses raw TCP with auth sent as plaintext `user:pass` bytes. The pproxy-compatible wire format (1-byte handshake + raw auth) is intentional and matches upstream behavior.
- **No built-in TLS**: The control channel has no TLS support. Operators must wrap the control connection with stunnel, haproxy, or use a WireGuard tunnel when traversing untrusted networks.
- **Auth bypass risk**: If `auth_username` and `auth_password` are both `None` on a `[[reverse_servers]]` table, any host that can reach the control port can connect. Recommended: always configure `auth_username` and `auth_password` (or `auth_password_env`) when binding to a non-loopback address.
- **Auth replay**: The same `user:pass` bytes are accepted on every reconnect. There is no nonce, challenge-response, or forward secrecy. Operators needing forward secrecy must add TLS over the control channel.
- **Listener bind access control**: There is no built-in allowlist in the current implementation. Operators exposing public listeners must restrict the `control_bind` address at the OS / firewall level. An `allow_bind` policy is planned but not yet implemented.
- **Recommended hardening**:
  - Use TLS over the control channel via stunnel or equivalent.
  - Configure strong `auth_password` (use `auth_password_env` for environment injection rather than plaintext in config).
  - Restrict `control_bind` to loopback or a known VPC interface.
  - Run firewall rules to limit which hosts can reach the control port.
  - Monitor `eggress_reverse_control_connections_rejected_total` for anomalies.
- **Documented limitations**:
  - UDP reverse mode is not supported.
  - Jump chains through reverse are not supported.
  - On-wire version is fixed at "user:pass + 1-byte handshake + bidirectional TCP relay" -- no versioning or upgrade path.

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

**Unsupported Protocol Diagnostics** (`eggress-pproxy-compat/src/translate.rs`):
- Unsupported schemes (SSH) produce `UnsupportedFeature` errors with scheme name. Unix upstream and transparent upstream produce appropriate diagnostics. Unix listener and transparent listener are now supported.
- Shadowsocks listeners are now supported as an explicit protocol mode (no mixed-listener auto-detection).
- Trojan listeners are rejected with clear diagnostic (upstream-only).
- Legacy stream cipher URIs are rejected at parse time.
- All diagnostic messages redact credentials.

**Shadowsocks TCP Security Properties**:
- Full AEAD stream encryption (not just header encryption); each direction is independently encrypted.
- Standard TCP framing: encrypted length prefix + encrypted payload, compatible with standard Shadowsocks implementations.
- Nonces are per-direction counters starting at 1, preventing nonce reuse.
- Per-connection subkeys derived via HKDF-SHA256 from the shared secret and a random salt.
- Only AEAD methods are supported (`aes-128-gcm`, `aes-256-gcm`, `chacha20-ietf-poly1305`); legacy stream cipher URIs produce `LegacyMethodUnsupported` errors with a message suggesting AEAD methods.
- SSR URIs (`ssr://`) produce `SsrUnsupported` errors. The pproxy compat layer produces `UnsupportedFeature` diagnostics for both legacy stream ciphers and SSR URIs.
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

### Python Binding Surface

**Exception Strings** (`eggress-python`):
- Python exceptions wrap Rust error types; no raw Rust error strings leak to Python.
- `repr()` output uses Python class names, not Rust type paths.

**Translation Warnings** (`eggress-pproxy-compat`):
- pproxy translation warnings and unsupported-feature diagnostics redact credentials.
- Generated TOML uses `RedactedUri` for display; actual credentials are in config only.

**Context Manager Cleanup**:
- `EggressHandle.__exit__` calls `shutdown()` which triggers graceful drain.
- `AsyncEggressHandle.__aexit__` calls `shutdown()` asynchronously.
- Drop without explicit shutdown triggers cleanup via Rust `Drop` impl.

**Signal handling (`Server.run()`)**:
- `Server.run()` registers `SIGINT`/`SIGTERM` handlers and blocks on a `threading.Event`. It raises `RuntimeError` if invoked from a non-main thread because Python's `signal.signal()` API requires the main thread.
- Old signal handlers are restored in a `try/finally` block, so a panic during signal handler registration does not leave the process with a wedged handler.
- Context-manager exit (`__exit__`) calls `close()` regardless of whether `run()` was used.

**Concurrent `Server` instances**:
- Each `Server` owns its own `EggressService` (which owns its own Tokio runtime and two OS threads: `eggress-embed-rt` and `eggress-embed-run`). Two `Server` instances do not share runtime state.
- `EggressHandle.shutdown()` is idempotent and safe to call from any thread.
- `EggressHandle` is `Send + Sync` in Rust, so `handle.status()`, `handle.bound_addresses`, `handle.metrics_text()`, and `handle.reload_toml()` are safe to call from multiple Python threads.
- Concurrent `start()` on the same `EggressService` fails with `AlreadyStartedError`.

**GIL release completeness** (`eggress-python` PyO3 bindings):
- All blocking Rust entry points (`EggressConfig.from_toml`, `EggressConfig.from_file`, `EggressService.start`, `EggressHandle.shutdown`, `handle.metrics_text()`, `handle.status()`, `handle.reload_toml()`, `handle.bound_addresses`, `route_explain`, `test_upstream_connect`, `translate_pproxy_*`, `check_pproxy_*`, `redact_pproxy_uri`, `diagnostics_for_uri`, `explain_*`) release the GIL via `py.detach()` before crossing the FFI boundary.
- Verified by `python/tests/test_threading.py` (concurrent shutdown, parallel handle access).
- This means that a Python thread holding the GIL (e.g., during callback invocation) does not block the Rust runtime.

**Open-proxy risk on non-loopback binds**:
- `Server(listen=["socks5://0.0.0.0:1080"])` binds on all interfaces with no authentication. The Python binding does not add a guardrail; the operator is responsible for ensuring the bind address is appropriate.
- This is a documentation/sandbox responsibility, not a runtime check. The Rust supervisor enforces the same `bind` semantics; the embed API surfaces them as-is.

**DoS via repeated start/close cycles**:
- A loop that calls `Server.start()` repeatedly without bound is bounded by `AlreadyStartedError` after the first start.
- `Server.close()` is idempotent and does not block on thread join beyond the `eggress-embed` 5-second best-effort timeout.
- No persistent state survives a `close()`, so repeated cycles do not accumulate listeners or upstreams.

**No Import-Time Side Effects**:
- `import eggress` does not start services, bind ports, or log.
- Native module (`_eggress`) loads on import but performs no I/O.

### Python Packaging

**No secrets in package data**:
- The `eggress` wheel does not contain `.env` files, API tokens, TLS certificates, or private keys.
- Generated config/fixture files in the repository use placeholder credentials (`user:password`, `example.com`).
- `pyproject.toml` declares no runtime dependencies; the only dev dependency is `pytest>=7.0`.

**Wheel builds exclude debug artifacts**:
- Wheels are built with `maturin build --release`. Debug symbols are stripped.
- The `include` directive in `[tool.maturin]` explicitly lists `eggress/**/*.py` and `eggress/py.typed`. No additional files are bundled.
- No build scripts (`build.rs`) or post-install hooks exist in the workspace.

**Generated config/fixture files contain no real credentials**:
- All TOML config strings in tests and examples use `user:password` or `example.com`.
- `check_pproxy_uri()` and `translate_pproxy_uri()` accept user-provided URIs but redact credentials in all output (`RedactedUri`).
- Test fixtures under `tests/compat/fixtures/` use synthetic data.

**Long description does not overclaim compatibility**:
- The package description is "Python bindings for the eggress proxy".
- The README and long description do not claim drop-in replacement status for pproxy.
- Compatibility claims are scoped to specific features with diagnostic-backed boundaries.

**Dependency list is minimal**:
- Runtime dependencies: none (native extension only).
- Dev dependencies: `pytest>=7.0`.
- Build dependencies: `maturin>=1.0,<2.0` (declared in `[build-system]`).
- No transitive dependency risk beyond PyO3 and maturin.

**`pip-audit` recommended as optional check**:
- Run `pip-audit` against installed wheels to verify no known vulnerabilities in dependencies.
- Not a gate for release, but recommended as a periodic supply-chain hygiene check.

**`.gitignore` prevents stale build artifacts from being committed**:
- `/target`, `/dist`, `crates/eggress-python/dist/`, `*.so`, `*.pyc`, `__pycache__/`, `*.egg-info/`, `.venv/` are all gitignored.
- Built wheels and sdist archives are not committed to the repository.

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
13. **Unsupported protocol diagnostics**: pproxy compat layer produces structured `UnsupportedFeature` errors for SSH, Unix (upstream), and other unsupported protocols. No silent fallback to direct or different protocols.
14. **Python binding security**: Exception strings do not leak raw Rust errors; `repr()` uses Python class names; translation warnings redact credentials; no import-time side effects; context manager ensures cleanup.
15. **Transparent proxy privilege separation**: Transparent proxy requires explicit `CAP_NET_ADMIN` or root; the listener validates `SO_ORIGINAL_DST` availability at startup and fails fast if unavailable.
16. **Transparent proxy loop prevention**: Connections destined to the proxy's own listen address are rejected to prevent forwarding loops.
17. **Unix socket permissions**: Socket file permissions are configurable (default `0o660`); operator controls access via filesystem permissions and group membership.
18. **Transparent proxy original destination trust**: The original destination is extracted from kernel socket options (`SO_ORIGINAL_DST`), which is trusted kernel-provided metadata. No spoofing risk from the network.
19. **H2 CONNECT method validation**: Only `CONNECT` method accepted; non-CONNECT requests receive `PROTOCOL_ERROR` reset (Phase 26).
20. **WebSocket frame size limits**: `max_message_size` enforced on inbound binary frames; oversized frames trigger IO error and connection closure (Phase 26).
21. **WebSocket binary-only**: Text frames are logged and skipped; no text frame data is processed (Phase 26).
22. **Raw tunnel fixed target**: Target is fixed by config at startup; no runtime target selection from client data (Phase 26).
23. **Raw tunnel no-auth warning**: Documented that raw tunnels have no authentication or encryption; operator must control network access (Phase 26).
24. **Python packaging security**: No secrets in package data; wheels built with `--release` exclude debug artifacts; generated configs use placeholder credentials; `.gitignore` prevents stale build artifacts from being committed; long description does not overclaim compatibility.
25. **Non-loopback bind warnings**: Config validation emits structured warnings for non-loopback listener, admin, and reverse control binds without authentication (Phase 35).
26. **`load_and_validate_with_warnings()`**: New API returns both compiled config and security warnings, allowing operators to review dangerous configurations before startup (Phase 35).
27. **Security documentation suite**: Dedicated `docs/security/` directory with threat model, hardening guide, open-proxy prevention, reverse security, and redaction policy documents (Phase 35).
28. **Security manifest entries**: Eight security features tracked in `pproxy_manifest.toml` with synthetic evidence and test references (Phase 35).

## Residual Risks

1. **No admin authentication**: Admin endpoints have no auth; access control relies entirely on network-level loopback binding. If operator binds admin to `0.0.0.0`, it is exposed without auth.
2. **No per-connection timeout for protocol detection**: A client that connects but sends no data will hold a connection indefinitely (until TCP keepalive or OS timeout).
3. **No global connection limit**: Only per-listener limits are configurable; no cross-listener cap.
4. **Route expression DoS**: Complex regex or deeply nested matchers could cause high CPU usage during evaluation. No regex timeout is enforced.
5. **No rate limiting**: No request rate limiting on any protocol or admin endpoint.
6. **Logging level sensitivity**: At `debug` level, connection metadata is logged; operators should be cautious about log retention in sensitive environments.
7. **No credential rotation**: Credentials are static in config; no support for dynamic credential rotation without restart/reload.
8. **Transparent proxy privilege scope**: Running with `CAP_NET_ADMIN` grants the process ability to manipulate network configuration; ensure the binary is not writable by untrusted users.
9. **Unix socket file cleanup**: Stale socket files from unclean shutdown require operator-managed cleanup; `unlink_existing` handles the common case but does not cover all race conditions.
10. **macOS PF transparent proxy**: Not implemented. Operators using macOS must use pfctl with a standard TCP listener, which has different trust and configuration characteristics.

## Deferred Items

1. **mTLS for admin server**: Mutual TLS authentication for admin endpoints when exposed beyond loopback.
2. **Per-connection timeout for protocol detection**: Configurable idle timeout before protocol detection completes.
3. **Global connection limit**: Cross-listener connection cap.
4. **Regex evaluation timeout**: Bounded CPU time for regex-based routing rules.
5. **Admin endpoint rate limiting**: Request throttling for admin HTTP API.
6. **Dynamic credential sources**: Integration with external secret managers (Vault, AWS Secrets Manager).
7. **Connection-level metrics with user-controlled labels**: Ensure no label injection in future metric expansions.
8. **Transparent proxy TPROXY support**: Linux TPROXY workflow for transparent UDP proxying.
9. **macOS PF transparent proxy**: Native PF integration for macOS transparent proxy.

## Release Blockers

No high-severity findings that block release. The implemented mitigations address the primary attack surfaces for a proxy server at this stage. Residual risks are acknowledged and appropriate for the current release scope (single-operator, controlled-network deployments).
