# System Proxy Integration

System proxy inspection and configuration helpers for Eggress.

## Overview

Eggress provides **read-only system proxy inspection** and **explicit dry-run apply** capabilities. System proxy mutation is operationally risky and never happens automatically.

## Design Principles

1. **No hidden global mutation**: System proxy settings are never changed during normal `eggress run` or `eggress pproxy run`.
2. **Read-only by default**: Inspection is safe and requires no elevated privileges.
3. **Explicit apply**: Any proxy mutation requires explicit `--apply` flag and supports `--dry-run`.
4. **Rollback support**: Apply saves previous settings to a rollback file for revert.
5. **Credential redaction**: Passwords are stripped from all output and logs.

## CLI Usage

### Inspect current settings

```bash
# Human-readable output
eggress system-proxy inspect

# JSON output
eggress system-proxy inspect --json
```

### Apply proxy settings (dry-run)

```bash
# Show what commands would be run (default)
eggress system-proxy apply --http http://127.0.0.1:8080 --https http://127.0.0.1:8443

# Actually apply (future phase)
eggress system-proxy apply --http http://127.0.0.1:8080 --apply
```

### Revert proxy settings

```bash
eggress system-proxy revert ~/.cache/eggress/proxy.rollback.json
```

## Platform Support

| Platform | Inspection | Apply | Notes |
|----------|-----------|-------|-------|
| macOS | `networksetup` | Dry-run commands | Uses first network service |
| Windows | Registry (Internet Settings) | Dry-run commands | `HKCU\...\Internet Settings` |
| Linux | `gsettings` (GNOME) | Dry-run commands | GNOME desktop environment |
| All | Environment variables | Shell exports | `HTTP_PROXY`, `HTTPS_PROXY`, etc. |

## Architecture

- **`eggress-system-proxy`** crate provides the core library
- **`CommandRunner`** trait enables testable command execution
- **Capability model** detects platform support at runtime
- **Credential redaction** in `redaction` module

## See Also

- [pproxy system proxy behavior](PPROXY_SYSTEM_PROXY_BEHAVIOR.md)
- [macOS networksetup](MACOS_NETWORKSETUP.md)
- [Windows proxy settings](WINDOWS_PROXY_SETTINGS.md)
- [Linux desktop proxy](LINUX_DESKTOP_PROXY.md)
