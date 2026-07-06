# Phase 45: SOCKS BIND and UDP edge semantics

## Goal

Close the remaining SOCKS command gaps and cleanly classify UDP edge semantics. The main pproxy parity blockers in this area are SOCKS4 BIND, SOCKS5 BIND, UDP through unsupported transports, and UDP multi-hop behavior. This phase should either implement these features with tests or explicitly classify them with stable diagnostics and manifest entries.

## Current baseline

Current strengths:

- SOCKS4/4a CONNECT server/client support exists.
- SOCKS5 CONNECT server/client support exists.
- SOCKS5 username/password auth exists.
- SOCKS5 UDP ASSOCIATE exists.
- Standalone pproxy UDP `-ul`/`-ur` exists.
- Direct UDP and one-hop UDP through SOCKS5/Shadowsocks are supported.

Known gaps:

- SOCKS4 BIND is not implemented.
- SOCKS5 BIND is not implemented.
- UDP through Trojan is not complete.
- UDP through HTTP/CONNECT and MASQUE/CONNECT-UDP is not supported.
- UDP through multi-hop proxy chains is not supported.
- SOCKS5 UDP framing has a compatibility warning against pproxy in the manifest.

## Scope

### In scope

- Implement or intentionally classify SOCKS4 BIND.
- Implement or intentionally classify SOCKS5 BIND.
- Add runtime, config, CLI, and Python diagnostics for unsupported BIND when relevant.
- Confirm pproxy behavior for BIND commands and UDP edge cases.
- Harden UDP remote validation for unsupported transports.
- Prevent translator from generating UDP configs that runtime cannot execute.
- Add compatibility tests for supported and unsupported UDP paths.
- Update manifest/report/docs.

### Out of scope

- QUIC/H3/MASQUE production support.
- Trojan UDP unless it can be completed safely within this phase.
- Full UDP multi-hop unless the routing model already supports it cleanly.
- Kernel transparent UDP proxying.

## Workstream A: SOCKS4 BIND

### Design tasks

1. Inspect pproxy SOCKS4 BIND behavior:
   - command code support;
   - bind address reply semantics;
   - active/passive open timing;
   - whether SOCKS4a domain targets are accepted for BIND;
   - error code mapping.
2. Decide whether eggress will implement BIND or classify it as intentional non-parity.
3. If implementing, define connection lifecycle ownership and timeout behavior.

### Implementation tasks if accepted

- Add SOCKS4 BIND command parsing in `eggress-protocol-socks`.
- Add a bounded bind listener allocation path.
- Return the bound address in SOCKS4 reply format.
- Accept one inbound peer connection and relay to the original client.
- Enforce handshake timeout, bind timeout, and connection limits.
- Track metrics: bind attempts, success, timeout, refused.
- Add security controls to avoid opening arbitrary public listeners without policy.

### Tests

- Unit parse tests for SOCKS4 BIND request.
- Integration test where target peer connects to the bound port.
- Timeout test when peer never connects.
- Auth/user-id behavior if applicable.
- IPv4-only limitations documented.

## Workstream B: SOCKS5 BIND

### Design tasks

1. Inspect pproxy SOCKS5 BIND behavior:
   - reply sequence count;
   - first reply bound address;
   - second reply peer address;
   - domain/IPv4/IPv6 behavior;
   - auth interaction.
2. Define policy defaults for opening bind sockets.
3. Decide whether to support BIND only on loopback by default or all configured bind interfaces.

### Implementation tasks if accepted

- Add SOCKS5 BIND command handling in protocol server.
- Allocate a bind socket and report it to the client.
- Accept a single peer connection and send the second SOCKS5 reply.
- Relay bytes with existing TCP relay logic.
- Enforce per-listener and global limits.
- Add clear errors for unsupported address families.

### Tests

- No-auth BIND success.
- Username/password BIND success.
- Peer timeout.
- IPv4 and IPv6 where available.
- Domain target behavior based on pproxy semantics.
- Failure code mapping.

## Workstream C: UDP transport validation

Audit pproxy compat translation and runtime execution for UDP remotes.

Required outcome: translator must not generate a UDP upstream config that runtime cannot actually support without warning or unsupported diagnostic.

Check these cases:

- `-ur direct://...`
- `-ur socks5://...`
- `-ur ss://...`
- `-ur trojan://...`
- `-ur http://...`
- `-ur https://...`
- `-ur socks4://...`
- `-ur ssh://...`
- `-ur ssr://...`
- `-ur remote1__remote2`

For each, classify as:

- `drop_in` if runtime works and pproxy behavior matches;
- `compatible_with_warning` if behavior differs but works;
- `native_equivalent` if only native TOML achieves it;
- `unsupported` if runtime cannot do it.

## Workstream D: SOCKS5 UDP framing warning

The manifest currently says SOCKS5 UDP ASSOCIATE has a framing divergence from pproxy. This needs a focused audit:

1. Capture pproxy's actual UDP datagram framing for the pinned version.
2. Compare with RFC 1928 SOCKS5 UDP request/response framing.
3. Determine whether eggress should emulate pproxy compatibility framing in pproxy mode or keep standards framing.
4. If keeping standards framing, document the incompatibility precisely and keep the warning tier.

Tests should include packet-level assertions for both pproxy mode and native mode if behavior diverges.

## Workstream E: Manifest and diagnostics

Add stable diagnostics for:

- `socks4-bind-unsupported`
- `socks5-bind-unsupported`
- `udp-transport-unsupported`
- `udp-multihop-unsupported`
- `udp-trojan-unsupported`
- `udp-http-connect-unsupported`
- `socks5-udp-framing-divergence`

Manifest entries should state whether each gap is missing, intentionally non-parity, or future work.

## Files to inspect/change

- `crates/eggress-protocol-socks*/`
- `crates/eggress-server/`
- `crates/eggress-runtime/`
- `crates/eggress-udp/`
- `crates/eggress-pproxy-compat/src/translate.rs`
- `crates/eggress-pproxy-compat/src/diagnostics.rs`
- `crates/eggress-cli/tests/differential_pproxy.rs`
- `crates/eggress-cli/tests/pproxy_differential.rs`
- `docs/parity/pproxy_capability_manifest.toml`
- `docs/parity/PPROXY_PARITY_REPORT.md`
- `docs/cli/PPROXY_CLI_INVENTORY.md`
- `docs/PARITY_MATRIX.md`
- Python tests if exposed through `check_pproxy_args`

## Acceptance criteria

- SOCKS BIND capabilities are no longer vague: each is implemented or explicitly classified with a diagnostic.
- UDP remote translation refuses unsupported transports instead of generating misleading config.
- SOCKS5 UDP framing compatibility is documented with packet-level evidence.
- pproxy compatibility check output clearly reports BIND and UDP edge support.
- Manifest/report/docs agree after regeneration.
- No new UDP path bypasses routing/security limits.

## Verification commands

```bash
cargo fmt --all -- --check
cargo test -p eggress-protocol-socks
cargo test -p eggress-udp
cargo test -p eggress-pproxy-compat udp
cargo test -p eggress-pproxy-compat bind
cargo test -p eggress-cli --test pproxy_differential -- --ignored
cargo test --workspace
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
```

## Non-goals

- Do not add QUIC/H3/MASQUE in this phase.
- Do not implement UDP multi-hop unless it has clear routing/runtime support and tests.
- Do not silently emulate nonstandard pproxy UDP framing without an explicit compatibility mode.
