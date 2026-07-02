# Compatibility Evidence Table

Canonical evidence source for pproxy compatibility claims. Manually synchronized
from the manifest at [`tests/compat/pproxy_manifest.toml`](../tests/compat/pproxy_manifest.toml).
Keep this document in sync with the manifest.

## Evidence Tiers

| Tier | Meaning | Claim strength |
|------|---------|----------------|
| **Compatible** | Behavior matches pproxy for tested scenarios; has runtime or differential test reference | Full parity — safe to migrate |
| **Supported** | Feature is implemented and tested, but pproxy equivalence is not claimed | Functional — may differ in edge cases |
| **Partial** | Usable subset exists; not enough for full compatibility | Limited — verify specific use case |
| **Intentional non-parity** | Deliberately not replicated with rationale | By design — see rationale |

## Evidence Types

| Evidence type | Meaning | Gated? |
|---------------|---------|--------|
| Differential (pproxy==2.7.9) | Tested against real pproxy; behavioral match verified | Yes |
| Standard AEAD interop | Interoperable with standard Shadowsocks implementations | No |
| Synthetic | Tested without a real pproxy instance | No |

## Inbound TCP Protocols

| Feature | Tier | Evidence | How to run |
|---------|------|----------|------------|
| `http_connect_server` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `http_forward_proxy` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `socks4_server` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `socks4a_server` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `socks5_connect_server` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `socks5_udp_associate_server` | Supported | Differential (pproxy==2.7.9) — framing differs | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `shadowsocks_tcp_upstream` | Supported | Standard AEAD interop | `cargo test -p eggress-protocol-shadowsocks` |
| `trojan_upstream` | Partial | Synthetic only (client-only) | `cargo test -p eggress-protocol-trojan` |

## Inbound UDP Protocols

| Feature | Tier | Evidence | How to run |
|---------|------|----------|------------|
| `socks5_udp_associate_relay` | Partial | Differential (pproxy==2.7.9) — framing differs | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `shadowsocks_udp` | Supported | Standard AEAD interop | `cargo test -p eggress-runtime shadowsocks_udp` |
| `direct_udp_forwarding` | Supported | Synthetic | `cargo test -p eggress-runtime udp` (SOCKS5 ASSOCIATE) + `cargo test -p eggress-runtime standalone` (standalone pproxy UDP relay) |
| `standalone_udp_relay` | Supported | Differential (pproxy==2.7.9) — standalone mode | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored -- standalone_udp` |
| `standalone_udp_error_handling` | Supported | Differential (pproxy==2.7.9) — malformed/frag handling | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored -- standalone_udp` |

## Upstream TCP Protocols

| Feature | Tier | Evidence | How to run |
|---------|------|----------|------------|
| `http_connect_upstream` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `socks4_upstream` | Supported | Synthetic | `cargo test -p eggress-runtime upstream_protocols` |
| `socks5_upstream` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `shadowsocks_upstream` | Supported | Standard AEAD interop | `cargo test -p eggress-protocol-shadowsocks` |
| `trojan_upstream_client` | Partial | Synthetic | `cargo test -p eggress-protocol-trojan` |
| `direct_upstream` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |

## Upstream UDP Protocols

| Feature | Tier | Evidence | How to run |
|---------|------|----------|------------|
| `socks5_udp_upstream` | Partial | Synthetic | `cargo test -p eggress-runtime udp_upstream` |
| `shadowsocks_udp_upstream` | Supported | Standard AEAD interop | `cargo test -p eggress-runtime shadowsocks_udp` |

## Chain Behavior

| Feature | Tier | Evidence | How to run |
|---------|------|----------|------------|
| `single_hop_tcp_chain` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `multi_hop_tcp_chain` | Partial | Synthetic | `cargo test -p eggress-runtime multihop_tcp` |
| `udp_chain` | Partial | Synthetic | `cargo test -p eggress-runtime udp` |
| `chain_capability_validation` | Supported | Synthetic | `cargo test -p eggress-runtime upstream_protocols` |

## Scheduler Behavior

| Feature | Tier | Evidence | How to run |
|---------|------|----------|------------|
| `round_robin_scheduler` | Supported | Synthetic | `cargo test -p eggress-runtime scheduler_runtime` |
| `first_available_scheduler` | Supported | Synthetic | `cargo test -p eggress-runtime scheduler_runtime` |
| `random_scheduler` | Supported | Synthetic | `cargo test -p eggress-runtime scheduler_runtime` |
| `least_connections_scheduler` | Supported | Synthetic | `cargo test -p eggress-runtime scheduler_runtime` |
| `health_aware_skip` | Supported | Synthetic | `cargo test -p eggress-runtime scheduler_runtime` |
| `scheduler_state_persistence` | Intentional non-parity | — | — |

