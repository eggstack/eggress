# Phase 5 — Remaining High-Value pproxy Runtime Gaps

## Status

Proposed.

## Parent roadmap

`plans/PPROXY_PRACTICAL_PARITY_ROADMAP.md`

## Depends on

Phases 1 through 4. The parser and public Python API must already represent these features before runtime work begins.

## Objective

Close the remaining documented pproxy behaviors that provide meaningful compatibility and can be implemented by extending existing Eggress abstractions locally.

This phase is deliberately split into mandatory low-cost work and conditional bounded work. It must not become a vehicle for implementing every protocol or legacy extension that pproxy has ever accepted.

## Scope policy

A runtime gap belongs in this phase only when all of the following are true:

1. It is documented or demonstrably used in pproxy 2.7.9.
2. It is not already available through the native runtime.
3. It can reuse an existing listener, stream, datagram, transport, or platform abstraction.
4. It does not require a new cryptographic family, large third-party subsystem, or independent proxy runtime.
5. It can be verified with a small local test.

If a feature fails these tests, retain it as intentional non-parity with an accurate diagnostic.

## Explicit exclusions retained

The following remain out of scope:

- SSH transport;
- QUIC and HTTP/3;
- ShadowsocksR;
- legacy Shadowsocks stream ciphers and OTA;
- pproxy obfuscation/plugin families;
- general daemonization;
- general cross-session connection reuse;
- exhaustive replication of platform mutation behavior;
- arbitrary multi-hop UDP through every transport;
- new TLS interception or MITM behavior.

## Mandatory workstream 5.1 — HTTP-only upstream mode

### Goal

Support pproxy's HTTP-only upstream mode (`httponly` or the exact oracle scheme) using the existing HTTP forward-proxy implementation.

### Tasks

1. Confirm the exact scheme name, constructor behavior, and role from pproxy 2.7.9.
2. Add the scheme to the Phase 1 compatibility AST and role table if not already represented.
3. Map it to ordinary HTTP forwarding without CONNECT where appropriate.
4. Preserve authentication and TLS wrappers.
5. Validate request-target rewriting and hop-by-hop header handling through the existing HTTP crate.
6. Add one direct upstream test and one chained listener-to-HTTP-only-upstream test.
7. Reject UDP use.

### Acceptance criteria

- a documented pproxy HTTP-only URI translates and starts;
- GET and POST with a small body pass through the upstream;
- CONNECT is rejected or handled exactly as the oracle does;
- existing HTTP CONNECT upstream behavior is unchanged.

## Mandatory workstream 5.2 — Echo protocol

### Goal

Expose pproxy's TCP and UDP echo endpoints for local testing without introducing a general application server.

### Tasks

1. Confirm `echo://` URI roles and default behavior.
2. Implement a tiny protocol handler using existing listener and datagram abstractions.
3. For TCP, echo bytes until EOF while preserving half-close behavior.
4. For UDP, return each accepted datagram to its sender within existing packet limits.
5. Allow compatibility CLI/Python construction.
6. Keep echo disabled from unrelated native defaults.

### Acceptance criteria

- TCP and UDP echo work on loopback;
- packet and connection limits still apply;
- no new admin or HTTP framework is introduced;
- the handler is small and self-contained.

## Mandatory workstream 5.3 — Fixed-target TCP and UDP

### Goal

Support pproxy brace-target tunnel forms using existing raw/direct relay primitives.

### Tasks

1. Consume `fixed_target` from the Phase 1 AST.
2. Map fixed-target TCP to the existing raw/tunnel stream path.
3. Add fixed-target UDP using the existing UDP association/datagram relay machinery.
4. Keep one target per listener/upstream instance; do not build a general UDP routing layer.
5. Apply existing packet-size, amplification, association, and idle-time limits.
6. Support hostname and IP targets consistently with pproxy.
7. Reject fixed targets on schemes where the oracle does not allow them.

### Acceptance criteria

- `tunnel{host:port}`-style TCP relays to the fixed destination;
- the corresponding UDP form relays a local echo datagram;
- clients cannot override the configured target;
- target resolution and errors are deterministic;
- no multi-hop UDP claim is made unless the exact tested path works.

## Mandatory workstream 5.4 — Unix-domain upstreams

### Goal

Allow a Unix-domain socket to act as an upstream hop on Unix platforms where pproxy permits it.

### Tasks

1. Confirm exact URI representation from the oracle.
2. Add an upstream connector using Tokio's existing Unix stream support.
3. Integrate it with the common stream trait/enum used by chain execution.
4. Permit only TCP-like byte streams; reject UDP.
5. Preserve file-path redaction rules.
6. Add cleanup/error tests for missing and permission-denied sockets.
7. Compile-gate cleanly on Windows with a stable unsupported-platform error.

### Acceptance criteria

- a SOCKS or HTTP listener can relay through a Unix upstream echo socket;
- a Unix upstream can appear in a supported TCP chain position;
- Windows builds do not require Unix APIs;
- no socket-path secrets leak into broad diagnostics beyond the operator-provided path policy.

## Mandatory workstream 5.5 — Outbound local bind

### Goal

Honor pproxy's local/source bind URI component for direct and supported upstream TCP/UDP connections.

### Tasks

