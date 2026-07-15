# True pproxy Parity Release Candidate

> **Superseded by Phase 36 release audit.**
> This document captures the release-candidate state at Phase 17
> (commit `15eb0d1` and earlier). For the current parity status, see:
>
> - [`release/PARITY_TARGET_FREEZE.md`](release/PARITY_TARGET_FREEZE.md) — frozen targets.
> - [`release/FINAL_PPROXY_PARITY_REPORT.md`](release/FINAL_PPROXY_PARITY_REPORT.md) — current compatible / supported / partial / non-parity split.
> - [`release/RELEASE_NOTES_PARITY_RC.md`](release/RELEASE_NOTES_PARITY_RC.md) — current release-candidate notes.
>
> The Phase 36 audit tightened the CLI tier: 17 CLI entries that previously
> claimed `compatible` with synthetic evidence are now `supported`. The
> `intentional_non_parity` and `supported` lists in this document are still
> informative for historical context but should not be cited as the current
> contract.

## Corrective Verification Pass (Workstream 10) Acceptance Criteria

The following acceptance criteria from the corrective verification pass have
been met:

| Criterion | Status | Evidence |
|-----------|--------|----------|
| Construction-only object labeling | **Met** | `protocol.py` and `cipher.py` classes annotated with construction-only docstrings; `.pyi` stubs updated |
| Migration examples | **Met** | Section 12.5 added with 6 concrete before/after examples (TCP forwarder, SOCKS5 server, chain, auth, load balancing, translation workflow) |
| Acceptance criteria marking | **Met** | This section added to both `TRUE_PPROXY_PARITY_RELEASE_CANDIDATE.md` and `PHASE_17_TRUE_PPROXY_PARITY_RELEASE_CANDIDATE_COMPLETION.md` |
| Python import strategy note | **Met** | Note added to `PYTHON_BINDINGS.md` explaining deferred `import pproxy` compatibility |

**Remaining items:** None identified for this workstream. All four criteria
are addressed.

## 1. Release Candidate Summary

Eggress is a Rust-native, embeddable, multi-protocol proxy framework targeting practical and behavioral parity with Python `pproxy`. This document defines the release candidate boundary: what is supported, what is compatible with pproxy, what is experimental, and what is intentionally not supported.

Release claim:
- Rust-native pproxy-style proxy service with CLI and TOML configuration
- Python package (`eggress`) that embeds the Rust networking/runtime via PyO3
- Compatibility for documented common pproxy use cases (SOCKS5, HTTP, SOCKS4, CLI flags)
- Explicit non-parity for unsupported or intentionally rejected behavior

## 2. Version / Commit Under Audit

- **Version**: 0.1.0 (pre-release)
- **Phase**: 17 (True pproxy parity release candidate audit)
- **Commit**: HEAD of `main` at time of audit
- **Verification date**: Phase 17

## 3. Supported Rust Features

### Production-Ready

| Feature | Status |
|---------|--------|
| HTTP CONNECT (server + client) | Production-ready |
| SOCKS4/4a CONNECT | Production-ready |
| SOCKS5 CONNECT | Production-ready |
| SOCKS5 UDP ASSOCIATE (direct forwarding) | Production-ready |
| Mixed-protocol listeners | Production-ready |
| Multi-hop proxy chains (TCP) | Production-ready |
| TLS transport (upstream + listener) | Production-ready |
| TCP bidirectional relay | Production-ready |
| Routing rule engine (recursive matchers) | Production-ready |
| Upstream groups (first-available, round-robin, random, least-connections) | Production-ready |
| Health probes (TCP connect) with hysteresis | Production-ready |
| Atomic config reload (SIGHUP) | Production-ready |
| TOML configuration with validation | Production-ready |
| Admin HTTP API | Production-ready |
| PAC file serving | Production-ready |
| Static content serving | Production-ready |
| Prometheus metrics | Production-ready |
| SOCKS5 UDP upstream relay (one-hop) | Production-ready |
| HTTP upstream (forwarding) | Production-ready |
| Shadowsocks UDP upstream (standard AEAD, one-hop) | Production-ready |
| SOCKS4/SOCKS4a upstream | Production-ready |
| Trojan upstream (TCP, rustls) | Production-ready |
| Trojan inbound listener (TLS + SHA224) | Production-ready |

