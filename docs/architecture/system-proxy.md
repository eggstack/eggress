# eggress-system-proxy

`crates/eggress-system-proxy/`

System-level proxy configuration inspection, application, and rollback.

## Key Types

| Type | Description |
|---|---|
| `SystemProxySettings` | Current system proxy configuration |
| `SystemProxyCapability` | Platform capabilities |
| `SystemProxyCapabilityReport` | Full capability report |
| `SystemProxyStatus` | Current status |
| `InspectionResult` | Result of proxy inspection |
| `ApplyPlan` | Plan to apply proxy settings |
| `AppliedProxy` | In-memory, idempotent rollback guard |
| `Command` | System command to execute |
| `RollbackState` | State for rolling back changes |

## Platform Support

| Platform | Method |
|---|---|
| Linux (GNOME) | `gsettings` / `dconf` |
| Linux (KDE) | `kwriteconfig5` |
| macOS | `networksetup` |
| Environment | `HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY` variables |

## Operations

| Operation | Description |
|---|---|
| `inspect_system_proxy()` | Read current system proxy settings |
| `check_system_proxy_capability()` | Detect platform capabilities |
| `plan_apply()` | Build an apply plan with rollback state |
| `apply_compatibility_proxy()` | Apply a bound localhost HTTP/SOCKS5 listener and retain rollback state |
| `AppliedProxy::restore()` | Restore captured settings; safe to call more than once |

## Command Runner

| Type | Description |
|---|---|
| `CommandRunner` | Trait for executing system commands |
| `RealCommandRunner` | Production implementation |
| `MockCommandRunner` | Test implementation |

## Dependencies

None — standalone crate with no workspace dependencies.

The pproxy compatibility runtime calls `apply_compatibility_proxy()` only
after all configured listeners bind successfully. It prefers a usable local
SOCKS5 listener and otherwise selects HTTP, captures the prior settings, and
restores them on normal shutdown, signal handling, or a later startup error.
Native `eggress system-proxy` commands retain their explicit semantics.

Apply and rollback pass structured program/argument vectors to the command
runner; shell-string execution and credential-bearing logs are not allowed.
Unit tests inject `MockCommandRunner`.

See [overview.md](overview.md) for context.