## Authentication

| Feature | Tier | Evidence | How to run |
|---------|------|----------|------------|
| `socks5_auth_rejection` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `http_auth_rejection` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `socks5_username_password` | Supported | Synthetic | `cargo test -p eggress-runtime integration` |
| `http_basic_auth` | Supported | Synthetic | `cargo test -p eggress-runtime integration` |
| `shadowsocks_password` | Supported | Standard AEAD interop | `cargo test -p eggress-protocol-shadowsocks` |

## Phase 19 HTTP/SOCKS Baseline Closure

All features below have compatible status backed by differential evidence against pproxy==2.7.9.

| Feature | Tier | Evidence | How to run |
|---------|------|----------|------------|
| `http_connect_auth_success` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `http_connect_auth_rejection` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `http_connect_ipv4_target` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `http_connect_domain_target` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `http_connect_ipv6_target` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `http_connect_refused_target` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `http_forward_persistent_connection` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `http_forward_post_body` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `http_forward_head` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `http_forward_connection_close` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `socks4_connect` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `socks4a_domain_connect` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `socks5_connect_ipv6` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `socks5_connect_domain` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |
| `socks5_refused_target` | Compatible | Differential (pproxy==2.7.9) | `EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored` |

## CLI Compatibility

| Feature | Tier | Evidence | How to run |
|---------|------|----------|------------|
| `listen_flag` | Compatible | Synthetic | `cargo test -p eggress-cli` |
| `remote_flag` | Compatible | Synthetic | `cargo test -p eggress-cli` |
| `udp_listen_flag` | Compatible | Synthetic (pproxy translate) | `cargo test -p eggress-pproxy-compat` |
| `udp_remote_flag` | Compatible | Synthetic (pproxy translate) | `cargo test -p eggress-pproxy-compat` |
| `pproxy_translate_command` | Compatible | Synthetic | `cargo test -p eggress-cli` |
| `pproxy_check_command` | Compatible | Synthetic | `cargo test -p eggress-cli` |
| `pproxy_run_command` | Compatible | Synthetic | `cargo test -p eggress-cli` |
| `cli_exit_codes` | Compatible | Synthetic (exit code constants, subprocess tests) | `cargo test -p eggress-cli cli_exit_codes` |
| `cli_check_json` | Compatible | Synthetic (`--json` flag produces structured output) | `cargo test -p eggress-cli cli_exit_codes` |
| `cli_diagnostics_taxonomy` | Compatible | Synthetic (DiagnosticCode serialization, 13 codes) | `cargo test -p eggress-pproxy-compat diagnostics` |
| `cli_translate_golden` | Compatible | Synthetic (deterministic translation output) | `cargo test -p eggress-cli pproxy_translation_golden` |
| `cli_translate_chain` | Compatible | Synthetic (chain translation via __ separator) | `cargo test -p eggress-pproxy-compat` |
| `cli_translate_scheduler` | Compatible | Synthetic (scheduler flag translation) | `cargo test -p eggress-cli pproxy_translation_golden` |
| `cli_translate_auth` | Compatible | Synthetic (auth flag translation) | `cargo test -p eggress-cli pproxy_translation_golden` |
| `cli_translate_reverse` | Compatible | Synthetic (backward/reverse mode translation) | `cargo test -p eggress-cli pproxy_translation_golden` |
| `cli_translate_standalone_udp` | Supported | Synthetic (standalone UDP translation) | `cargo test -p eggress-cli pproxy_translation_golden` |
| `cli_translate_ssr_rejection` | Intentional non-parity | Synthetic (SSR URIs rejected with diagnostics) | `cargo test -p eggress-pproxy-compat` |
| `cli_translate_ssh_rejection` | Intentional non-parity | Synthetic (SSH URIs rejected with diagnostics) | `cargo test -p eggress-pproxy-compat` |
| `cli_run_process_behavior` | Compatible | Synthetic (signal handling, clean shutdown) | `cargo test -p eggress-cli pproxy_run_process` |
| `cli_inventory_complete` | Compatible | Synthetic (full flag inventory documented) | docs review |

## Gated vs Ungated Tests

**Gated tests** require all of:
- `EGRESS_REQUIRE_EXTERNAL_INTEROP=1` environment variable
- Python 3 with `pproxy==2.7.9` installed
- A running pproxy process (started automatically by the test harness)

Run gated differential tests with:
```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
```

