# Phase 20 Plan: Standalone UDP and UDP Chain Parity

## Purpose

Phase 20 closes the UDP replacement gap between Eggress and Python `pproxy`. Eggress already has standards-oriented SOCKS5 UDP ASSOCIATE support, direct UDP forwarding, and one-hop SOCKS5/Shadowsocks UDP upstream support. That is useful, but it is not a drop-in replacement for pproxy's standalone UDP workflow.

The goal of this phase is to implement pproxy-compatible standalone UDP listen and remote behavior, equivalent to `-ul` and `-ur`, while preserving Eggress-native SOCKS5 UDP ASSOCIATE as a distinct standards-compliant mode.

## Dependencies

Phase 20 depends on Phase 18 for pproxy oracle/differential infrastructure. It should preferably follow Phase 19 so the HTTP/SOCKS baseline remains stable before UDP chain behavior is added.

## Non-goals

Do not fix Shadowsocks TCP framing in this phase except where needed to route UDP through an already-supported Shadowsocks UDP path. Do not implement MASQUE, CONNECT-UDP, HTTP/3 datagrams, or QUIC UDP tunneling here. Those belong to the advanced transport phase.

Do not remove SOCKS5 UDP ASSOCIATE. The goal is additive compatibility, not replacing standards-compliant behavior with pproxy-specific behavior.

## Work items

### 20.1 Capture pproxy UDP behavior

Before implementation, use the Phase 18 oracle to characterize pproxy's UDP mode.

Capture:

- `-ul` syntax forms;
- `-ur` syntax forms;
- default bind addresses and ports;
- packet framing;
- whether a TCP control channel is required;
- behavior for IPv4, IPv6, and domain targets;
- handling of nonzero FRAG;
- behavior on oversized datagrams;
- client association identity semantics;
- idle timeout behavior;
- error behavior for unreachable targets;
- how multiple clients using the same UDP listener are demultiplexed;
- behavior through direct, SOCKS5, Shadowsocks, and chained upstreams;
- log output and exit behavior for invalid configurations.

Persist the captured behavior in:

```text
docs/PPROXY_PARITY_SPEC.md
tests/compat/fixtures/pproxy_udp_behavior.md
```

### 20.2 Define a distinct UDP mode model

Add a runtime distinction between:

- `Socks5UdpAssociate`: standards-compliant UDP tied to a SOCKS5 TCP session;
- `StandalonePproxyUdp`: pproxy-compatible standalone UDP relay;
- `NativeConfiguredUdp`: any Eggress-native TOML-driven UDP mode, if retained separately.

The distinction should appear in configuration, route explanation, metrics, and admin status. Avoid silently mapping pproxy `-ul` onto SOCKS5 UDP ASSOCIATE. That was the previous semantic gap.

### 20.3 CLI compatibility for `-ul` and `-ur`

Extend `eggress-pproxy-compat` so it can parse and run pproxy-style UDP flags.

Requirements:

- accept `-ul` listen URI or address forms observed in pproxy;
- accept `-ur` remote/upstream forms observed in pproxy;
- support multiple UDP remotes if pproxy does;
- support direct UDP remote behavior;
- support pproxy-compatible error diagnostics for invalid combinations;
- include warnings only for true divergence, not for implemented behavior;
- ensure generated TOML preserves the standalone UDP semantics.

Update `translate`, `check`, and `run` subcommands.

### 20.4 Standalone UDP listener implementation

Implement a UDP listener that accepts SOCKS5-style UDP datagram headers without requiring a SOCKS5 TCP control connection, matching pproxy's standalone UDP mode.

Requirements:

- decode address header: RSV, FRAG, ATYP, ADDR, PORT;
- reject or handle FRAG according to captured pproxy behavior;
- support IPv4, IPv6, and domain targets;
- maintain per-client flow state;
- route each datagram using the routing engine;
- encode replies with the expected pproxy-compatible header;
- enforce association limits and target-flow limits;
- expose metrics distinct from SOCKS5 UDP ASSOCIATE metrics;
- enforce amplification controls.

### 20.5 UDP routing and chain execution

Extend UDP routing beyond one-hop cases.

Requirements:

- support direct UDP target forwarding;
- support UDP through one-hop SOCKS5 upstream;
- support UDP through one-hop Shadowsocks upstream;
- implement UDP multi-hop chain semantics where pproxy supports them;
- reject impossible chains at startup or first route selection with deterministic diagnostics;
- expose chain capability validation for UDP separately from TCP.

Design note:

