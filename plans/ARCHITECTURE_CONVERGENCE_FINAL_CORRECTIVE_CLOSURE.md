# Architecture Convergence Final Corrective Closure

## Status

**PLANNED**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Baseline commit: `ff4d5fcc724e423c91e6b3f850f764e4ed8cb9c5`
- Parent roadmap: `plans/ARCHITECTURE_CONVERGENCE_ROADMAP.md` (**IMPLEMENTED**)
- Purpose: close two residual defects found during post-implementation review without reopening the completed roadmap or expanding scope.

## Scope

This is a narrow corrective pass. It contains exactly two workstreams:

1. route `OutboundConnector::from_toml()` through the canonical shared parse/validate/compile boundary introduced in Phase 2;
2. make listener-free outbound UDP socket binding address-family aware for direct IPv4/IPv6 targets and review the SOCKS5 UDP relay bind for the same family mismatch class.

No other cleanup, feature expansion, parity work, module reshaping, CI work, release automation, protocol additions, or documentation program is authorized by this plan.

## Why this plan exists

The architecture-convergence roadmap landed correctly in its major objectives, but post-closure review found two residual issues:

- `eggress-embed::outbound::OutboundConnector::from_toml()` still duplicates TOML parse, config-version checking, validation, and compilation instead of delegating to the crate's shared `parse_validate_compile()` boundary. This is primarily a maintenance/convergence miss rather than a currently demonstrated behavioral defect.
- `OutboundConnector::associate_udp()` supports IPv6 targets in its target/address model, but the direct UDP path binds the local socket to `127.0.0.1:0` unconditionally before connecting to the resolved target. A direct IPv6 target therefore attempts to use an IPv4 socket for an IPv6 peer and can fail despite the public API accepting IPv6. The single-hop SOCKS5 path also passes `127.0.0.1:0` as the UDP bind to `open_socks5_udp_upstream`, so it must be reviewed for equivalent IPv6-relay incompatibility.

The roadmap remains implemented. This file is the final corrective closure record for these residual findings.

---

## Workstream 1 — Canonicalize `OutboundConnector::from_toml()`

### Current problem

`crates/eggress-embed/src/outbound.rs` currently performs its own sequence:

```text
TOML string
  -> toml::from_str<ConfigFile>()
  -> version check
  -> validate_config()
  -> compile_config()
  -> outbound-specific upstream checks
```

`crates/eggress-embed/src/lib.rs` already defines the shared crate-internal boundary:

```rust
parse_validate_compile(input: &str) -> Result<RuntimeConfig, String>
```

Phase 2 established this helper specifically so TOML entry points do not drift in version/validation/compile semantics. The outbound constructor should consume that canonical result and retain only its own outbound-specific semantic checks.

### Required implementation

1. Replace the local TOML parse/version/validate/compile block in `OutboundConnector::from_toml()` with `crate::parse_validate_compile(config_toml)`.
2. Map the shared helper error into the existing `EggressError::Config` taxonomy without changing public error-class shape.
3. Preserve outbound-specific checks after compilation:
   - at least one upstream must exist;
   - selected/first upstream chain must not be empty;
   - any existing outbound-only composition constraints remain where they are semantically owned.
4. Do not make `parse_validate_compile` public solely for this change; `pub(crate)` is the intended boundary.
5. Do not create another helper in `outbound.rs` that simply wraps the canonical one.
6. Review other immediately adjacent TOML entry points only to confirm they already use the shared boundary. Do not broaden this pass into an unrelated constructor refactor.

### Required regression tests

Add focused tests proving:

- a valid TOML outbound configuration still constructs successfully;
- unsupported config `version` produces the same public error class/message family as other embed TOML entry points;
- validation failures from malformed/invalid config are surfaced through `EggressError::Config`;
- an otherwise valid config with no upstreams still fails with the existing outbound-specific `no upstreams configured` condition;
- an upstream with an empty chain still fails with the existing outbound-specific empty-chain condition where constructible through test fixtures;
- no behavior change occurs for representative supported HTTP/SOCKS/TLS-wrapped outbound configs.

