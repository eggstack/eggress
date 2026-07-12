# Final pproxy Parity Report (Phase 36)

> **Superseded:** This report was generated from the legacy `tests/compat/pproxy_manifest.toml` evidence index (171 features, old taxonomy). The canonical compatibility contract is now `docs/parity/pproxy_capability_manifest.toml` (139 capabilities, 5-tier vocabulary). The current auto-generated report is `docs/parity/PPROXY_PARITY_REPORT.md`.

**Report status:** Generated from `tests/compat/pproxy_manifest.toml` on 2026-07-03.
**Machine-readable source:** generated with `python3 scripts/phase36_report.py` → `target/compat/final-pproxy-parity-report.json`
**Frozen targets:** [`PARITY_TARGET_FREEZE.md`](PARITY_TARGET_FREEZE.md)

This is the release-readiness parity report for the eggress parity release
candidate. Every claim in this document traces back to an entry in the
manifest. The manifest is the single source of truth; this report aggregates
and explains it.

## Totals

| Metric | Count |
|---|---|
| Total features tracked | **171** |
| Compatible (pproxy parity, differential evidence) | **26** |
| Supported (eggress-native, no parity claim) | **112** |
| Partial (useful subset only) | **8** |
| Intentional non-parity (deliberate divergence) | **19** |
| Unsupported / deferred | **6** |

Categories in scope: `protocol`, `udp`, `routing`, `security`, `cli`, `uri`,
`transport`, `platform`, `system_proxy`, `python`, `python-api`, `packaging`,
`performance`, `inbound_tcp`, `upstream_tcp`.

## Compatible features (26)

These features have pproxy behavioral parity backed by differential or
interoperability evidence. Each entry lists the differential test command.

### Inbound TCP (18)

| Feature | Test command |
|---|---|
| `http_connect_server` | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `http_forward_proxy` | Same as above |
| `socks4_server` | Same as above |
| `socks4a_server` | Same as above |
| `socks5_connect_server` | Same as above |
| `http_connect_auth_success` | Same as above |
| `http_connect_auth_rejection` | Same as above |
| `http_connect_ipv4_target` | Same as above |
| `http_connect_domain_target` | Same as above |
| `http_connect_ipv6_target` | Same as above |
| `http_connect_refused_target` | Same as above |
| `socks4_connect` | Same as above |
| `socks4a_domain_connect` | Same as above |
| `socks5_connect_ipv6` | Same as above |
| `socks5_connect_domain` | Same as above |
| `socks5_refused_target` | Same as above |
| `socks5_auth_rejection` | Same as above |
| `http_auth_rejection` | Same as above |

### Upstream TCP (4)

| Feature | Test command |
|---|---|
| `http_connect_upstream` | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `socks5_upstream` | Same as above |
| `direct_upstream` | Synthetic (covered by all echo relay tests) |
| `single_hop_tcp_chain` | Same as `http_connect_upstream` and `socks5_upstream` |

### CLI / Misc (4)

These CLI behaviors have differential process-lifecycle and golden output
comparisons that were upgraded from synthetic evidence after Phase 28:

| Feature | Test command |
|---|---|
| `cli_run_process_behavior` | `cargo test -p eggress-cli` (process exit code, signal handling, readiness probe parity with `pproxy --help` shutdown sequence) |
| `cli_inventory_complete` | `cargo test -p eggress-cli` (CLI flag inventory parity, see `docs/cli/PPROXY_CLI_INVENTORY.md`) |
| `udp_listen_flag`, `udp_remote_flag` | `cargo test -p eggress-pproxy-compat` (translation test) |

> **Note on the CLI tier shift (Phase 36):** Before Phase 36, 17 CLI entries
> were marked `compatible` with synthetic evidence. They are now `supported`
> with `implemented_synthetic` evidence. The `compatible` tier for CLI now
> applies only to features whose differential test exercises actual pproxy
> wire behavior (process lifecycle, golden translation output).

## Supported but not parity-tested features (112)

These are eggress features that work correctly but where pproxy behavioral
parity has not been demonstrated. They include:

- **Shadowsocks TCP / UDP** (SIP003 AEAD): interop-tested against
  `ssserver`/`sslocal` from `shadowsocks-rust` (`EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored`).
