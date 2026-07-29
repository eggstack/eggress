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

## Command Runner

| Type | Description |
|---|---|
| `CommandRunner` | Trait for executing system commands |
| `RealCommandRunner` | Production implementation |
| `MockCommandRunner` | Test implementation |

## Dependencies

None — standalone crate with no workspace dependencies.

See [overview.md](overview.md) for context.