Where practical, use a small equivalence test that feeds the same TOML through `EggressConfig::from_toml_str()` and `OutboundConnector::from_toml()` and asserts that shared parse/version/validation failures are classified consistently.

### Acceptance criteria

This workstream is complete only when:

- `OutboundConnector::from_toml()` no longer directly invokes `toml::from_str`, `validate_config`, or `compile_config` for normal construction;
- TOML parsing/version/validation/compilation has one canonical implementation inside `eggress-embed`;
- outbound-only post-compilation checks remain explicit;
- public API shape and successful supported behavior remain unchanged.

---

## Workstream 2 — Make listener-free UDP address-family aware

### Current problem

The direct listener-free UDP path resolves the target to a `SocketAddr` and then always binds:

```rust
UdpSocket::bind("127.0.0.1:0")
```

before `connect(resolved)`.

The target model accepts both IPv4 and IPv6 (`SocksAddr::IPv4` / `SocksAddr::IPv6`), so this creates an implementation mismatch: IPv6 is accepted and resolved but the local socket family is forced to IPv4.

The SOCKS5 path currently supplies `127.0.0.1:0` as `udp_bind` to `open_socks5_udp_upstream`. This may likewise fail if the relay endpoint is IPv6 or if the underlying UDP path requires a same-family local bind. The implementation must inspect the existing upstream primitive before deciding whether that bind should be family-specific, dual-stack, or intentionally IPv4-only with an explicit error.

### Required direct-path behavior

1. After target resolution, choose the local wildcard bind address based on the resolved target family:
   - IPv4 target -> `0.0.0.0:0` or equivalent IPv4 unspecified address;
   - IPv6 target -> `[::]:0` or equivalent IPv6 unspecified address.
2. Do not bind direct outbound UDP to loopback unless a documented policy requires it. The association's destination is explicitly caller-selected, so the socket should use a normal ephemeral wildcard source bind matching the destination family.
3. Preserve private/loopback destination allowance already documented for the listener-free API.
4. Preserve DNS failure classification and timeout/cancellation semantics.
5. If DNS yields multiple addresses, do not silently introduce a large Happy-Eyeballs-style subsystem in this pass. Use the current resolution selection semantics unless a minimal family-aware retry is required to avoid choosing an unusable first result.

### Required SOCKS5-path review

Inspect `eggress_udp::upstream_socks5::open_socks5_udp_upstream` and its `udp_bind` semantics.

Implement one of the following based on the actual primitive contract:

**Preferred if relay family is known before bind:** choose an unspecified local bind matching the SOCKS5 UDP relay address family.

**Preferred if the primitive itself can own family selection:** change the narrow primitive/API so callers may request an ephemeral family-compatible bind without providing a hard-coded IPv4 address. Keep the abstraction local to the UDP subsystem; do not introduce a generalized datagram socket factory.

**If upstream SOCKS5 IPv6 relay is intentionally unsupported:** return a structured `UnsupportedFeature`/runtime classification and document/test that limitation explicitly. Do not allow a cryptic address-family OS error to serve as the public contract.

The implementer must not assume the current `127.0.0.1:0` is harmless merely because current tests use IPv4 in-process relays.

### Required tests

At minimum add:

- direct IPv4 UDP echo remains passing;
- direct IPv6 UDP echo using `::1` when the test host supports IPv6;
- IPv6 test must bind an actual IPv6 echo socket and prove `associate_udp("::1", port)` send/recv works;
- if IPv6 is unavailable on the host, skip only with a capability-detection reason rather than treating failure as success;
- DNS target resolving to IPv6 is covered where deterministic local test infrastructure can provide it without external DNS dependency; otherwise literal `::1` is mandatory and sufficient for closure;
- direct close/drop/accounting behavior remains unchanged for IPv6 associations;
- SOCKS5 UDP relay family handling gets a deterministic test for whichever behavior is implemented:
  - IPv6 relay works, or
  - IPv6 relay fails with the explicit structured classification chosen above;
- no new silent fallback to direct is introduced when SOCKS5 UDP setup fails.

### Security and correctness constraints

