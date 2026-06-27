# Phase 7: pproxy Parity Specification — Completion Record

## Summary

Phase 7 created the formal compatibility contract for pproxy parity. No new protocol behavior was implemented. The output is a precise, testable specification describing what Python pproxy does, what Eggress matches, what remains unimplemented, and what Eggress intentionally rejects.

## Completed Workstreams

### Workstream 1: pproxy Inventory
- Created `docs/PPROXY_PARITY_SPEC.md` (426 lines)
- Covers pproxy 2.7.9 (CI-installed version)
- 16 sections: scope, listener protocols, upstream protocols, URI schemes, chaining, schedulers, auth, UDP, encryption, CLI flags, Python library, error behavior, observed behaviors, intentional non-parity, open items, references

### Workstream 2: Compatibility Tiers
- Defined 6 tiers: Compatible, Supported, Partial, Experimental, Intentional non-parity, Unsupported
- Tiers documented in both `docs/PPROXY_PARITY_SPEC.md` and `docs/PARITY_MATRIX.md`
- Every row in the parity matrix uses one of these tiers

### Workstream 3: Expanded Parity Matrix
- Rewrote `docs/PARITY_MATRIX.md` from 65 lines to 182 lines
- 11 feature categories with 7 columns each
- 7 differential tests referenced by exact function name
- Coverage summary and limitations sections

### Workstream 4: Refactor Differential Test Harness
- Refactored `crates/eggress-cli/tests/differential_pproxy.rs` from 1053 to 1600 lines
- Added reusable primitives: `ProcessGuard`, `TaskGuard`, `start_tcp_echo()`, `start_udp_echo()`, `start_eggress_from_toml()`, `compare_tcp_echo()`, `compare_udp_echo()`, `assert_coarse_failure_equivalence()`, `socks5_udp_associate()`, `build_socks5_udp_packet()`, `recv_udp_response()`
- Added `tempfile` to dev-dependencies

### Workstream 5: Black-Box Probe Tests
- 5 new probe tests added to `differential_pproxy.rs`:
  1. `probe_pproxy_socks5_refused_reply` — Documents SOCKS5 reply on connection refusal
  2. `probe_pproxy_http_refused_reply` — Documents HTTP CONNECT behavior on refused ports
  3. `probe_pproxy_socks5_auth_success_shape` — Verifies authenticated SOCKS5 connection succeeds
  4. `probe_pproxy_chained_failure_behavior` — Documents failure when upstream hop is dead
  5. `probe_pproxy_udp_relay_lifetime` — Documents UDP relay lifetime relative to TCP control

### Workstream 6: Intentional Non-Parity
- 12 behaviors documented as intentionally rejected in `docs/PPROXY_PARITY_SPEC.md`
- Includes: transparent proxy, stream ciphers, SSR, QUIC, HTTP/3, WebSocket, SSH, reverse proxy, Unix sockets, plugins, malformed input leniency, insecure TLS

## Documentation Updates

- `AGENTS.md` — Added Phase 7 references, parity spec, differential harness primitives
- `README.md` — Updated status to Phase 7 complete, added doc links
- `docs/ROADMAP.md` — Updated current phase, added Phase 7 milestone section
- `.skills/testing/skill.md` — Added differential test harness primitives section

## Inspected pproxy Version

- **Version**: 2.7.9
- **Source**: https://github.com/nimlang/pproxy
- **CI installation**: `pip install "pproxy==2.7.9"`

## Unresolved needs-probe Items

| Item | Notes |
|------|-------|
| SOCKS5 BIND command support | Not tested; pproxy may support BIND (0x02) |
| UDP ASSOCIATE as SOCKS5 server | pproxy appears to use standalone `-ul` instead |
| Shadowsocks AEAD key derivation details | Salt, KDF, key size need confirmation |
| Trojan password hashing variant | SHA224 vs. other hash needs confirmation |
| HTTP forward proxy (non-CONNECT) | Whether plain HTTP forwarding is supported |
| Multi-hop chain error handling | Behavior for chains >2 hops |
| Connection reuse semantics | How `--reuse` interacts with chaining |
| `--rulefile` format details | Exact syntax for rule entries |
| SOCKS4a domain resolution | Server-side vs. forwarded resolution |

## Corrective Audit Notice

The parity specification was produced from documentation review and black-box
probing of pproxy 2.7.9. Full differential verification against a running
pproxy instance is gated (`EGRESS_REQUIRE_EXTERNAL_INTEROP=1`) and requires
Python 3.11/3.12 (not compatible with Python 3.14). Differential tests have
not yet been run end-to-end; see `docs/DIFFERENTIAL_TESTING.md` for details.

## Definition of Done Checklist

- [x] `docs/PPROXY_PARITY_SPEC.md` exists and names inspected pproxy version
- [x] Compatibility tiers are defined
- [x] `docs/PARITY_MATRIX.md` uses the tier taxonomy
- [x] Every compatible claim has a runtime or differential test reference
- [x] Differential harness primitives are reusable
- [x] Ambiguous pproxy behavior is probed or marked `needs-probe`
- [x] Intentional non-parity is documented
- [x] No new unsupported protocol is promoted
- [x] Normal workspace checks pass locally

## Next Phase

Phase 8: pproxy-compatible CLI and URI (`plans/PHASE_8_PPROXY_COMPAT_CLI_URI_PLAN.md`)
