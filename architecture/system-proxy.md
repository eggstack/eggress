# eggress-system-proxy — OS Proxy Settings (Inspect / Apply / Rollback)

Leaf crate that reads and mutates OS proxy configuration, powering
`eggress system-proxy inspect` and the pproxy-compatible `--sys` flag.
Platform-classified: unsupported combinations fail with structured
diagnostics. Commands are structured (`Command { program, args }`), never
shell-style strings, preserving names with spaces.

## Module map

| File | Role |
|------|------|
| `src/apply.rs` | `plan_apply()` (dry-run), `ApplyPlan`, `apply_compatibility_proxy[_with_runner]()`, `AppliedProxy` (RAII rollback), `RollbackState`, `Command`, `CompatibilityProxyKind`, `create_rollback`, `execute_apply`, `generate_revert_commands` |
| `src/capability.rs` | `SystemProxyCapability` (9 variants), `SystemProxyStatus`, `check_system_proxy_capability[_with_overrides]()`, `system_proxy_platform_info()` |
| `src/backends/macos.rs` | `networksetup` inspect/apply/disable; `list_network_services`, `inspect_macos_proxy`, `generate_macos_apply_commands`, `generate_macos_disable_commands` |
| `src/backends/linux.rs` | GNOME `gsettings` inspect/apply/disable; `inspect_gnome_proxy`, `generate_gnome_apply_commands`, `generate_gnome_disable_commands` |
| `src/backends/windows.rs` | Windows registry inspect/apply/disable; `inspect_windows_proxy`, `generate_windows_apply_commands`, `generate_windows_disable_commands` |
| `src/backends/env.rs` | `HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY`/`NO_PROXY` env-var fallback; `inspect_environment`, `generate_env_exports` |
| `src/command_runner.rs` | `CommandRunner` trait + `RealCommandRunner` + `MockCommandRunner` |
| `src/inspection.rs` | `inspect_system_proxy[_with_runner]()`, `detect_platform`, `InspectionResult`, `SystemProxySettings` |
| `src/redaction.rs` | `redact_proxy_uri`, `redact_proxy_settings` — strips credentials |

## Public API

| Symbol | Signature |
|--------|-----------|
| `plan_apply` | `(platform, service, http, https, socks, no_proxy, current_settings) -> ApplyPlan` |
| `apply_compatibility_proxy` | `(kind, address) -> Result<AppliedProxy, String>` |
| `apply_compatibility_proxy_with_runner` | Same with `&dyn CommandRunner` |
| `AppliedProxy::restore` | `(&mut self) -> Result<(), String>` — idempotent |
| `inspect_system_proxy` | `() -> InspectionResult` |
| `inspect_system_proxy_with_runner` | Same with `&dyn CommandRunner` |
| `check_system_proxy_capability` | `(cap) -> SystemProxyStatus` |

## Inspect path

1. `detect_platform()` returns `"macos"`, `"linux"`, `"windows"`, or
   `"unknown"`.
2. Platform-specific best-effort inspection (macOS: `networksetup` via first
   network service; Linux: `gsettings`; Windows: `reg query`; fallback: env).
3. `InspectionResult` includes `apply_supported: bool` (any `Apply*`
   capability reports `Available`).

## Apply-plan flow

`apply_compatibility_proxy` (`src/apply.rs:113-161`):

```
detect_platform()
  -> inspect_system_proxy_with_runner(runner)
  -> plan_apply(platform, ..., Some(&settings))     // dry-run
  -> create_rollback(platform, service, &settings)
  -> execute_apply(&plan, runner)
  -> on failure: restore_with_runner(&rollback)      // immediate revert
  -> AppliedProxy { rollback: Some(state) }
```

`AppliedProxy` is RAII: `Drop` calls `restore()` (idempotent).
`execute_apply` runs each `Command` sequentially; on failure, rollback
restores the full previous state.

`generate_revert_commands` builds the inverse sequence per platform:
- macOS: disable all proxies, re-enable with previous values.
- Windows: disable `ProxyEnable`, re-add `ProxyServer`/`ProxyEnable=1`.
- Linux: disable GNOME proxy, re-apply previous `gsettings` values.

## CommandRunner injection

| Impl | Use |
|------|-----|
| `RealCommandRunner` | Production: `Command::new(program).args(args).output()` |
| `MockCommandRunner` | Tests: pre-programmed responses, records all calls |

`MockCommandRunner`: `add_response(program, args, result)`,
`add_always(program, result)`, `calls()` for assertion. Tests never shell
out.

## Capability checks

9 `SystemProxyCapability` variants, each checked via `cfg(target_os)` or
`which <tool>` on Unix:

| Capability | Check |
|-----------|-------|
| `InspectEnvironment` | Always `Available` |
| `Inspect/ApplyMacosNetworksetup` | `which networksetup` |
| `Inspect/ApplyWindowsInternetSettings` | `cfg!(target_os = "windows")` |
| `Inspect/ApplyGnomeSettings` | `which gsettings` |
| `Inspect/ApplyKdeSettings` | `which kwriteconfig5` |