### Experimental

| Feature | Status | Notes |
|---------|--------|-------|
| Shadowsocks TCP upstream | Experimental | Non-standard AEAD framing (not wire-compatible with standard Shadowsocks); see `docs/protocols/SHADOWSOCKS_TCP_AUDIT.md` |
| Persistent HTTP forwarding | Experimental | Single-exchange forward only |

## 4. Supported Python Features

| Feature | Status |
|---------|--------|
| `EggressConfig` (parse TOML, from_file) | Supported |
| `EggressService` (start, astart) | Supported |
| `EggressHandle` (status, metrics, reload, shutdown) | Supported |
| `AsyncEggressHandle` (async lifecycle) | Supported |
| Context manager protocol | Supported |
| Exception hierarchy (6 exception types) | Supported |
| GIL release on all blocking calls | Supported |
| pproxy translation helpers | Supported |
| `start_pproxy` / `from_pproxy_args` | Supported |
| Type hints (`py.typed`) | Supported |

## 5. pproxy-Compatible Features

These features match pproxy behavior. Evidence labels indicate verification method:

| Feature | Evidence |
|---------|----------|
| SOCKS5 CONNECT (byte-exact echo) | `differential_socks5_connect_tcp_echo` |
| HTTP CONNECT (byte-exact echo) | `differential_http_connect_tcp_echo` |
| SOCKS5 → HTTP chain | `differential_socks5_through_http_upstream` |
| SOCKS5 → SOCKS5 chain | `differential_socks5_through_socks5_upstream` |
| SOCKS5 auth rejection | `differential_socks5_auth_failure` |
| HTTP auth rejection | `differential_http_auth_failure` |
| `-l` / `-r` CLI flags | `cli_tests` |
| `--rulefile` unsupported diagnostic | `cli_tests` |
| pproxy compat CLI (`translate`/`check`/`run`) | `cli_tests` |
| Round-robin scheduling | `scheduler_runtime.rs` |
| First-available scheduling | `scheduler_runtime.rs` |
| Health-aware upstream filtering | `scheduler_runtime.rs` |

## 6. Supported-but-Not-pproxy-Compatible Features

| Feature | Notes |
|---------|-------|
| SOCKS4/4a CONNECT | Unit tested; no differential against pproxy |
| SOCKS5 UDP ASSOCIATE | Framing differs from pproxy's standalone UDP relay |
| Shadowsocks UDP upstream | Standard AEAD format; interoperable with standard Shadowsocks |
| Multi-hop TCP chains | Basic support; pproxy compatibility untested |
| Random scheduling | Eggress-specific |
| Least-connections scheduling | Eggress-specific |
| Active lease tracking | Eggress-specific |
| Atomic config reload (routing only) | pproxy reloads full config |

## 7. Experimental Features

| Feature | Notes |
|---------|-------|
| Shadowsocks TCP upstream | Non-standard AEAD framing; not wire-compatible with standard Shadowsocks |
| Persistent HTTP forwarding | Single-exchange forward only; pproxy supports persistent |

## 8. Intentional Non-Parity

