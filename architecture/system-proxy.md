# eggress-system-proxy — OS Proxy Settings (Inspect / Apply / Rollback)

Leaf crate that reads and mutates operating-system proxy configuration,
powering `eggress system-proxy inspect` and the pproxy-compatible `--sys`
flag. Explicitly platform-classified: unsupported combinations fail with
structured diagnostics rather than guessing.

## Module map

| File | Role |
|---|---|
| `src/apply.rs` | `plan_apply()` → `ApplyPlan` (reviewable command list), `apply_compatibility_proxy[_with_runner]()`, `AppliedProxy` handle, `RollbackState`, `Command`, `CompatibilityProxyKind` |
| `src/capability.rs` | `check_system_proxy_capability()`, `system_proxy_platform_info()` → per-platform `SystemProxyCapabilityReport` (supported / degraded / unavailable with reasons) |
| `src/backends/{linux,macos,windows,env}.rs` | Per-OS mechanisms (gsettings/KDE env on Linux, `networksetup` on macOS, registry on Windows, plain env-var fallback) |
| `src/command_runner.rs` | `CommandRunner` trait + `RealCommandRunner` / `MockCommandRunner` (tests inject mocks, never shell out) |
| `src/inspection.rs` | `inspect_system_proxy()` → current `SystemProxySettings` |
| `src/redaction.rs` | Credentials stripped from any rendered plan/output |

## Design notes

- Plan-then-apply: callers can render the exact mutations before executing;
  rollback restores prior state even on partial failure.
- Capability checks run before apply so unsupported platforms get actionable
  messages (surfaced via `platform_capability_check_failures_total` metric).

## Interactions

- `eggress-cli` subcommand + compat `--sys` path; exposed to Python via
  `apply_system_proxy()` binding.

## Review entry points

- Verify: `cargo test -p eggress-system-proxy`