**Ungated tests** run without pproxy. They exercise eggress behavior independently:
- Standard integration tests (`cargo test -p eggress-runtime`)
- Unit tests (`cargo test -p eggress-protocol-*`)
- Property and fuzz tests

## Manifest

The source of truth is [`tests/compat/pproxy_manifest.toml`](../tests/compat/pproxy_manifest.toml).
All compatibility claims in this document and in [PARITY_MATRIX.md](PARITY_MATRIX.md) must have
a corresponding manifest entry with an appropriate evidence level.

After running differential tests, a machine-readable parity report is generated at
`target/compat/pproxy-parity-report.json`.

## Phase 26: Advanced Transports

| Feature | Status | Evidence |
|---------|--------|----------|
| H2 CONNECT server | Supported | Synthetic (unit tests) |
| H2 CONNECT upstream | Supported | Synthetic (unit tests) |
| WebSocket tunnel server | Supported | Synthetic (echo, close, max-size tests) |
| WebSocket tunnel upstream | Supported | Synthetic (connect test) |
| Raw fixed-target tunnel | Supported | Synthetic (bind + relay test) |
| TLS ALPN config | Supported | Synthetic (config compilation test) |
| H2/WS/Raw URI schemes | Supported | Synthetic (parser tests) |
| QUIC/H3 | Deferred | ADR (no implementation) |

## Phase 27: Reverse / Backward Proxying

| Feature | Tier | Evidence | How to run |
|---------|------|----------|------------|
| `reverse_server_bind_listener` | Supported | Synthetic (unit + integration tests in `eggress-protocol-reverse`) | `cargo test -p eggress-protocol-reverse` |
| `reverse_client_backward_modifier` | Supported | Synthetic (unit + integration tests in `eggress-protocol-reverse`) | `cargo test -p eggress-protocol-reverse` |
| `reverse_auth_handshake` | Supported | Synthetic (success/failure unit tests in `eggress-protocol-reverse`) | `cargo test -p eggress-protocol-reverse` |
| `reverse_relay_bidirectional` | Supported | Synthetic (echo integration test) | `cargo test -p eggress-protocol-reverse` |
| `reverse_allow_bind_security` | Supported | Synthetic (allow-list integration tests) | `cargo test -p eggress-protocol-reverse` |
| `reverse_max_control_streams` | Supported | Synthetic (max control / max streams integration tests) | `cargo test -p eggress-protocol-reverse` |
| `reverse_redacted_logging` | Supported | Synthetic (`redact_auth` unit tests + log smoke checks) | `cargo test -p eggress-protocol-reverse` |
| `reverse_metrics` | Supported | Synthetic (Prometheus format + per-state integration tests) | `cargo test -p eggress-protocol-reverse` |
| `reverse_graceful_drain` | Supported | Synthetic (drain metric + graceful shutdown integration tests) | `cargo test -p eggress-protocol-reverse` |
| `reverse_routing_integration` | Supported | Synthetic (`RouteEngineTargetResolver` unit tests in `eggress-runtime`) | `cargo test -p eggress-runtime reverse::` |
| `reverse_admin_endpoint` | Supported | Synthetic (`/-/reverse` admin route integration tests) | `cargo test -p eggress-admin` |
| `reverse_python_helper` | Supported | Synthetic (Python `describe_reverse_pproxy_uri` tests) | `python -m pytest python/tests/test_reverse_uri_helper.py` |
| `reverse_eggress_self_interop` | Supported | Synthetic (loopback self-interop test) | `cargo test -p eggress-runtime --test reverse_interop -- reverse_eggress_self_interop_loopback` |
| `reverse_pproxy_interop` | Supported | Synthetic (gated pproxy handshake smoke tests) | `EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test -p eggress-runtime --test reverse_interop -- --ignored` |

**Note on differential evidence:** Phase 27 does not yet claim pproxy-compatible
status for reverse mode. The two gated tests (`reverse_pproxy_interop`) verify
handshake wiring in both directions but do not compare wire-format byte-for-byte
behavior against pproxy==2.7.9. To upgrade to **Compatible**, additional
differential tests must be added that run a real pproxy 2.7.9 instance against an
eggress reverse server/client, exchange a known payload through a relay, and
assert byte equality. The `EGRESS_REQUIRE_REVERSE_INTEROP` test scaffold is the
documented place for that work.

## Phase 28: CLI Exit Codes, JSON Output, and Structured Diagnostics

