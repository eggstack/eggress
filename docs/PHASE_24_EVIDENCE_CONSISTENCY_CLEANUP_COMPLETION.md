# Phase 24: Evidence Consistency Cleanup — Completion

**Date:** 2026-06-30
**Status:** Complete

## Summary

Phase 24 is a targeted cleanup pass that resolved the last visible metadata-vs-reality
contradictions before continuing with new pproxy parity feature work. No new protocol
capability was added. The goal was to make the current compatibility story internally
consistent, mechanically validated, and easy to audit.

## Changes

### 24.1: Removed `last_updated` from manifest

- Removed `last_updated = "2025-06-30"` from `tests/compat/pproxy_manifest.toml`
- Removed `last_updated` field from `ManifestMeta` struct
- Removed `StaleLastUpdated` error variant and `is_recent_date()` function from `eggress-testkit::manifest`
- Rationale: a hand-maintained timestamp is easy to stale; the manifest is versioned by Git history; generated reports carry run timestamps

### 24.2: Reconciled `-ul` and `-ur` manifest entries

- Changed `udp_listen_flag` from `intentional_non_parity` → `compatible` with evidence `compatible`
- Changed `udp_remote_flag` from `intentional_non_parity` → `compatible` with evidence `compatible`
- Updated divergence notes to describe standalone pproxy UDP mode translation, not "SOCKS5 UDP ASSOCIATE instead"
- Added concrete test names: `test_ul_generates_standalone_udp_listener`, `test_ul_and_ur_generates_udp_upstream_group`
- Tests confirmed: `test_ul_generates_standalone_udp_listener` and `test_ul_and_ur_generates_udp_upstream_group` exist in `eggress-pproxy-compat`

### 24.3: Added standalone UDP-specific manifest feature IDs

- Added `standalone_udp_relay` (category: udp, status: supported, evidence: implemented_differential)
- Added `standalone_udp_error_handling` (category: udp, status: supported, evidence: implemented_differential)
- Updated `direct_udp_forwarding` from `implemented_synthetic` → `implemented_differential` with standalone test references
- All three entries include `external_dependency = "pproxy==2.7.9"` where applicable

### 24.4: Standalone UDP differential tests

- Existing differential tests already had proper names: `differential_standalone_udp_direct_echo`, `differential_standalone_udp_domain_target`, `differential_standalone_udp_malformed_short_datagram`, `differential_standalone_udp_nonzero_frag`, `differential_standalone_udp_two_clients`, `differential_standalone_udp_two_targets_from_same_client`, `differential_standalone_udp_oversized_datagram`
- Manifest entries now reference these test names directly

### 24.5: Fixed parity matrix contradictions

- Fixed standalone UDP relay row: changed differential test reference from `differential_socks5_udp_associate` to `differential_standalone_udp_direct_echo, differential_standalone_udp_domain_target`
- Fixed `-ul`/`-ur` CLI rows: added `pproxy_cli_tests` to runtime test references
- Changed "Retry within group" from `Compatible` to `Supported` (pproxy behavior undocumented)
- Updated coverage summary to reflect `-ul`/`-ur` as properly classified compatible

### 24.6: Fixed compatibility evidence doc generation claim

- Changed `docs/COMPATIBILITY_EVIDENCE.md` header from "Generated from" to "Manually synchronized from"
- Added standalone UDP feature entries (`standalone_udp_relay`, `standalone_udp_error_handling`)
- Added `-ul`/`-ur` CLI entries (`udp_listen_flag`, `udp_remote_flag`)

### 24.7: CI status note

- `docs/CI_STATUS.md` already existed with comprehensive coverage of workflows, local verification, and billing limitations
- No changes needed; document accurately distinguishes configured CI from observed green runs

### 24.8: Tighten manifest validation for external dependency claims

Added three new validation rules to `eggress-testkit::manifest`:

1. **CompatibleDifferentialMissingExternalDependency**: compatible evidence with `differential_` test names requires `external_dependency`
2. **InteropMissingExternalDependencyOrDivergence**: `implemented_interop` without `external_dependency` requires a divergence explaining the interop suite
3. **MissingExternalDependency**: compatible/`implemented_differential` evidence with `differential_` tests requires `external_dependency`

Added corresponding unit tests. All manifest entries now pass validation.

## Verification

```bash
cargo fmt --all -- --check           # PASS
cargo check --workspace --all-targets # PASS
cargo clippy --workspace --all-targets -- -D warnings # PASS
cargo test -p eggress-testkit manifest # PASS (31 tests including validate_real_manifest)
cargo test -p eggress-pproxy-compat   # PASS (88 tests)
cargo test -p eggress-protocol-socks --test codec_properties # PASS (14 tests)
```

Hosted CI is non-functional due to billing issues. Local verification is the source of truth.

## Remaining gaps (unchanged from before this phase)

- UDP multi-hop chains
- Trojan server/listener
- SSH upstream transport
- Transparent proxy/redir/PF and Unix sockets
- HTTP/2, HTTP/3, QUIC, WebSocket, raw tunnel, reverse/backward proxying
- System proxy configuration
- True pproxy-shaped Python API drop-in replacement
- Legacy Shadowsocks/SSR intentional non-parity unless the ADR changes
