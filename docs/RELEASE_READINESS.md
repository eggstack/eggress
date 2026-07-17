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

### Track B/C Python closure

The current Python compatibility claim is a certified subset, not strict full
pproxy parity. The canonical `eggress` wheel remains namespace-clean; the
separate `eggress-pproxy-compat` wheel provides the opt-in top-level `pproxy`
package and depends on the matching engine plus the declared `cryptography`
range. Python outbound TCP uses the native Rust connector and does not create
a temporary local listener. Validate both installed wheels in a clean
environment and generate the redacted, commit-bound evidence bundle with
`scripts/release_evidence.py` before tagging.

The Track B/C verification pass (2026-07-16) additionally:

- Hardened `scripts/release_evidence.py` to fail closed on dirty worktrees, missing inputs, SHA mismatches, and tracked-input drift via `--require-clean`, `--expected-commit`, and `--verify-tracked-inputs`.
- Added AEAD known-answer tests using NIST SP 800-38D (AES-256-GCM, AES-128-GCM) and a documented skip for ChaCha20-Poly1305 (RFC 8439 AAD not exposed by the Python API).
- Added 40 native outbound stream lifecycle and resource tests proving that `OutboundConnector.connect_tcp()` does not start a temporary local listener, handles cancellation cleanly, and passes loop-affinity / GIL-release / repeated-cycle stress.
- Added 12 in-tree fuzz-smoke tests across `eggress-{protocol-http,protocol-trojan,protocol-websocket,protocol-shadowsocks,config}` plus reverse-handshake coverage in the existing reverse test suite.
- Fixed two cipher regressions: `AEADCipher.setup_iv` now keeps the nonce in sync with the IV; `AEADCipher.__copy__` no longer resets the nonce by re-running `__init__`.
- Aligned the compatibility wheel's Python version classifiers with the canonical wheel (`3.9`, `3.10`, `3.11`, `3.12`, `3.13`, plus `3 :: Only`).

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

### Current certification artifacts

| Artifact | Status |
|----------|--------|
| Track B/C certification report | [`release/FINAL_PPROXY_PARITY_CERTIFICATION_TRACK_BC.md`](release/FINAL_PPROXY_PARITY_CERTIFICATION_TRACK_BC.md) |
| Machine-validated parity report | [`parity/PPROXY_PARITY_REPORT.md`](parity/PPROXY_PARITY_REPORT.md) |
| Release evidence generator | [`../scripts/release_evidence.py`](../scripts/release_evidence.py) |

### Experimental / Limited

| Feature | Status | Notes |
|---------|--------|-------|
| Shadowsocks TCP | Supported | AEAD methods only; standard SIP003 wire-compatible AEAD framing; single-hop upstream |
| Shadowsocks UDP | Supported | Standard AEAD format; single-hop upstream only |
| Trojan TCP | Supported | Inbound listener with TLS + SHA224 password verification; upstream client with rustls |
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
| Python 3.9–3.13 | n/a | Supported | Required by both wheels; `cryptography>=42,<47` for cipher support |
| musllinux x86_64 | `musllinux` | Not built | May be added later |
| musllinux aarch64 | `musllinux` | Not built | May be added later |
| Windows arm64 | `win-arm64` | Not built | May be added later |

Unsupported platforms will receive a clear error at install time if no compatible wheel is available. Building from source requires a Rust toolchain.

## Operational Certification (2026-07-17)

The Track B/C operational certification has been completed successfully:

| Metric | Result |
|--------|--------|
| Rust tests (12 key suites) | 686 passed, 0 failed |
| Python tests (full suite) | 1,763 passed, 0 failed, 127 skipped |
| pproxy drop-in API | 46/46 passed |
| pproxy compat | 12/12 passed |
| Composition matrix | 33/33 passed |
| Clean wheel install | Verified (eggress + eggress-pproxy-compat) |
| Source-tree isolation | No source paths in clean venv |
| Fuzz smoke tests | 12/12 passed |
| Security invariants | 8/8 passed |
| Lifecycle invariants | 11/11 passed |
| 148-capability audit | Complete, no demotions required |

**GO decision issued.** All mandatory lanes pass. The release candidate is ready for tagging.

## Release Checklist

- [x] `cargo fmt --all -- --check` passes
- [x] `cargo check --workspace` passes
- [x] `cargo test --workspace` passes
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [x] `cargo deny check` passes
- [x] [SECURITY_REVIEW.md](SECURITY_REVIEW.md) reviewed, no blockers
- [x] [PARITY_MATRIX.md](PARITY_MATRIX.md) up to date
- [x] [CONFIG_REFERENCE.md](CONFIG_REFERENCE.md) reflects current schema
- [x] [CI_STATUS.md](CI_STATUS.md) reflects current CI state
- [x] Known limitations documented and acceptable for target deployment
- [x] Changelog updated (if applicable)
- [x] `scripts/release_evidence.py --require-clean --expected-commit HEAD --verify-tracked-inputs --output target/release-evidence --reference pproxy==2.7.9` passes
- [x] `python3.11 -m pytest python/tests/test_outbound_stream_verification.py -v` passes
- [x] `arch -arm64 python3.11 -m pytest python/tests/test_protocol_cipher.py::TestAEADKnownAnswerVectors -v` passes
- [x] `cargo check --manifest-path fuzz/Cargo.toml --bins` passes
- [x] Both wheels' Python classifiers align (3.9–3.13)