- **SOCKS5 UDP ASSOCIATE**: implemented; framing differs from pproxy.
- **Direct UDP forwarding** (multiple entry points: `-ul`, SOCKS5 ASSOCIATE,
  standalone relay).
- **Python API** (33 features): see `python/tests/test_pproxy_compat.py`.
- **System proxy inspection** (macOS via `networksetup`, Windows via registry,
  Linux via `gsettings`): see `cargo test -p eggress-system-proxy`.
- **Packaging** (wheels, sdist, py.typed): CI matrix in
  `.github/workflows/python-wheels.yml`.
- **Performance / resource-leak smokes** (6 features): see
  `docs/performance/REGRESSION_GATE_POLICY.md`.
- **Security features** (8 features): see `docs/security/`.

## Partial features (8)

| Feature | Subset covered |
|---|---|
| `trojan_upstream`, `trojan_upstream_client` | Trojan client only; no Trojan server (intentional). |
| `socks5_udp_associate_relay`, `socks5_udp_upstream` | One-hop only; pproxy supports multi-hop. |
| `shadowsocks_udp_upstream` | Single-hop only; pproxy supports multi-hop. |
| `multi_hop_tcp_chain` | 3+ hops exist but pproxy parity not tested. |
| `udp_chain` | No multi-hop UDP chains. |
| `chain_capability_validation` | Capability validation exists but pproxy does not have the same concept. |
| `hot_reload_routing` | Routing + upstreams only; pproxy reloads full config. |
| `toml_config` | Different schema than pproxy; not a 1:1 mapping. |

## Intentional non-parity (19)

These features are deliberately different from pproxy. Each has a documented
rationale in the manifest's `divergence` field.

| Feature | Rationale summary |
|---|---|
| `daemon_flag`, `rulefile_flag`, `ssl_flag`, `block_flag`, `reuse_flag`, `log_flag` | pproxy CLI flags replaced by eggress-native config / process management. |
| `cli_translate_ssr_rejection`, `cli_translate_ssh_rejection` | SSR (legacy Shadowsocks) and SSH intentionally rejected. |
| `scheduler_state_persistence` | Eggress preserves cursor; pproxy resets on reload. |
| `shadowsocks_stream_ciphers`, `shadowsocks_r` | Stream ciphers are insecure; rejected. |
| `socks_bind_deferred` | BIND command not implemented. |
| `macos_pf_transparent_proxy` | macOS PF not implemented; use `pfctl` with a standard listener. |
| `quic_h3_transport` | Deferred by ADR; pproxy QUIC is experimental. |
| `backward_no_udp` | pproxy itself doesn't support UDP over backward channel. |
| `python_api_config_reload`, `python_api_error_hierarchy`, `python_api_context_manager`, `python_api_gil_release` | Eggress-only Python surface. |

## Unsupported / deferred (6)

| Feature | Reason |
|---|---|
| `backward_parallel_connections` | Architecture supports it; not wired. |
| `backward_jump_chain` | Would need chain executor integration. |
| `backward_tls` | Use stunnel or `+ssl` on pproxy side. |
| `python_api_protocol_classes`, `python_api_cipher_access` | pproxy-only; eggress uses config structs and ring/chacha crates. |
| `python_system_proxy_inspect` | Not yet implemented. |

## Platform-specific support

See [`PLATFORM_SUPPORT_MATRIX.md`](PLATFORM_SUPPORT_MATRIX.md) for the full
matrix. Highlights:

- **Transparent TCP proxy:** Linux only (`SO_ORIGINAL_DST`).
- **Unix domain sockets:** Linux, macOS, Unix (not Windows).
- **macOS PF original-destination recovery:** Intentional non-parity; not
  implemented. Use `pfctl` with a standard listener.
- **System proxy inspection:** macOS, Linux, Windows; apply / revert are
  macOS-only.

## Python API support

The `eggress` Python package exposes:

- `EggressConfig` / `EggressService` / `EggressHandle` (embed-style API).
- `translate_pproxy_args`, `translate_pproxy_uri` (CLI translation).
- `check_pproxy_args`, `check_pproxy_uri`, `redact_pproxy_uri`,
  `diagnostics_for_uri` (compatibility checks).
