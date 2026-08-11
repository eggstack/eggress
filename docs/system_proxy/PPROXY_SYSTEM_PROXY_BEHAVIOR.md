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

Eggress intentionally diverges from pproxy's hidden global mutation:

1. **No automatic mutation**: System proxy settings are never changed during normal operation.
2. **Read-only inspection**: `eggress system-proxy inspect` reads settings without modification.
3. **No native mutation command**: The native CLI does not expose a public
   command for changing or reverting system proxy state.
4. **Rust library capability**: `eggress-system-proxy` retains planning,
   platform command-generation, and rollback-state primitives for Rust callers.
   These are not native CLI features.

## Classification

| Feature | pproxy | Eggress | Status |
|---------|--------|---------|--------|
| `--sys` flag | Global mutation | Compatibility mode refuses it | **Intentional non-parity** |
| System proxy inspection | Via `--sys` | `eggress system-proxy inspect` | **Supported** |
| System proxy mutation command | Implicit through `--sys` | No public native CLI command | **Intentional non-parity** |
| Planning and rollback primitives | Internal to pproxy cleanup | Rust library API only | **Library capability** |
