# Phase 25 Plan: Transparent Proxy, redir/PF, and Unix Socket Parity

## Purpose

Phase 25 implements the platform-facing pproxy compatibility surface that is still missing after the HTTP/SOCKS/UDP/Shadowsocks evidence work: transparent proxying, Linux redirect/original-destination recovery, macOS PF integration boundaries, and Unix domain socket listeners.

This phase is inherently platform-specific and privilege-sensitive. The goal is to add the capability without making normal Eggress operation require elevated privileges and without weakening the current evidence discipline.

## Scope

This phase covers:

- Linux transparent redirected TCP listeners using original destination recovery.
- Linux IPv4 and IPv6 behavior where supported by the OS APIs.
- Explicit startup diagnostics for missing capabilities, unsupported platforms, and invalid listener combinations.
- macOS PF compatibility investigation and, if practical, implementation behind a platform gate.
- Unix domain socket listener support for local stream proxying.
- pproxy URI/CLI classification and migration docs for `redir://`, `pf://`, and Unix socket forms.
- Differential or platform integration evidence where practical.

## Non-goals

Do not implement TPROXY unless it is required for the minimum pproxy-compatible transparent workflow. If TPROXY is investigated, capture the decision and defer to a later phase unless the implementation is small and low-risk.

Do not implement system proxy setting integration in this phase. That is a separate platform convenience phase.

Do not implement UDP transparent proxying unless pproxy evidence proves a compatible behavior that can be implemented safely in this phase.

Do not require root/admin privileges for normal HTTP/SOCKS operation.

## Current expected gaps

The repo currently treats transparent proxying, `redir://`, PF, and Unix sockets as missing or intentionally non-parity in older docs. Phase 25 should replace that coarse classification with precise status:

- implemented and compatible;
- implemented and supported but not pproxy-compatible;
- intentionally unsupported with rationale;
- unsupported but planned.

## Work items

### 25.1 Capture pproxy transparent and Unix behavior

Use the pproxy oracle and local black-box probes to capture target behavior before implementation.

Capture:

- accepted URI schemes: `redir://`, `pf://`, `unix://`, and any aliases in pproxy 2.7.9;
- CLI examples involving `-l redir://...`, `-l pf://...`, Unix sockets, and combinations with `-r`;
- whether transparent modes are listener-only, upstream-only, or both;
- expected behavior on unsupported platforms;
- expected diagnostics when not running with sufficient privileges;
- IPv4 behavior;
- IPv6 behavior;
- behavior with domain targets, if any;
- interaction with routing and upstream chains;
- process exit codes for invalid setups.

Document results in:

```text
docs/PPROXY_PARITY_SPEC.md
docs/protocols/TRANSPARENT_PROXY.md
docs/protocols/UNIX_SOCKETS.md
```

Update `tests/compat/pproxy_manifest.toml` with captured but not yet implemented entries before implementation starts.

### 25.2 Add platform capability model

Create an explicit model for privileged/platform features.

Suggested crate/module locations:

```text
crates/eggress-runtime/src/platform.rs
crates/eggress-server/src/listener/transparent.rs
crates/eggress-server/src/listener/unix.rs
```

Model:

```rust
pub enum PlatformCapability {
    LinuxOriginalDstIpv4,
    LinuxOriginalDstIpv6,
    LinuxTransparentBind,
    MacosPfOriginalDst,
    UnixDomainSockets,
}

pub enum CapabilityStatus {
    Available,
    MissingPrivilege,
    UnsupportedPlatform,
    KernelUnsupported,
    DisabledAtCompileTime,
}
```

Requirements:

- capability checks must be explicit;
- startup errors must name the missing capability;
- normal listeners must not perform privileged checks;
- unsupported platforms must fail deterministically with useful diagnostics;
- tests should be able to mock capability state.

### 25.3 Linux original destination recovery

Implement Linux original-destination recovery for redirected TCP sockets.

Likely APIs:

- `SO_ORIGINAL_DST` for IPv4 on Linux netfilter REDIRECT;
- IPv6 equivalent if available and practical;
- `getsockopt` through a small internal platform module.

Requirements:

- isolate all OS-specific code behind `cfg(target_os = "linux")`;
- preserve workspace `unsafe_code = "forbid"` if possible by using safe wrappers/dependencies;
- if unsafe is unavoidable, add an ADR and narrow exception rather than weakening workspace policy silently;
- return typed errors for missing original destination;
- support local test shims/mocks without requiring iptables in normal tests;
- document required iptables/nftables commands separately.

Security notes:

- never trust original destination blindly when policy says to reject private networks;
- route original destination through the normal routing engine;
- preserve source identity in route request;
- ensure loop prevention applies to transparent targets.

### 25.4 Transparent TCP listener integration

Add an explicit transparent listener mode.

Configuration sketch:

```toml
[[listeners]]
id = "redir-local"
bind = "127.0.0.1:12345"
protocol = "redir"
mode = "transparent_tcp"
```

Runtime behavior:

1. accept TCP connection;
2. recover original destination;
3. construct route request using original target;
4. connect direct or through selected upstream chain;
5. relay bidirectionally with existing TCP relay machinery;
6. expose metrics and structured diagnostics.

Requirements:

- original target is visible in route explanation;
- listener metrics distinguish transparent accepted/rejected/original-dst-failed;
- startup validates protocol/mode combinations;
- transparent listener can be disabled in unprivileged builds/configs.

### 25.5 Linux integration tests

Normal CI cannot be assumed to have root or iptables. Use a two-tier test strategy.

Ungated tests:

- URI parsing for redir schemes;
- config translation;
- capability model behavior;
- mocked original destination recovery;
- route request construction;
- startup diagnostics for unsupported platform/missing capability;
- relay path using injected original destination.

Gated tests:

- Linux-only, root/capability-required transparent redirect smoke;
- create temporary loopback listener;
- configure iptables or nftables REDIRECT rule;
- connect to target and verify Eggress routes original destination;
- cleanup rules on success/failure.

Gate with:

```text
EGRESS_REQUIRE_TRANSPARENT_INTEROP=1
```

Add docs for running these tests manually.

### 25.6 macOS PF investigation and implementation decision

pproxy exposes PF support. Capture what pproxy actually does on macOS and determine whether Eggress should implement it now.

Tasks:

- document pproxy PF syntax and expected behavior;
- identify whether original destination recovery is practical from user-space in current macOS versions;
- decide implementation versus intentional non-parity;
- if implemented, keep it behind `cfg(target_os = "macos")` and explicit runtime capability checks;
- if not implemented, reject with pproxy-shaped diagnostics and a clear rationale.

Output:

```text
docs/adr/ADR_macos_pf_transparent_proxy.md
```

Acceptance requires either a working implementation or a documented non-parity decision.

### 25.7 Unix domain socket listener support

Implement Unix domain socket listeners for local stream proxying.

Requirements:

- `cfg(unix)` only;
- support listener path cleanup behavior with explicit config;
- protect against accidentally unlinking arbitrary files;
- configurable file mode/permissions where practical;
- support direct fixed target or route-resolved target depending on pproxy behavior;
- support HTTP/SOCKS protocol handling over Unix socket if pproxy does;
- graceful shutdown removes socket only when Eggress created it;
- Windows returns unsupported diagnostics.

Suggested config:

```toml
[[listeners]]
id = "local-socks-unix"
path = "/tmp/eggress.sock"
protocol = "socks5"
transport = "unix"
unlink_existing = false
```

Tests:

- bind Unix socket;
- connect client over Unix socket;
- run SOCKS5 CONNECT over Unix stream;
- run HTTP CONNECT over Unix stream if supported;
- cleanup behavior;
- reject existing file unless explicit unlink enabled;
- Windows/unsupported platform compile behavior.

### 25.8 URI and CLI compatibility

Extend `eggress-pproxy-compat` URI translation and diagnostics.

Tasks:

- parse pproxy transparent and Unix socket URI forms;
- translate supported forms to TOML;
- reject unsupported forms with structured errors;
- update `eggress pproxy check` tier output;
- update Python translation helpers if they expose pproxy URI translation;
- add redaction tests if any URI can contain credentials.

Acceptance:

- `eggress pproxy translate -l redir://...` produces transparent config or clear unsupported diagnostics;
- Unix listener forms produce config on Unix platforms;
- invalid platform-specific forms fail with platform-aware diagnostics.

### 25.9 Metrics and observability

Add metrics for platform listeners.

Suggested metrics:

- transparent connections accepted;
- original destination lookup success/failure;
- transparent route rejects;
- transparent relay failures;
- Unix listener accepted connections;
- Unix listener bind failures;
- platform capability check failures.

Admin/status should show:

- listener mode;
- capability status;
- original destination support state;
- Unix socket path and ownership behavior, without exposing sensitive paths in redaction-sensitive contexts if configured.

### 25.10 Documentation and manifest updates

Update:

- `docs/PARITY_MATRIX.md`;
- `docs/COMPATIBILITY_EVIDENCE.md`;
- `docs/PPROXY_PARITY_SPEC.md`;
- `docs/PPROXY_MIGRATION.md`;
- `docs/CONFIG_REFERENCE.md`;
- `docs/OPERATIONS.md`;
- `docs/SECURITY_REVIEW.md`;
- `tests/compat/pproxy_manifest.toml`;
- README capability table.

The docs must state which transparent features require elevated privileges and which tests are gated/manual-only.

## Validation commands

Baseline:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-testkit manifest
cargo test -p eggress-pproxy-compat redir
cargo test -p eggress-pproxy-compat unix
```

Linux gated transparent tests, if implemented:

```bash
sudo -E EGRESS_REQUIRE_TRANSPARENT_INTEROP=1 cargo test -p eggress-runtime transparent -- --ignored --test-threads=1
```

Unix socket tests:

```bash
cargo test -p eggress-runtime unix_socket
cargo test -p eggress-server unix_socket
```

## Acceptance criteria

Phase 25 is complete when:

- pproxy transparent/PF/Unix behavior has been captured.
- Linux transparent TCP listener support exists or has a documented final deferral.
- Original destination recovery is platform-gated and tested by mocks plus optional gated integration tests.
- macOS PF has an ADR and either implementation or explicit non-parity.
- Unix domain socket listeners work on Unix platforms or are explicitly deferred with rationale.
- pproxy URI/CLI translation reflects actual support.
- Manifest, parity matrix, compatibility evidence, README, and migration docs agree.
- No normal proxy workflow requires elevated privileges.

## Remaining expected gaps after this phase

- SSH upstream transport.
- HTTP/2, HTTP/3, QUIC, WebSocket, raw tunnel, reverse/backward proxying.
- Trojan server/listener if not handled separately.
- System proxy configuration.
- True pproxy-shaped Python API drop-in replacement.

## Handoff notes

The key risk is privilege creep. Treat transparent proxying as a platform capability behind explicit listener modes, not as a global runtime behavior. The second risk is documentation overclaiming: Linux REDIRECT, macOS PF, Unix sockets, and TPROXY are separate surfaces and must be classified separately.