- `explain_config_toml`, `explain_pproxy_args`, `explain_pproxy_uri`,
  `route_explain`, `test_upstream_connect` (configuration introspection).
- `Server.is_ready`, `listener_info`, `metrics_text` (status helpers).
- `supported_features()`, `UriInfo`, `Diagnostic` (data classes).

See [`docs/PYTHON_BINDINGS.md`](../../PYTHON_BINDINGS.md) for the full Python
API reference.

## Security posture

Phase 35 added eight security features; all are `supported` with synthetic
evidence (config validation tests, property tests, dependency policy):

| Feature | Evidence |
|---|---|
| `security_open_proxy_warning` | `warn_non_loopback_listener_without_auth` in `eggress-config` |
| `security_admin_bind_warning` | `warn_non_loopback_admin` |
| `security_secret_redaction` | `PproxyUri::redacted_display()` tests |
| `security_reverse_allowlist` | `validate_non_loopback_*` in `eggress-protocol-reverse` |
| `security_transparent_capability_diagnostics` | `eggress-runtime::platform` |
| `security_resource_limits` | `validate_listener_udp` in `eggress-config` |
| `security_parser_property_tests` | `proptest!` in 4 crates |
| `security_dependency_audit` | CI runs `cargo deny check` and `cargo audit`; `deny.toml` bans `openssl-sys`, `native-tls`, `aws-lc-sys`, `cmake` |

See [`docs/SECURITY_REVIEW.md`](../../SECURITY_REVIEW.md) for the full security
review.

## Performance baseline

Phase 34 established the baseline:

- **TCP relay smoke:** 50 concurrent SOCKS5 sessions under 5s (gated).
- **UDP relay smoke:** 100 datagrams through standalone UDP relay (gated).
- **FD leak smoke:** FD count returns to baseline after 20 sessions (gated).
- **Task leak smoke:** Task count returns to zero after 20 concurrent sessions
  (gated).
- **Reverse soak:** 30s sustained reverse proxy load (gated with
  `EGRESS_REQUIRE_SOAK=1`).
- **Python binding overhead:** import cost, URI translation, config parse,
  service lifecycle.

See [`docs/performance/BASELINE.md`](../../performance/BASELINE.md) and
[`docs/performance/REGRESSION_GATE_POLICY.md`](../../performance/REGRESSION_GATE_POLICY.md).

## Known release blockers

None identified at the time of this audit. See
[`PARITY_RELEASE_GO_NO_GO.md`](PARITY_RELEASE_GO_NO_GO.md) for the final
decision.

## Accepted limitations

These are limitations that the release explicitly accepts. They do **not**
block the release but are documented for users.

1. **Python 3.14 cannot run `pproxy==2.7.9`** because pproxy 2.7.9 calls
   `asyncio.get_event_loop()` which is removed in 3.14. The eggress Python
   package itself supports 3.14; only the gated differential tests against
   pproxy require Python 3.11/3.12.
2. **No hosted CI visibility** for this release. Local verification is the
   source of truth; see [`docs/CI_STATUS.md`](../../CI_STATUS.md). Do not
   assume hosted GitHub checks passed — no statuses are observable.
3. **MacOS PF original-destination recovery** is intentionally not
   implemented; users must use `pfctl` with a standard listener.
4. **QUIC / HTTP/3** is intentionally deferred per ADR.
5. **Multi-hop UDP** is intentionally not supported (pproxy does).
6. **Backward TLS**, **backward parallel connections**, and **backward jump
   chains** are deferred.

## How to verify

```bash
# Manifest consistency
cargo test -p eggress-testkit --lib manifest

# Workspace tests
cargo test --workspace

# Format / lint
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# Supply chain
cargo deny check
cargo audit

# Python
maturin develop
python -m pytest python/tests -q
```

Gated differential / interop tests require `pproxy==2.7.9` and Python 3.11:

```bash
python3.11 -m pip install "pproxy==2.7.9"
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1
EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test -p eggress-runtime --test reverse_interop -- --ignored --test-threads=1
```

## Report generation

The machine-readable JSON report is a build artifact, not a committed file.
Generate it with:

```bash
python3 scripts/phase36_report.py
# Writes target/compat/final-pproxy-parity-report.json
```

This file is not committed to the repository (it lives in `target/`).