| Feature | Tier | Evidence | How to run |
|---------|------|----------|------------|
| `cli_exit_codes` | Compatible | Synthetic (exit code constants, subprocess tests) | `cargo test -p eggress-cli cli_exit_codes` |
| `cli_check_json` | Compatible | Synthetic (`--json` flag produces JSON with tier, diagnostics, features) | `cargo test -p eggress-cli cli_exit_codes` |
| `cli_diagnostics_taxonomy` | Compatible | Synthetic (all 13 codes serialize correctly) | `cargo test -p eggress-pproxy-compat diagnostics::tests::all_diagnostic_codes_serialize` |
| `cli_translate_golden` | Compatible | Synthetic (deterministic translation output for fixtures) | `cargo test -p eggress-cli pproxy_translation_golden` |
| `cli_translate_chain` | Compatible | Synthetic (chain translation via __ separator) | `cargo test -p eggress-pproxy-compat` |
| `cli_translate_scheduler` | Compatible | Synthetic (scheduler flag translation) | `cargo test -p eggress-cli pproxy_translation_golden` |
| `cli_translate_auth` | Compatible | Synthetic (auth flag translation) | `cargo test -p eggress-cli pproxy_translation_golden` |
| `cli_translate_reverse` | Compatible | Synthetic (backward/reverse mode translation) | `cargo test -p eggress-cli pproxy_translation_golden` |
| `cli_run_process_behavior` | Compatible | Synthetic (signal handling, clean shutdown, readiness) | `cargo test -p eggress-cli pproxy_run_process` |
| `cli_inventory_complete` | Compatible | Synthetic (full flag inventory documented) | docs review |
| `cli_translate_ssr_rejection` | Intentional non-parity | Synthetic (SSR URIs rejected with diagnostics) | `cargo test -p eggress-pproxy-compat` |
| `cli_translate_ssh_rejection` | Intentional non-parity | Synthetic (SSH URIs rejected with diagnostics) | `cargo test -p eggress-pproxy-compat` |

**Note:** Phase 28 exit codes and JSON output are classified as **Compatible**
with eggress behavior (not pproxy behavioral parity). pproxy uses a single exit
code (1) for all errors; eggress provides differentiated codes by design.

## Phase 29: Python API Discovery and Parity Spec

Phase 29 established the Python API compatibility specification. The evidence
below reflects specification-level coverage (synthetic), not behavioral parity.

| Feature | Tier | Evidence | How to run |
|---------|------|----------|------------|
| `python_module_import` | Supported | Synthetic (import test) | `python -m pytest python/tests/test_pproxy_oracle.py` |
| `python_version_metadata` | Supported | Synthetic (snapshot comparison) | `EGRESS_REQUIRE_PPROXY_ORACLE=1 python -m pytest python/tests/test_pproxy_oracle.py` |
| `python_server_constructor` | Partial | Synthetic (native + Python wrappers) | `python -m pytest python/tests/test_pproxy_oracle.py` |
| `python_server_lifecycle_async` | Supported | Synthetic (EggressService async) | `python -m pytest python/tests/test_pproxy_oracle.py` |
| `python_server_lifecycle_blocking` | Supported | Synthetic (EggressService blocking) | `python -m pytest python/tests/test_pproxy_oracle.py` |
| `python_server_context_manager` | Supported | Synthetic (sync + async CM) | `python -m pytest python/tests/test_pproxy_oracle.py` |
| `python_listen_uri_api` | Supported | Synthetic (pproxy translate) | `python -m pytest python/tests/test_pproxy_compat.py` |
| `python_remote_uri_api` | Supported | Synthetic (pproxy translate) | `python -m pytest python/tests/test_pproxy_compat.py` |
| `python_chain_api` | Partial | Synthetic (translation only) | `python -m pytest python/tests/test_pproxy_compat.py` |
| `python_auth_api` | Supported | Synthetic (pproxy translate) | `python -m pytest python/tests/test_pproxy_compat.py` |
| `python_error_types` | Supported | Synthetic (7 exception classes) | `python -m pytest python/tests/test_pproxy_compat.py` |
| `python_reload_api` | Supported | Synthetic (EggressHandle.reload_config) | `python -m pytest python/tests/test_pproxy_oracle.py` |

**Note:** Phase 29 is a specification phase. All evidence is `implemented_synthetic`.
No pproxy behavioral parity is claimed for Python API surfaces. Tier classifications
are in `docs/python/PPROXY_API_INVENTORY.md`. Full differential testing of Python API
surfaces is deferred to Phase 30+.

## Gated Python Tests

```bash
# Oracle tests (requires pproxy package)
EGRESS_REQUIRE_PPROXY_ORACLE=1 python -m pytest python/tests/test_pproxy_oracle.py -v

# pproxy compat tests
python -m pytest python/tests/test_pproxy_compat.py -v
```