UDP chains are not stream chains. The implementation should model each hop as a datagram transform/relay capability. Avoid forcing UDP into the TCP chain executor if it creates hidden state or incorrect backpressure assumptions.

### 20.6 Flow lifecycle and cleanup

Define clear lifecycle semantics for standalone UDP flows.

State should include:

- client socket address;
- requested target address;
- selected route/upstream;
- upstream association handle if applicable;
- last activity timestamp;
- packet counters;
- byte counters;
- close/reap reason.

Cleanup requirements:

- per-flow idle timeout;
- per-client flow cap;
- global flow cap;
- target-flow idle cleanup;
- cleanup on runtime shutdown;
- cleanup on config reload where route compatibility changes;
- metrics decrement on all close paths.

### 20.7 Differential tests

Add pproxy oracle tests for UDP.

Initial cases:

- standalone UDP direct echo;
- standalone UDP domain target echo;
- malformed short datagram;
- nonzero FRAG behavior;
- oversized datagram behavior;
- two clients using the same UDP listener;
- two targets from the same client;
- UDP through SOCKS5 upstream if pproxy supports the same topology;
- UDP through Shadowsocks upstream if pproxy supports the same topology;
- UDP chain behavior for at least one multi-hop topology if pproxy supports it.

Compare:

- reply payload;
- reply header target fields;
- timeout behavior;
- whether malformed datagrams are ignored or produce errors;
- process log/diagnostic shape for invalid configs.

### 20.8 Security controls

Standalone UDP is more exposed than SOCKS5 UDP ASSOCIATE because it lacks a TCP control channel.

Required controls:

- reject multicast/broadcast/private ranges according to configured policy;
- configurable private-network egress policy;
- packet size limit;
- per-source flow cap;
- global flow cap;
- idle reaping;
- amplification ratio considerations;
- no reflection of large errors to spoofed clients;
- optional source pinning when a flow is established;
- bounded DNS resolution if domain targets are resolved inside Eggress.

Compatibility mode may need to allow behavior that is less strict than default mode. If so, require explicit compatibility configuration.

### 20.9 Metrics and admin visibility

Expose standalone UDP metrics separately.

Suggested metrics:

- standalone UDP packets in/out;
- standalone UDP bytes in/out;
- malformed UDP datagrams;
- rejected UDP datagrams by reason;
- active standalone UDP flows;
- UDP flow reaps by reason;
- upstream UDP association failures;
- UDP route fallback count.

Admin/status should show listener mode, bind address, active flow count, limits, and last error category.

### 20.10 Documentation updates

Update:

- `docs/PPROXY_PARITY_SPEC.md` with captured pproxy UDP behavior;
- `docs/PARITY_MATRIX.md` for `-ul`, `-ur`, standalone UDP, and UDP chains;
- `docs/CONFIG_REFERENCE.md` for standalone UDP config;
- `docs/METRICS.md` for new metrics;
- `docs/PPROXY_MIGRATION.md` to explain distinction from SOCKS5 UDP ASSOCIATE;
- README capability table.

## Validation commands

At minimum:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p eggress-udp
cargo test -p eggress-runtime udp
cargo test -p eggress-pproxy-compat udp
cargo test --test differential_pproxy -- udp --nocapture
```

Add any required local script:

```bash
./scripts/compat_udp_pproxy.sh
```

## Acceptance criteria

Phase 20 is complete when:

- `-ul` and `-ur` are parsed and runnable in pproxy compatibility mode;
- standalone UDP relay exists independently from SOCKS5 UDP ASSOCIATE;
- UDP direct relay has pproxy differential evidence;
- UDP through supported upstreams has differential or interop evidence;
- UDP multi-hop chains are implemented where pproxy supports them or explicitly documented with captured evidence if deferred;
- UDP metrics/admin state distinguish standalone UDP from SOCKS5 UDP ASSOCIATE;
- security controls are enforced and tested;
- docs and the manifest reflect the new evidence accurately.

## Risks

UDP spoofing and amplification risk is higher in standalone mode. Do not let compatibility mode silently weaken safe defaults. Make compatibility permissiveness explicit.

UDP chain behavior may be underspecified in pproxy. Use the oracle harness to record observed behavior, then implement only confirmed semantics.

Flow cleanup bugs can create slow leaks. Add tests for idle reaping, client churn, target churn, and shutdown.

## Handoff notes

The key architectural point is to stop treating SOCKS5 UDP ASSOCIATE as equivalent to pproxy standalone UDP. They share a datagram header shape but not session semantics. This phase should make both modes first-class and separately testable.