- Do not weaken existing datagram-size validation.
- Do not weaken target validation or private-egress policy in listener-based UDP code.
- Do not alter listener-side UDP binding semantics unless the shared upstream primitive genuinely requires a narrow fix used by both surfaces.
- Preserve cancellation of pending send/recv and idempotent close.
- Preserve `active_udp_associations()` exactly-once increment/decrement behavior.
- Avoid logging proxy credentials or full secret-bearing URIs while adding diagnostics.

### Acceptance criteria

This workstream is complete only when:

- direct listener-free UDP works for both IPv4 and IPv6 loopback targets on hosts supporting IPv6;
- local direct UDP binding selects the address family from the resolved destination rather than hard-coding IPv4 loopback;
- SOCKS5 UDP's hard-coded IPv4 bind has been either removed/family-corrected or converted into an explicit documented IPv6 limitation with a structured error;
- IPv4 behavior remains unchanged;
- lifecycle, timeout, size-limit, and unsupported-composition tests remain green.

---

## Explicit non-goals

Do not implement or redesign any of the following in this corrective pass:

- MASQUE / CONNECT-UDP;
- Trojan UDP;
- Shadowsocks UDP support for the listener-free facade;
- multi-hop UDP expansion;
- a generalized address-selection or Happy Eyeballs framework;
- listener-side UDP architecture changes unrelated to family compatibility;
- TPROXY or transparent IPv6 original-destination support;
- reverse TLS changes;
- certificate hot reload;
- new Cargo feature groups;
- new crates;
- new hosted CI workflows or OS matrices;
- new parity manifests, evidence bundles, certification reports, or completion documents beyond updating this plan in place;
- broad API naming cleanup or constructor redesign.

If either workstream exposes another independent defect, document it in the implementation/PR summary. Do not pull it into this plan unless it blocks these acceptance criteria.

## Implementation sequence

1. Canonicalize `OutboundConnector::from_toml()` first. This should be a small, behavior-preserving change and reduces noise before UDP changes.
2. Fix direct UDP bind-family selection and add IPv4/IPv6 tests.
3. Inspect and correct/classify SOCKS5 UDP family behavior.
4. Run focused embed/UDP tests.
5. Run the existing broad repository gate.
6. Update this plan's closure record in place; do not create another follow-up plan if all criteria pass.

## Verification

During implementation:

```bash
cargo test -p eggress-embed outbound
cargo test -p eggress-embed udp
cargo test -p eggress-udp
```

At closure:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-embed
cargo test -p eggress-udp
cargo test --workspace --locked
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Python tests are not required unless implementation touches Python-facing code. External pproxy interoperability is not required because neither correction changes the compatibility grammar or parity claim.

Do not add a new CI job solely for IPv6. The deterministic local Rust test is sufficient; it may capability-skip on hosts where IPv6 loopback is unavailable.

## Final closure criteria

This corrective plan can be marked **IMPLEMENTED** only when all are true:

- `OutboundConnector::from_toml()` delegates parse/version/validation/compilation to the canonical embed helper;
- no duplicate parse/validate/compile block remains in that constructor;
- direct outbound UDP uses a destination-family-compatible local bind;
- direct IPv6 UDP is proven by deterministic loopback test when IPv6 is available;
- SOCKS5 UDP family behavior is either family-compatible or explicitly structured/documented as unsupported for IPv6 relays;
- existing IPv4 UDP, timeout, cancellation, close/drop, accounting, and unsupported-composition behavior remains green;
- no unrelated feature expansion or CI architecture change lands as part of this pass;
- the workspace broad gate and fuzz-bin compile gate pass;
- this file is updated in place with implementation commit(s), affected test locations, and any intentionally retained limitation.

## Closure record

When complete, replace this section with:

- implementation commit/range;
- exact canonical helper now used by `OutboundConnector::from_toml()`;
- final direct IPv4/IPv6 bind strategy;
- final SOCKS5 UDP IPv6 behavior;
- principal regression test names/locations;
- broad verification result;
- confirmation that no roadmap scope was reopened.

Do not create a separate completion/evidence document.