# Phase 39: pproxy URI Grammar & Chain Semantics Completion Record

## Summary

Closed the default port inference gap in Phase 39. pproxy assigns standard default ports for known schemes (e.g., socks5→1080, http→80, trojan→443, ss→8388). The pproxy-compat URI parser now applies these defaults when no explicit port is given. Previously, `socks5://host` would error with "missing port in endpoint"; now it correctly infers port 1080. All existing chain, modifier, and reverse URI features were already implemented.

## Status: Complete

## Commits

### Files Modified

- `crates/eggress-pproxy-compat/src/uri.rs` — Relaxed `parse_endpoint` to return `(host, 0)` on missing port instead of erroring; added `default_port_for_scheme()` mapping; applied default ports in `parse_pproxy_uri` when port==0 and host is non-empty; added 8 tests

### Files Created

None

## Workstream Decisions

### 1. Default port mapping
**Decision:** Implemented `default_port_for_scheme()` returning: http/h2→80, https/ws/wss/trojan→443, socks4/socks4a/socks5→1080, ss/shadowsocks→8388, ssh→22.
**Rationale:** Matches pproxy's built-in port inference behavior. Unknown schemes return `None` (no default), preserving the previous error behavior for unrecognized protocols.

### 2. parse_endpoint relaxation
**Decision:** Changed `parse_endpoint` to return `(endpoint, 0)` when there's a host but no colon-separated port, instead of returning an error.
**Rationale:** This allows the caller (`parse_pproxy_uri`) to apply scheme-specific defaults. Bare hosts like `socks5://proxy` now parse successfully with port 0, which is then replaced by the default.

### 3. Conditional inference
**Decision:** Default ports are only applied when `port == 0 && !host.is_empty()`.
**Rationale:** Empty endpoints (e.g., `socks5://:1080`) should preserve explicit port 1080. Only bare-host URIs (e.g., `socks5://proxy`) get the default.

## Verification Commands Run

| Command | Status |
|---------|--------|
| `cargo check --workspace` | PASS |
| `cargo test -p eggress-pproxy-compat` | PASS (211 tests) |
| `cargo fmt --all -- --check` | PASS |
