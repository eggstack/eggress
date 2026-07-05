# Phase 38: pproxy CLI Native-Equivalent Closure Completion Record

## Summary

Closed the remaining gaps in Phase 38 where pproxy CLI flags were translated to warnings-only rather than actionable TOML config or equivalent eggress behavior. The `-a` (alive) flag now generates a `[health]` section with the interval, `--pac` generates an `[admin.pac]` block, `--test` performs a test-and-exit via `eggress upstream test`, and `--sys` auto-invokes `eggress system-proxy inspect` before starting the service.

## Status: Complete

## Commits

### Files Modified

- `crates/eggress-pproxy-compat/src/translate.rs` — Added `HealthToml`, `PacToml`, `AdminToml` structs; extended `ConfigToml` with `health` and `admin` fields; updated `generate_toml` to accept `health_interval` and `pac_enabled` params; added 2 tests for health and PAC generation
- `crates/eggress-cli/src/main.rs` — `handle_pproxy_run` now detects `--sys` and auto-invokes system proxy inspection, and detects `--test` and runs `eggress upstream test` before exiting

### Files Created

None

## Workstream Decisions

### 1. Health TOML generation
**Decision:** `-a N` generates `[health] interval = "Ns"` in the translated TOML.
**Rationale:** Matches eggress config schema where `health.interval` accepts a duration string like `"10s"`. The pproxy `-a` flag takes a numeric seconds value, so we append `"s"`.

### 2. PAC TOML generation
**Decision:** `--pac` generates `[admin.pac] enabled = true` in the translated TOML.
**Rationale:** eggress admin PAC serving is configured via `admin.pac.enabled` in TOML config. The flag is a simple enable toggle.

### 3. Test-and-exit path
**Decision:** `--test` writes the translated TOML to a temp file, invokes `eggress upstream test -c <config>`, and exits with the child's exit code.
**Rationale:** pproxy's `--test` validates connectivity before starting the service. eggress has a dedicated `upstream test` subcommand that performs the same validation. Spawning it as a child process preserves exit code semantics.

### 4. System proxy auto-inspection
**Decision:** `--sys` calls `inspect_system_proxy()` + `print_inspection_result()` before starting the service.
**Rationale:** pproxy's `--sys` displays system proxy settings. eggress's `system-proxy inspect` provides equivalent output. Auto-invoking it gives the user the same information without requiring a separate command.

## Verification Commands Run

| Command | Status |
|---------|--------|
| `cargo check --workspace` | PASS |
| `cargo test -p eggress-pproxy-compat` | PASS (211 tests) |
| `cargo fmt --all -- --check` | PASS |