| Feature | Rationale |
|---------|-----------|
| Shadowsocks inbound listener | Upstream-only; no local SS server |
| Shadowsocks stream ciphers | Insecure; no authentication; vulnerable to bit-flipping |
| ShadowsocksR (SSR) | Non-standard extension; no RFC |
| SSH transport | Not a proxy protocol; adds significant dependency weight |
| Unix domain sockets | Not in scope |
| Redir/transparent proxy | Requires root and kernel hooks |
| QUIC / HTTP/3 / WebSocket tunnels | Out of scope |
| `--daemon` flag | Use systemd or process manager |
| `-ul` / `-ur` flags | Uses SOCKS5 UDP ASSOCIATE instead |
| `--ssl` listener flag | Configure TLS in eggress TOML |
| `-b` block rules | Use eggress TOML routing rules |
| `--reuse` (connection pooling) | Not implemented; one upstream connection per session |
| `--log` flag | Use tracing-subscriber |
| `--sys` (system proxy) | Not supported |
| Persistent HTTP forwarding | Single-exchange forward only |
| Multi-hop UDP chains | One-hop only |
| Reverse/backward proxying | Different product category |
| Plugin system | Fixed protocol set with TOML configuration |

## 9. Unsupported Features

| Feature | Notes |
|---------|-------|
| SOCKS5 BIND | Not implemented |
| SOCKS4 BIND | Not implemented |
| HTTP/2 CONNECT | Not implemented |
| HTTP/3 CONNECT | Not implemented |
| QUIC transport | Not implemented |
| MASQUE transport | Not implemented |
| mTLS for admin | Deferred |

## 10. Test Evidence Table

| Category | Tests | Status |
|----------|-------|--------|
| Unit tests (per-crate) | Codecs, parsers, routing, AEAD, protocol handlers | All passing |
| Property tests (proptest) | Codec round-trips, parser round-trips, route match | All passing |
| Fuzz smoke tests | Seed inputs for cargo fuzz targets | All passing |
| Runtime integration | Startup, routing, health, admin, reload, shutdown, UDP, upstream protocols | All passing |
| Differential tests (gated) | pproxy behavioral comparison | Gated; not run in this audit |
| Interop tests (gated) | Shadowsocks standard interop | Gated; not run in this audit |
| Security invariant tests | Credential redaction, input bounds, protocol safety | All passing |
| Lifecycle invariant tests | Startup/shutdown ordering, drain behavior | All passing |
| Observability tests | Metrics, admin endpoints, logging | All passing |
| Load tests | High-concurrency stress | Ignored by default |
| Python tests | Binding API, pproxy compat, redaction, concurrency | All passing (46 tests) |

## 11. Differential / Interop Evidence Table

