# Compatibility Evidence Table

Canonical evidence source for pproxy compatibility claims. Generated from the
manifest at [`tests/compat/pproxy_manifest.toml`](../tests/compat/pproxy_manifest.toml).
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
