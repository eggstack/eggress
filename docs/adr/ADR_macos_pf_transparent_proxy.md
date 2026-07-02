# ADR: macOS PF Transparent Proxy

| Field | Value |
|-------|-------|
| Status | Accepted (Intentional Non-Parity) |
| Date | Phase 25 |
| Decision makers | Eggress maintainers |
| Related | `crates/eggress-server/src/listener/transparent.rs`, `crates/eggress-runtime/src/platform.rs` |

## Context

pproxy exposes PF (Packet Filter) support on macOS for transparent proxying.
On macOS, PF can redirect traffic to a local proxy using a divert socket.
However, this requires:

1. Root privileges to modify PF rules
2. PF kernel support (available on macOS via `/dev/pf`)
3. Divert socket creation (macOS-specific API, not available in standard Rust)

eggress defines `PlatformCapability::MacosPfOriginalDst` to expose the
platform capability surface, but does not implement actual PF rule injection
or divert socket handling. After Phase 25-28 hardening the capability check
returns `KernelUnsupported` on macOS (and `UnsupportedPlatform` elsewhere)
to honestly reflect that no PF integration exists, rather than reporting
`Available` based on `/dev/pf` existence.

## Decision

**Eggress will NOT implement macOS PF transparent proxy in this phase.**

The reasons are:

1. **Privilege requirement**: Normal proxy operation should not require root.
   PF rule management (`pfctl`) and divert socket creation both need elevated
   privileges, which conflicts with eggress's design goal of running as a
   non-privileged service.

2. **Complexity**: PF rule management adds significant platform-specific code
   including `pfctl` invocation, rule template generation, divert socket
   lifecycle management, and cleanup on shutdown. This is a substantial
   platform-specific surface area for limited benefit.

3. **Limited use case**: Most transparent proxy deployments target Linux
   servers using iptables/nftables REDIRECT or TPROXY. macOS transparent
   proxy is primarily useful for development and testing scenarios.

4. **Alternative available**: Users can configure PF rules externally via
   `pfctl` and redirect traffic to a standard HTTP or SOCKS listener bound
   on `127.0.0.1`. This achieves the same functional outcome without
   requiring eggress to manage PF state.

## Consequences

### Positive

- **No root requirement**: eggress continues to run without elevated
  privileges on all platforms.
- **Reduced platform surface**: No macOS-specific PF code to maintain, test,
  or audit.
- **Clear diagnostics**: `redir://` on macOS produces a diagnostic message
  explaining the limitation and suggesting the `pfctl` workaround.
- **Capability model honest**: `PlatformCapability::MacosPfOriginalDst`
  returns `KernelUnsupported` on macOS to make clear that eggress does not
  perform PF-based original-destination recovery. Operators are not led to
  believe the platform check implies any usable behavior.
- **Easy future extension**: The platform capability model and transparent
  listener abstraction make it straightforward to add PF support later if
  user demand warrants.

### Negative

- **pproxy compatibility gap on macOS**: Users who rely on pproxy's PF-based
  transparent proxy cannot use eggress as a drop-in replacement on macOS.
- **Manual PF setup required**: Users who want transparent proxy on macOS
  must configure PF rules and `pfctl` redirection externally.

### Neutral

- **`redir://` on macOS**: Translates to a standard listener with
  `transparent.enabled = true`. The supervisor logs a diagnostic but does
  not fail startup. `get_original_destination()` returns
  `UnsupportedPlatform` on non-Linux, so transparent accept loops will
  report errors.

## Alternatives Considered

### 1. Full PF Implementation

Implement PF rule generation, `pfctl` invocation, and divert socket handling
directly within eggress.

**Rejected because**: Requires root for both rule injection and divert socket
creation. Adds ~500 lines of macOS-specific code including unsafe FFI for
divert sockets. Maintenance burden outweighs the limited use case.

### 2. PF Rule Generation Only

Generate `pfctl` commands that users can run externally, but don't execute
them automatically.

**Rejected because**: This is functionally equivalent to the current
workaround (users run `pfctl` themselves) but adds code complexity for
marginal UX improvement.

### 3. Divert Socket Approach

Use macOS `socket()` with `AF_DIVERT` to intercept redirected traffic.

**Rejected because**: `AF_DIVERT` is not exposed in the `libc` crate and
requires unsafe raw FFI. The API is not stable across macOS versions. Root
privileges are still required.

## References

- `crates/eggress-server/src/listener/transparent.rs` — TransparentListener
  and `get_original_destination()` (returns `UnsupportedPlatform` on non-Linux)
- `crates/eggress-runtime/src/platform.rs` — `MacosPfOriginalDst` capability
  check (intentionally reports `KernelUnsupported` to reflect no integration)
- `crates/eggress-pproxy-compat/src/diagnose.rs` — Diagnostic messages for
  unsupported transparent proxy configurations
- `docs/PPROXY_PARITY_SPEC.md` — Tier taxonomy and intentional non-parity
  decisions