1. Carry `local_bind` from compatibility AST into typed runtime connection options.
2. Apply it before TCP connect or UDP socket association.
3. Support IPv4 and IPv6 family-compatible binds.
4. Reject family mismatches with a clear pre-connect error.
5. Keep the option per connection/upstream; do not mutate global networking state.
6. Expose the effective local address in debug/route explanation without exposing unrelated credentials.

### Acceptance criteria

- a loopback test observes the requested source address where the OS permits it;
- invalid or unavailable bind addresses fail at the expected stage;
- native callers may use the typed option without parsing pproxy syntax;
- no global socket configuration is introduced.

## Conditional workstream 5.6 — macOS PF transparent destination recovery

### Decision gate

Implement only if current listener/platform abstractions allow original-destination recovery in a small macOS-specific module comparable to the Linux redir implementation.

Before coding, produce a short technical note answering:

- which socket option or PF lookup API pproxy uses;
- whether stable Rust or a small libc call can retrieve the destination;
- required privileges;
- whether a local disposable test is feasible;
- expected dependency and unsafe-code footprint.

### Implement when

- the change is isolated to a platform module;
- no background daemon or packet capture subsystem is needed;
- the unsafe surface is small and reviewed;
- the feature can be manually verified on macOS.

### Retain non-parity when

- destination recovery requires a broad privileged helper;
- reliable behavior depends on undocumented kernel internals;
- no bounded test is possible;
- implementation materially increases maintenance or binary size.

### Acceptance criteria if implemented

- original TCP destination is recovered under a documented PF redirect rule;
- failure without required privileges is clear;
- Linux and Windows behavior is unchanged;
- the feature is platform-gated and absent from routine CI.

## Conditional workstream 5.7 — Reverse/backward TLS and chain composition

### Decision gate

The runtime already has reverse control connections but rejects TLS on backward links and multi-hop chains containing `+in`. Extend this only when existing stream wrappers can compose around the reverse channel without redesigning the reverse protocol.

### Tasks if local composition is feasible

1. Treat the reverse control stream as another chain stream endpoint.
2. Apply existing TLS wrappers before or after the reverse hop according to pproxy ordering.
3. Permit one reverse hop inside an otherwise supported TCP chain.
4. Preserve authentication and parallel reverse connection count.
5. Add one loopback TLS case and one two-hop case.
6. Reject nested or ambiguous reverse compositions not observed in the oracle.

### Stop condition

If support requires changing reverse framing, introducing multiplexing, or rebuilding the chain executor, retain explicit partial compatibility and document the supported single-hop form.

### Acceptance criteria if implemented

- a supported TLS reverse configuration relays a TCP echo payload;
- one bounded mixed chain containing a reverse hop works;
- unsupported forms fail at config validation;
- no UDP reverse claim is added.

## Conditional workstream 5.8 — `--sys`, logging, and compatibility process behavior

Implement only the observable behavior that reuses current facilities:

- map `--log PATH` to a compatibility-only file sink if tracing initialization can support it locally;
- map `--sys` to existing explicit system-proxy apply/rollback functions when the compatibility executable is used and when the platform backend already exists;
- ensure rollback occurs on normal shutdown and handled signals;
- retain daemonization as unsupported;
- retain `--reuse` as intentional non-parity unless a general native pool already exists by this phase.

Do not create privileged CI. Use dry-run tests and one operator-run platform check.

## Implementation order

1. HTTP-only upstream.
2. Echo protocol.
3. Fixed-target TCP and UDP.
4. Unix upstream.
5. Outbound local bind.
6. Evaluate PF decision gate.
7. Evaluate reverse/TLS decision gate.
8. Apply small process-behavior mappings only where existing facilities suffice.

Each mandatory workstream should be independently mergeable.

## Acceptance criteria for the phase

Phase 5 is complete when:

- all five mandatory workstreams pass their criteria;
- conditional PF and reverse work has either landed within its bounded gate or has an explicit recorded non-parity decision;
- process options use existing facilities or remain explicitly unsupported;
- no excluded protocol family has been introduced;
- no new test matrix, privileged CI workflow, or compatibility runtime has been added;
- the binary-size/dependency impact is reviewed for any new crate dependency;
- Python compatibility entry points can construct every newly supported feature;
- documentation and diagnostics accurately classify remaining exclusions.

## Focused verification

Run tests per workstream rather than a mandatory full matrix:

```bash
cargo fmt --check
cargo test -p eggress-protocol-http httponly
cargo test -p eggress-runtime echo
cargo test -p eggress-runtime fixed_target
cargo test -p eggress-runtime unix_upstream
cargo test -p eggress-runtime local_bind
python -m pytest python/tests -k "httponly or echo or fixed_target or unix or local_bind"
```

Run `cargo test --workspace` once at the end only if shared stream or socket abstractions changed.

## Dependency policy

Prefer standard library, Tokio, and dependencies already in the workspace. A new dependency requires a short justification in the implementation commit describing:

- why existing dependencies cannot provide the function;
- feature flags enabled;
- approximate binary-size/security implications;
- whether it is platform-gated.

Do not add AsyncSSH, QUIC stacks, legacy crypto libraries, packet-capture stacks, or general web frameworks in this phase.

## Handoff guidance

Treat each workstream as a narrow feature patch. Do not begin with the conditional items. If a mandatory feature unexpectedly requires architectural redesign, stop and document the blocker rather than widening the patch.