`SystemProxyStatus`: `Available`, `MissingPrivilege`, `UnsupportedPlatform`,
`ToolMissing`, `DisabledAtCompileTime`.

## Backend details

**macOS** (`backends/macos.rs`): `networksetup -listallnetworkservices` ->
first service -> `-getwebproxy`/`-getsecurewebproxy`/`-getsocksfirewallproxy`.
Apply: `-setwebproxy on`, `-setwebproxyservers`, etc. Default service:
`*Wi-Fi`. No-proxy: `-setwebproxybypassdomains`.

**Linux** (`backends/linux.rs`): `gsettings get org.gnome.system.proxy
mode`; if `manual`, read host/port per protocol and `ignore-hosts`.
Apply: set mode `manual`, set host/port, set `ignore-hosts` (GVariant
string array with single-quote escaping). Disable: set mode `none`.

**Windows** (`backends/windows.rs`): `reg query HKCU\...\Internet
Settings` for `ProxyEnable`, `ProxyServer`, `ProxyOverride`. Parses
`http=addr;https=addr;socks=addr`; bare address applies to all protocols.
Apply: `reg add /f`. Disable: `ProxyEnable /d 0 /f`.

**Environment** (`backends/env.rs`): reads `HTTP_PROXY`, `HTTPS_PROXY`,
`ALL_PROXY`, `NO_PROXY` (+ lowercase). Always available; fallback.

## Redaction

`redact_proxy_uri` is shared with `eggress-uri` and masks the complete
userinfo as `****`, correctly handling bracketed IPv6 endpoints and `@` in
passwords.
`redact_proxy_settings` applies this to all values with "proxy" in the key.
Applied in `inspect_system_proxy_with_runner` for safe logging.

## Error model

- `apply_compatibility_proxy` returns `Result<AppliedProxy, String>`.
- `execute_apply` short-circuits on first failure; error includes the
  command and its error.
- Rollback failure reported alongside: `"{error}; rollback failed: {msg}"`.
- `plan_apply` always succeeds (may produce zero commands for unsupported
  platforms).
- No feature gates — all backends compile via `cfg(target_os)` guards.

## Security notes

- Credentials stripped by `redact_proxy_uri` before logging.
- Commands use structured `Command { program, args }` — no shell injection.
- `--sys` applies settings before accept loops start; failure is startup
  error.
- `RollbackState` JSON persistence is opt-in; `AppliedProxy` RAII guard is
  primary.

## Concurrency

- `AppliedProxy` is per-process, not `Send`/`Sync`.
- `MockCommandRunner` uses `Mutex<Vec<...>>` for call recording.
- All `CommandRunner::run` calls are synchronous.

## Test coverage

49 tests via `cargo test -p eggress-system-proxy --lib`:

| Module | Key tests |
|--------|-----------|
| `apply.rs` | `plan_apply_*_produces_commands`, `execute_apply_preserves_spaces_in_args`, `rollback_state_save_and_load`, `revert_commands_macos`, `applied_proxy_restore_is_idempotent` |
| `command_runner.rs` | `mock_runner_returns_predefined_response`, `mock_runner_records_calls`, `real_runner_executes_command` |
| `capability.rs` | `display_*`, `env_inspection_always_available`, `override_*`, `platform_info_returns_all_capabilities` |
| `inspection.rs` | `detect_platform_*`, `inspection_result_serializes`, `inspection_with_mock_runner` |
| `redaction.rs` | `redact_uri_with_credentials`, `redact_uri_without_credentials`, `redact_settings_map` |
| `backends/linux.rs` | `inspect_gnome_manual_mode`, `generate_gnome_apply_commands_*`, `parse_proxy_address_*`, `parse_gsettings_*` |
| `backends/macos.rs` | `list_services_parses_output`, `inspect_proxy_parses_web_proxy`, `generate_apply_commands`, `generate_disable_commands` |
| `backends/windows.rs` | `generate_apply_commands_produces_reg_commands`, `generate_disable_commands` |
| `backends/env.rs` | `inspect_environment_reads_env_vars`, `generate_exports_from_settings` |

## Reviewer gotchas

- `plan_apply` always succeeds; execution failure is handled separately.
- `AppliedProxy::restore` is idempotent; `Drop` also restores.
- macOS defaults to service `"*Wi-Fi"`.
- Windows bare proxy address applies to both HTTP and HTTPS.
- `gsettings` output uses GVariant single-quote strings; parser strips
  surrounding quotes and handles escaped quotes in `ignore-hosts`.
- `redact_proxy_uri` uses `rfind('@')` to handle nested `://` correctly.

## See also

- [overview.md](overview.md)
- [runtime.md](runtime.md)
- [cli.md](cli.md)
- [config.md](config.md)
