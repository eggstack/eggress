# pproxy System Proxy Behavior

## Overview

This document captures how `pproxy==2.7.9` handles system proxy operations, for comparison with Eggress behavior.

## pproxy `--sys` Flag

pproxy exposes a `--sys` command-line flag that configures the system-wide proxy settings on the current platform.

### Behavior

- **Mutates global state**: When `--sys` is used, pproxy modifies OS-level proxy settings.
- **Platform-specific**: Uses different mechanisms per platform:
  - macOS: `networksetup` commands
  - Windows: Registry (Internet Settings)
  - Linux: Environment variables or desktop environment settings
- **Cleanup**: pproxy attempts to restore previous settings on shutdown, but this is best-effort.
- **Privileges**: May require elevated privileges on some platforms.

### Risks

- Can break network connectivity if proxy is unreachable
- May leak traffic through wrong proxy
- Global state mutation affects all applications
- Cleanup failure leaves proxy settings modified

## Eggress Divergence

Eggress preserves the compatibility mutation boundary while making lifecycle
behavior explicit:

1. **Compatibility-only mutation**: pproxy `--sys` applies only after its
   listener binds successfully and selects the actual local SOCKS5/HTTP port.
2. **Lifecycle-safe rollback**: Prior settings are captured in memory and
   restored on normal shutdown, signal handling, or a later startup failure.
3. **Native separation**: Native mode does not mutate system proxy settings;
   `eggress system-proxy inspect` remains read-only.
4. **Structured commands**: `eggress-system-proxy` passes program and argument
   vectors directly to the command runner and uses `MockCommandRunner` in tests.

## Classification

| Feature | pproxy | Eggress | Status |
|---------|--------|---------|--------|
| `--sys` flag | Global mutation | Compatibility mode applies and rolls back through existing backend | **Supported with warning** |
| System proxy inspection | Via `--sys` | `eggress system-proxy inspect` | **Supported** |
| System proxy mutation command | Implicit through `--sys` | Compatibility-only; no implicit native mutation | **Supported with warning** |
| Planning and rollback primitives | Internal to pproxy cleanup | Rust library API only | **Library capability** |
