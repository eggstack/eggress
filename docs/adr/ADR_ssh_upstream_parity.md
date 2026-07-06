# ADR: SSH Upstream Parity — Intentional Non-Parity

| Field | Value |
|-------|-------|
| Status | Accepted |
| Date | Phase 47 |
| Decision makers | Eggress maintainers |
| Related | `docs/parity/pproxy_capability_manifest.toml`, `docs/PARITY_MATRIX.md`, `docs/PPROXY_PARITY_SPEC.md`, `plans/phase_47_ssh_upstream_parity_decision.md` |

## Context

pproxy 2.7.9 supports SSH upstream transport (`ssh://user:pass@host:22`) via
`direct-tcpip` channel forwarding. SSH is one of the largest remaining
differences between eggress and pproxy.

eggress currently:
- Recognizes `ssh://` URIs at parse time with default port 22
- Rejects SSH upstreams and listeners with `UnsupportedProtocol` diagnostics
- Has no SSH protocol crate implementation
- Classifies SSH as `unsupported` in the parity manifest

Phase 47 evaluated whether to implement SSH upstream transport or classify it
as intentional non-parity.

## Decision

**SSH upstream transport is classified as intentional non-parity.**

eggress will not implement SSH as a proxy transport. The rationale is documented
below. SSH URIs continue to be recognized at parse time for clean diagnostics,
but are rejected with an actionable recommendation.

## Rationale

### SSH Is Not a Proxy Protocol

SSH is a general-purpose encrypted remote access protocol. Using it as a proxy
transport via `direct-tcpip` channel forwarding is a secondary use case that
requires:

- SSH client library dependency (significant weight)
- Host-key verification and known-hosts policy management
- Authentication method support (password, key, agent)
- `direct-tcpip` channel lifecycle management
- Keepalive and reconnect semantics
- Flow control and backpressure across the SSH channel
- Security documentation for the host-key trust model

This is an entire SSH client implementation, not a protocol handler.

### No Acceptable Pure-Rust SSH Library

The project's design constraints (`unsafe_code = "forbid"`, no C dependencies,
no OpenSSL) eliminate the most mature SSH libraries:

| Library | Status | Constraint conflict |
|---------|--------|-------------------|
| `russh` | Active, async | Depends on `ring` (acceptable) but significant API surface |
| `thrussh` | Deprecated fork of russh | Maintenance concern |
| `libssh-rs` | Bindings to libssh C library | C dependency, violates no-C-deps policy |
| `ssh2` (libssh2 bindings) | Bindings to libssh2 C library | C dependency, violates no-C-deps policy |

`russh` is the only viable pure-Rust option, but it adds:
- A full SSH transport implementation (~15k lines)
- Host-key verification logic requiring ongoing security review
- Multiple authentication method implementations
- Channel multiplexing and flow control
- Cross-platform terminal and signal handling considerations

The maintenance burden of owning an SSH client implementation is
disproportionate to the proxy use case it serves.

### pproxy's SSH Support Is Limited

pproxy's SSH upstream support:
- Uses `paramiko` (Python SSH library) for `direct-tcpip` forwarding
- Is not documented as a primary feature
- Has no differential test evidence for SSH-specific behavior
- Adds significant dependency weight to pproxy itself

### Users Have Better Alternatives

For encrypted tunneling through an SSH server, users should:
- Use OpenSSH's built-in dynamic forwarding (`ssh -D`) for SOCKS proxy
- Use OpenSSH's `LocalForward` for port forwarding
- Use an SSH client directly for remote access
- Chain eggress through an SSH tunnel using platform tools

These alternatives are more robust, better maintained, and have broader
community support than any embedded SSH proxy implementation.

### Dependency and Packaging Impact

Adding `russh` would:
- Increase compile times for all workspace builds
- Add `ring` dependency (already present via `rustls`, but `russh` uses it differently)
- Require SSH-specific test infrastructure (test SSH servers, key generation)
- Impact Python wheel builds (larger binary size, more transitive dependencies)
- Require ongoing security monitoring for SSH protocol vulnerabilities

## Consequences

### Positive

- **Reduced dependency surface**: No SSH client library to maintain, audit, or update
- **Clear user guidance**: Diagnostics recommend proven alternatives (OpenSSH forwarding)
- **Honest parity accounting**: Manifest reflects actual capability, not aspirational scope
- **Security posture**: No SSH attack surface in the proxy codebase
- **Maintained parser recognition**: SSH URIs produce clean, actionable diagnostics

### Negative

- **pproxy parity gap**: Users who rely on pproxy's `ssh://` upstream cannot use eggress for that transport
- **No embedded SSH tunneling**: Users must use external tools for SSH-based proxying

### Neutral

- **Manifest tier change**: `unsupported` → `intentional_non_parity` (metadata only)
- **Diagnostic suggestion added**: SSH diagnostics now recommend OpenSSH dynamic forwarding
- **PARITY_MATRIX.md**: Already classified SSH as "Intentional non-parity" — no change needed

## Alternatives Considered

### 1. Full SSH Implementation with `russh`

Implement SSH upstream transport using `russh` with `direct-tcpip` channel
forwarding, host-key verification, and password/key authentication.

**Rejected because**: The maintenance burden of owning an SSH client
implementation (host-key policy, authentication methods, channel lifecycle,
keepalive, reconnect, flow control) is disproportionate to the proxy use case.
The `unsafe_code = "forbid"` constraint eliminates libraries with C bindings,
leaving only `russh` — which still requires significant security-critical code.

### 2. Feature-Gated SSH Implementation

Implement SSH behind a Cargo feature flag (e.g., `ssh-support`).

**Rejected because**: A feature gate does not reduce the implementation or
maintenance burden. The security-critical code (host-key verification,
authentication) still needs to be written, tested, and maintained. The feature
gate only makes compilation optional.

### 3. Stub SSH with Passthrough

Accept `ssh://` URIs and forward raw TCP through an SSH tunnel using an
external OpenSSH process.

**Rejected because**: Spawning external processes introduces process management
complexity, platform-specific behavior, and security concerns (command
injection, environment variable leakage). This is not a sustainable proxy
transport design.

## References

- `plans/phase_47_ssh_upstream_parity_decision.md` — Phase 47 plan
- `docs/parity/pproxy_capability_manifest.toml` — Capability manifest (updated in Phase 47)
- `docs/PARITY_MATRIX.md` — Feature parity tracking
- `docs/PPROXY_PARITY_SPEC.md` — Tier taxonomy
- `docs/adr/ADR_quic_h3_pproxy_parity.md` — Similar deferral precedent for QUIC/H3
