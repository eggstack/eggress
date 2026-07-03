# Release Readiness

## CI / Local Verification Status

- **Hosted CI**: `.github/workflows/ci.yml` exists but recent runs report `completed failure` with billing-related annotations (no code execution). Hosted CI is **not** a reliable signal at this time.
- **Source of truth**: Local verification. Run the full local check suite before any release tag.
- See [CI_STATUS.md](CI_STATUS.md) for detailed status and how to interpret completion docs.

### Required Local Verification

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
```

All five commands must pass cleanly before tagging a release.

## Security Review Status

- [SECURITY_REVIEW.md](SECURITY_REVIEW.md) exists and covers all reviewed surfaces.
- **No release blockers identified.** The security review documents 14 implemented mitigations and 7 residual risks, all appropriate for the current release scope (single-operator, controlled-network deployments).
- Key mitigations: credential redaction, HTTP header injection prevention, UDP amplification prevention, client pinning, input size limits, config validation, capability classification, TLS certificate verification, admin loopback default, no unsafe code, no OpenSSL, atomic reload, unsupported protocol diagnostics, Python binding security.
- See [SECURITY_REVIEW.md](SECURITY_REVIEW.md) Deferred Items for planned security enhancements.

## Parity Matrix Status

- [PARITY_MATRIX.md](PARITY_MATRIX.md) exists and tracks feature parity with Python `pproxy`.
- See the matrix for per-protocol, per-feature completion status.

## Supported vs Experimental

### Fully Supported

| Feature | Status |
|---------|--------|
| HTTP CONNECT (server + client) | Production-ready |
| SOCKS4/4a CONNECT | Production-ready |
| SOCKS5 CONNECT | Production-ready |
| SOCKS5 UDP ASSOCIATE (direct forwarding) | Production-ready |
| Mixed-protocol listeners | Production-ready |
| Multi-hop proxy chains | Production-ready |
| TLS transport (upstream + listener) | Production-ready |
| TCP bidirectional relay | Production-ready |
| Routing rule engine (recursive matchers) | Production-ready |
| Upstream groups (first-available, round-robin, random, least-connections) | Production-ready |
| Health probes (TCP connect) | Production-ready |
| Atomic config reload (SIGHUP) | Production-ready |
| TOML configuration with validation | Production-ready |
| Admin HTTP API | Production-ready |
| PAC file serving | Production-ready |
| Static content serving | Production-ready |
| Prometheus metrics | Production-ready |
| SOCKS5 UDP upstream relay (one-hop) | Production-ready |
| HTTP upstream (forwarding) | Production-ready |

### Experimental / Limited

| Feature | Status | Notes |
|---------|--------|-------|
| Shadowsocks TCP | Supported | AEAD methods only; standard SIP003 wire-compatible AEAD framing; single-hop upstream |
| Shadowsocks UDP | Supported | Standard AEAD format; single-hop upstream only |
| Trojan TCP | Experimental | Foundation only |
| Trojan UDP | Not supported | Validation rejects config with UDP listener + Trojan upstream |

### Not Implemented

| Feature | Notes |
|---------|-------|
| QUIC transport | Not planned for current scope |
| MASQUE transport | Not planned for current scope |
| mTLS for admin server | Deferred (see SECURITY_REVIEW.md) |

> Unix-domain socket listeners and persistent HTTP forwarding are now
> implemented (see manifest entries `unix_domain_sockets` and
> `http_forward_persistent_connection`). The previous "Listed in roadmap"
> rows are removed as of Phase 36.

## Known Limitations

1. **No admin authentication**: Admin endpoints have no auth; access control relies on loopback binding.
2. **No global connection limit**: Only per-listener limits are configurable.
3. **No rate limiting**: No request rate limiting on any protocol or admin endpoint.
4. **No dynamic credential rotation**: Credentials are static in config.
5. **No regex evaluation timeout**: Complex regex rules could cause high CPU usage.
6. **No per-connection timeout for protocol detection**: Silent clients hold connections indefinitely.

## Dependency Policy Enforcement

- `cargo deny check` runs in CI and locally.
- Workspace `Cargo.toml` bans `unsafe` code (`unsafe_code = "forbid"`).
- No C dependencies, no OpenSSL. Uses `rustls` with `ring` crypto provider.
- See [DEPENDENCY_POLICY.md](DEPENDENCY_POLICY.md) for the full policy.

## Platform Support

| Platform | Wheel Target | Status | Notes |
|----------|-------------|--------|-------|
| Linux x86_64 | `manylinux` | Supported | Primary CI target |
| Linux aarch64 | `manylinux` | Supported | Cross-compiled or native CI |
| macOS arm64 | `macos-arm64` | Supported | Apple Silicon |
| macOS x86_64 | `macos-x86_64` | Supported | Intel Macs |
| Windows x86_64 | `win-amd64` | Supported | MSVC toolchain |
| musllinux x86_64 | `musllinux` | Not built | May be added later |
| musllinux aarch64 | `musllinux` | Not built | May be added later |
| Windows arm64 | `win-arm64` | Not built | May be added later |

Unsupported platforms will receive a clear error at install time if no compatible wheel is available. Building from source requires a Rust toolchain.

## Release Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo check --workspace` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo deny check` passes
- [ ] [SECURITY_REVIEW.md](SECURITY_REVIEW.md) reviewed, no blockers
- [ ] [PARITY_MATRIX.md](PARITY_MATRIX.md) up to date
- [ ] [CONFIG_REFERENCE.md](CONFIG_REFERENCE.md) reflects current schema
- [ ] [CI_STATUS.md](CI_STATUS.md) reflects current CI state
- [ ] Known limitations documented and acceptable for target deployment
- [ ] Changelog updated (if applicable)
