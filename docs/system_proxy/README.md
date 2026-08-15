# System Proxy Integration

System proxy inspection and library-side configuration helpers for Eggress.

## Overview

Eggress provides read-only system proxy inspection through the native CLI. The
`eggress-system-proxy` Rust crate also contains planning, command-generation,
and rollback-state primitives for library integrations. Native `eggress run`
remains read-only; the pproxy compatibility executable explicitly opts into
apply/rollback behavior with `--sys`.

## Design Principles

1. **No hidden global mutation**: System proxy settings are never changed during normal `eggress run`; compatibility `eggress pproxy run --sys` is the explicit exception and always keeps rollback state.
2. **Read-only by default**: Inspection is safe and requires no elevated privileges.
3. **Read-only native CLI**: `eggress system-proxy inspect` is the only public
   native system-proxy command.
4. **Explicit compatibility mutation**: Planning, command execution, and
   rollback-state handling are used by `pproxy --sys`; native CLI mutation
   commands remain unavailable.
5. **Credential redaction**: Passwords are stripped from all output and logs.

## CLI Usage

### Inspect current settings

```bash
# Human-readable output
eggress system-proxy inspect

# JSON output
eggress system-proxy inspect --json
```

### Rust library capabilities

The `eggress-system-proxy` crate exposes apply planning, platform command
construction, and rollback-state primitives as Rust APIs. The native CLI
remains read-only, while compatibility `pproxy --sys` uses these APIs to apply
the selected bound listener and restore prior settings on shutdown.

## Platform Support

| Platform | Native CLI inspection | Rust library capability | Notes |
|----------|-----------|-------|-------|
| macOS | `networksetup` | Planning and command construction | Uses first network service |
| Windows | Registry (Internet Settings) | Planning and command construction | `HKCU\...\Internet Settings` |
| Linux | `gsettings` (GNOME) | Planning and command construction | GNOME desktop environment |
| All | Environment variables | Platform-specific library helpers | `HTTP_PROXY`, `HTTPS_PROXY`, etc. |

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
