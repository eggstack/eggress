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
3. **Explicit apply**: Any mutation requires explicit `--apply` flag.
4. **Dry-run support**: All apply commands can be previewed without execution.
5. **Rollback state**: Previous settings are saved to a file before any mutation.

## Classification

| Feature | pproxy | Eggress | Status |
|---------|--------|---------|--------|
| `--sys` flag | Global mutation | Not implemented | **Intentional non-parity** |
| System proxy inspection | Via `--sys` | `eggress system-proxy inspect` | **Supported** |
| Dry-run apply | Not available | `--dry-run` flag | **Supported** |
| Rollback state | Best-effort cleanup | Explicit rollback file | **Supported** |