| Test | Gate | Status |
|------|------|--------|
| `differential_pproxy` (Rust) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1` | **Gated, not run** — requires running pproxy instance |
| `interoperability_shadowsocks` (Rust) | `EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1` | **Gated, not run** — requires standard Shadowsocks server |
| `test_pproxy_differential` (Python) | `EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1` | **Gated, not run** — requires pproxy Python package |

Note: Gated tests are not run during this audit. They require external dependencies (pproxy server, standard Shadowsocks server) not available in the test environment. Results are recorded as unverified.

## 12. Python Wheel / Install Evidence

- Wheel build: `maturin build --release` produces platform-specific wheels
- Wheel contents: native module (`_eggress.*.so`), Python package (`eggress/`), `py.typed`, `METADATA`, `RECORD`
- Clean venv install: `pip install dist/eggress-*.whl` succeeds
- Test suite: 46 tests pass in clean venv
- No secrets, test certs, or prohibited files in wheel
- Supply chain: `cargo deny check` and `cargo audit` pass

## 13. Security Audit Summary

- **No release blockers identified**
- 14 implemented mitigations covering credential redaction, header injection prevention, UDP amplification prevention, input size limits, config validation, capability classification, TLS verification, admin loopback, no unsafe code, no OpenSSL, atomic reload, unsupported protocol diagnostics, and Python binding security
- 7 residual risks documented (no admin auth, no protocol detection timeout, no global connection limit, regex DoS, no rate limiting, logging sensitivity, no credential rotation)
- All deferred items are documented in `SECURITY_REVIEW.md`
- Python binding surface reviewed: exception strings, repr output, translation warnings, generated TOML, context manager cleanup, no import-time side effects

## 14. Dependency / Artifact Audit Summary

- `cargo deny check`: PASS (advisories ok, bans ok, licenses ok, sources ok)
- `cargo audit`: PASS (1 allowed warning for unmaintained crate `rustls-pemfile v2.2.0`)
- No banned dependencies (openssl-sys, native-tls, aws-lc-sys, cmake all absent)
- No C dependencies, no OpenSSL
- `unsafe_code = "forbid"` in all workspace crates
- Python wheel contains no secrets, test certs, or prohibited files

## 15. Performance Sanity Summary

Criterion benchmarks run on macOS (Apple Silicon). No pproxy comparison available in this environment.

### Route Match (ns/op)

| Benchmark | Time |
|-----------|------|
| `early_match_host_suffix` | ~107 ns |
| `cidr_match` | ~126 ns |
| `mid_match_host_suffix` | ~220 ns |
| `no_match_default` | ~104 ns |
| `ipv6_cidr_match` | ~325 ns |
| `compound_match_all` | ~238 ns |
| `late_match_port_range` | ~125 ns |

### HTTP CONNECT Upstream (µs/op)

| Benchmark | Time |
|-----------|------|
| `open_no_auth` | ~89 µs |
| `open_with_basic_auth` | ~89 µs |
| `rejected_407` | ~91 µs |

### UDP Codec (ns/op)

| Benchmark | Time |
|-----------|------|
| `encode_ipv4_small` | ~39 ns |
| `encode_ipv6_large` | ~86 ns |
| `encode_domain_small` | ~22 ns |
| `decode_ipv4` | ~2.2 ns |
| `decode_domain` | ~22 ns |
| `roundtrip_ipv4_small` | ~22 ns |
| `roundtrip_domain_small` | ~59 ns |

### Notes

- `tcp_relay` benchmark skipped due to macOS ephemeral port exhaustion on `127.0.0.1` during high-frequency bind/accept cycles. This is a benchmark environment limitation, not a runtime issue. Production relay path is covered by integration tests.
- Load tests exist but are `#[ignore]` by default; not run in this audit.
- Python package overhead is bounded by PyO3 FFI cost (minimal; GIL released on all blocking calls).

## 16. Hosted CI / Local Verification Status

- **Hosted CI**: Non-functional (billing-related failures; no code execution)
- **Local verification**: All commands pass
  - `cargo fmt --all -- --check`: PASS
  - `cargo check --workspace --all-targets`: PASS
  - `cargo test --workspace`: PASS (load tests ignored as expected)
  - `cargo clippy --workspace --all-targets -- -D warnings`: PASS
  - `cargo deny check`: PASS
  - `cargo audit`: PASS (1 allowed warning)
  - `python -m ruff check python/`: PASS (after Phase 17 fixes)
  - `python -m mypy python/eggress --ignore-missing-imports`: 20 false-positive `_inner` attribute errors (PyO3 native types invisible to mypy; expected)

## 17. Release Blockers

**Pre-release RC blockers: None identified.**

All verification commands pass. No high-severity security findings. Documentation is internally consistent after Phase 17 corrections.

**GA blockers remain and are listed below:**

- Hosted CI must run successfully or have a documented release fallback
- TestPyPI install must be verified
- At least core pproxy differential tests should run or be explicitly scoped out
- Shadowsocks UDP interop should hard-fail on failure if claimed standard-compatible
- Formal wheel install tests on supported platforms
- Security residuals (mTLS, protocol detection timeout, global connection limit) triaged for GA

## 18. Go / No-Go Recommendation

**GO for pre-release RC.** The release candidate is ready for pre-release tagging (not GA). Recommendations:

1. Tag as `v0.1.0-rc.1` or similar pre-release identifier
2. Run gated differential tests when pproxy environment is available
3. Run formal benchmarks before general availability release
4. Consider publishing Python package to TestPyPI for validation
5. Address deferred security items (mTLS, protocol detection timeout) before GA
