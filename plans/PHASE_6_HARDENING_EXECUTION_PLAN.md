# Phase 6 Detailed Execution Plan: Hardening, Parity Validation, and Release Readiness

## Purpose

Phase 5 is now functionally closed: supported upstream protocols are no longer overstated, Shadowsocks is explicitly experimental/unsupported for active parity claims, Trojan’s exported connect path is directly tested, TLS dependency policy is explicit, and hosted CI visibility is documented honestly.

Phase 6 is a hardening and validation phase. It must not add major protocol support. Its purpose is to prove that the current supported subset is safe, observable, regression-resistant, and comparable to Python `pproxy` for the claims Eggress actually makes.

This plan is written for smaller-model handoff. Keep changes incremental, test-first, and conservative. Any new feature discovered during hardening should become a later plan, not be implemented opportunistically in Phase 6.

---

# Current supported baseline

Treat this as the support matrix at Phase 6 start:

| Area | Status |
|---|---|
| Direct TCP CONNECT | Supported |
| Direct HTTP forward | Supported |
| SOCKS4 inbound | Supported |
| SOCKS5 inbound CONNECT | Supported |
| SOCKS5 UDP ASSOCIATE direct relay | Supported |
| SOCKS5 UDP one-hop upstream relay | Supported |
| HTTP CONNECT upstream TCP | Supported |
| SOCKS4/SOCKS4a upstream TCP | Supported |
| SOCKS5 upstream TCP | Supported |
| Trojan TCP upstream | Supported, rustls-based |
| Shadowsocks TCP | Experimental/partial; not supported |
| Shadowsocks UDP | Experimental/non-interop; not supported |
| HTTP/SOCKS4/Trojan UDP upstream | Unsupported |
| Multi-hop UDP | Unsupported |
| TLS transport wrapping | Supported where explicitly configured |

Phase 6 should harden this matrix. Do not promote Shadowsocks support in this phase.

---

# Non-goals

Do not implement:

- Shadowsocks stream adapter or UDP interoperability;
- new upstream protocols;
- QUIC, MASQUE, CONNECT-UDP, HTTP/3;
- transparent proxying;
- packet capture or kernel TPROXY;
- system proxy installation;
- native TLS/OpenSSL;
- unsafe Rust;
- production dependency on native C build tooling;
- public-internet-dependent tests.

---

# Workstream 1: Plan archive and repo-status cleanup

## Goal

Make the planning/audit trail coherent before deeper hardening begins.

## Tasks

1. Inspect `plans/README.md` and current `plans/` layout.
2. Ensure active Phase 6 plans are easy to identify.
3. Move or copy completed plans into `plans/archive/` if the repository policy says historical plans should be retained.
4. Confirm no completed plan files were accidentally deleted without an archived equivalent.
5. Add a short `plans/README.md` table:

```markdown
| Plan | Status | Completion doc |
|---|---|---|
| PHASE_1_* | Archived | docs/... |
| PHASE_2_* | Archived | docs/... |
| PHASE_3_* | Archived | docs/... |
| PHASE_4_* | Archived | docs/... |
| PHASE_5_* | Archived/closed | docs/PHASE_5_CORRECTIVE_CLOSURE_COMPLETION.md |
| PHASE_6_HARDENING_EXECUTION_PLAN.md | Active | pending |
```

## Acceptance criteria

- Plan lifecycle is explicit.
- Active vs archived plans are unambiguous.
- No runtime behavior changes in this workstream.

---

# Workstream 2: CI/status visibility remediation

## Goal

Hosted CI currently has no reliable visible status contexts from the connector perspective. Phase 6 should make CI actionable, or document precisely why it cannot run.

## Tasks

1. Inspect `.github/workflows/ci.yml` and `.github/workflows/security.yml`.
2. Verify triggers include pushes to `main` and pull requests.
3. Confirm workflows do not depend on unavailable secrets or paid minutes for ordinary checks.
4. If the repo’s GitHub Actions account is blocked by billing/spending limit, document that in `docs/CI_STATUS.md` and do not claim hosted CI is green.
5. Add local fallback verification instructions.
6. If workflow status can be restored, ensure the following are separate visible jobs:
   - `fmt`;
   - `check`;
   - `test`;
   - `clippy`;
   - `deny`;
   - `audit`.
7. Avoid long-running fuzz/bench jobs in default CI.

## Required workflow commands

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

## Docs

Create or update:

```text
docs/CI_STATUS.md
```

Include:

- current observed hosted CI state;
- required local commands;
- known GitHub Actions billing/status blocker if present;
- how to interpret completion docs when CI is unavailable.

## Acceptance criteria

- Hosted CI either produces visible status contexts, or `docs/CI_STATUS.md` clearly states local verification is the source of truth.
- Completion docs stop relying on invisible hosted checks.

---

# Workstream 3: Parser and codec property tests

## Goal

Add lightweight property tests before fuzzing. These should be deterministic enough for normal `cargo test`.

## Targets

Add property tests for:

- SOCKS5 UDP datagram codec;
- SOCKS5 address codec if exposed;
- SOCKS4/SOCKS4a request encoding/parsing;
- HTTP CONNECT response parser limits;
- URI parser/redaction;
- Trojan request encoder;
- route matcher behavior for generated request contexts.

## Guidelines

Use `proptest` where it improves coverage. Keep case counts modest in normal CI.

Recommended file placement:

```text
crates/eggress-protocol-socks/tests/codec_properties.rs
crates/eggress-protocol-http/tests/connect_properties.rs
crates/eggress-uri/tests/properties.rs
crates/eggress-protocol-trojan/tests/request_properties.rs
crates/eggress-routing/tests/properties.rs
```

If crate-private helpers block useful external property tests, expose `pub(crate)` helpers under unit tests instead of widening public APIs unnecessarily.

## Required properties

SOCKS5 UDP:

- encode/decode round-trip for IPv4, IPv6, and domain targets;
- invalid `FRAG != 0` is rejected;
- bad RSV is rejected;
- zero-length domain is rejected;
- max domain length is accepted if supported;
- oversized domain is rejected if encoder validates.

HTTP CONNECT:

- response parser rejects over-limit status lines;
- response parser rejects over-limit total headers;
- parser never accepts malformed status without a numeric code;
- credential validation rejects control chars.

URI/redaction:

- redacted display never includes credential substrings;
- parse/display/redact path handles IPv6 bracketed endpoints;
- duplicate hop separators reject;
- unknown protocol rejects.

Trojan:

- request encoder never emits a one-byte domain length for >255 bytes;
- encoded domain length equals actual domain length for valid domains;
- IPv4/IPv6 layouts remain fixed size;
- password hash is 56 hex chars.

Routing:

- reject rules stop routing;
- direct fallback only occurs when configured;
- unsupported UDP upstream never becomes direct unless route policy explicitly selects direct fallback.

## Acceptance criteria

- Property tests are committed and pass under normal `cargo test --workspace`.
- Case counts are low enough not to slow local development significantly.

---

# Workstream 4: Fuzz harness smoke foundation

## Goal

Add fuzz harnesses for high-risk byte parsers/codecs. Fuzz campaigns should be manual or smoke-only; do not make long fuzzing mandatory for every CI run.

## Preferred tooling

Use `cargo-fuzz` if acceptable. If that creates too much workspace friction, add a `fuzz/` directory that is documented but not part of the default workspace.

Recommended layout:

```text
fuzz/
├── Cargo.toml
└── fuzz_targets/
    ├── socks5_udp_datagram.rs
    ├── socks5_handshake.rs
    ├── http_connect_response.rs
    ├── uri_parse.rs
    ├── route_match.rs
    └── trojan_request_decode_or_encode.rs
```

## Smoke command examples

```bash
cargo fuzz run socks5_udp_datagram -- -runs=1000
cargo fuzz run http_connect_response -- -runs=1000
cargo fuzz run uri_parse -- -runs=1000
```

## Invariants

- no panic;
- no unbounded allocation;
- parser returns structured error or valid decoded value;
- no credential leak in redacted formatting;
- no infinite loops.

## Acceptance criteria

- Fuzz harnesses compile.
- `docs/TESTING.md` documents how to run smoke fuzzing.
- At least SOCKS5 UDP and URI parser harnesses exist.

---

# Workstream 5: Runtime invariant tests

## Goal

Strengthen runtime lifecycle safety around sessions, UDP associations, leases, reloads, and shutdown.

## Required tests

Add or extend runtime tests for:

1. TCP active lease increments after upstream connect and decrements after relay close.
2. Pending lease dropped on failed upstream connect does not increment active.
3. HTTP upstream 407/403 failure is categorized and releases pending lease.
4. SOCKS4 failure reply releases pending lease.
5. UDP target-flow idle timeout releases upstream active lease.
6. UDP association close removes registry entry and leaves active count zero.
7. Shutdown with active TCP sessions drains within grace period.
8. Shutdown with active UDP association cancels relay tasks and leaves counts zero.
9. Reload failure preserves previous generation and route behavior.
10. Reload success affects new sessions but not already-open TCP streams.
11. Unsupported UDP upstreams are rejected or metriced, never silently direct-routed.

Suggested file:

```text
crates/eggress-runtime/tests/lifecycle_invariants.rs
```

Avoid sleep-heavy tests. Use short timeouts, readiness flags, and observable counters.

## Acceptance criteria

- Runtime state counters return to zero after test cleanup.
- Tests are deterministic on normal developer machines.

---

# Workstream 6: pproxy differential/interoperability tests

## Goal

Validate practical parity against Python `pproxy` for the supported subset, without requiring public internet.

## Scope

Gated tests only. Do not run by default.

Environment gate:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1
```

If Python or pproxy is unavailable, tests should skip cleanly with a clear message.

## Scenarios

Compare Eggress and `pproxy` behavior for:

1. SOCKS5 CONNECT inbound to local TCP echo.
2. HTTP CONNECT inbound to local TCP echo.
3. SOCKS5 UDP ASSOCIATE direct local UDP echo.
4. SOCKS5 inbound through HTTP CONNECT upstream.
5. SOCKS5 inbound through SOCKS5 upstream.
6. SOCKS5 UDP inbound through SOCKS5 UDP upstream if pproxy supports comparable mode reliably.
7. Auth failure behavior for SOCKS5 and HTTP where comparable.

## Test design

For each scenario:

1. Start local echo target.
2. Start pproxy with equivalent mode.
3. Start Eggress with equivalent config.
4. Send identical client request to both.
5. Compare success payload or coarse error category.
6. Shut down all processes.

Suggested file:

```text
crates/eggress-cli/tests/differential_pproxy.rs
```

## Acceptance criteria

- Gated differential tests exist and are documented.
- Failure to find pproxy skips rather than fails normal tests.
- `docs/PARITY_MATRIX.md` states which scenarios have differential coverage.

---

# Workstream 7: Security review pass

## Goal

Document and test the main security surfaces before any release tag.

## Review areas

- credential redaction in URI display, logs, admin output, and errors;
- admin bind defaults and exposure risk;
- non-loopback unauthenticated listeners;
- UDP amplification controls;
- UDP target filtering for multicast/broadcast/unspecified addresses;
- oversized HTTP headers/status lines;
- oversized SOCKS domain fields;
- Trojan password and server-name handling;
- TLS insecure mode must remain test-only or explicitly gated;
- route expression complexity limits;
- reload safety and rollback;
- task leaks on malformed clients;
- metrics label cardinality.

## Required docs

Create:

```text
docs/SECURITY_REVIEW.md
```

Required sections:

- threat model;
- trusted/untrusted inputs;
- reviewed surfaces;
- mitigations already implemented;
- residual risks;
- deferred items;
- release blockers.

## Required tests

- redacted URI never includes username/password;
- admin config/status endpoints never expose credentials;
- HTTP CONNECT credentials with control chars reject;
- oversized HTTP CONNECT response headers reject;
- UDP broadcast/multicast/unspecified targets reject;
- TLS insecure mode not reachable from default production config;
- unsupported protocol/transport combinations do not fall back silently.

## Acceptance criteria

- `docs/SECURITY_REVIEW.md` exists.
- Any high-severity finding is either fixed or listed as a release blocker.

---

# Workstream 8: Benchmarks and load tests

## Goal

Add basic performance and resource-regression visibility without turning Phase 6 into an optimization phase.

## Benchmarks

Add one lightweight benchmark harness for:

- TCP direct relay throughput;
- SOCKS5 handshake overhead;
- HTTP CONNECT upstream open latency;
- SOCKS5 UDP direct echo throughput;
- SOCKS5 UDP upstream echo throughput;
- route matcher evaluation.

Use Criterion if acceptable; otherwise add ignored integration tests or examples with timing output.

Suggested locations:

```text
benches/tcp_relay.rs
benches/udp_relay.rs
benches/route_match.rs
```

## Load tests

Add ignored tests for:

- 100 concurrent TCP sessions through direct route;
- 100 concurrent TCP sessions through SOCKS5 upstream;
- many UDP associations up to configured limit;
- reload under active traffic;
- shutdown under active traffic.

All heavy tests must be `#[ignore]` or feature-gated.

## Acceptance criteria

- At least one TCP and one UDP benchmark/load command exists.
- `docs/TESTING.md` documents how to run them and expected local-only assumptions.

---

# Workstream 9: Docs parity and operations cleanup

## Goal

Make documentation reflect executable behavior and operational use.

## Docs to add/update

```text
docs/PARITY_MATRIX.md
docs/CONFIG_REFERENCE.md
docs/METRICS.md
docs/OPERATIONS.md
docs/TESTING.md
docs/RELEASE_READINESS.md
README.md
EGGRESS_ROADMAP.md
AGENTS.md
```

## Required content

`docs/PARITY_MATRIX.md`:

- feature-by-feature comparison to pproxy target behavior;
- supported/experimental/unsupported status;
- test file proving support;
- limitations.

`docs/CONFIG_REFERENCE.md`:

- listener config;
- UDP config;
- upstream URI syntax;
- TLS options;
- unsupported/experimental fields;
- examples.

`docs/METRICS.md`:

- all Prometheus metric names;
- labels and cardinality policy;
- examples;
- what is intentionally not labeled.

`docs/OPERATIONS.md`:

- running service;
- reload behavior;
- shutdown behavior;
- admin endpoints;
- recommended local-only/admin bind defaults.

`docs/TESTING.md`:

- normal local checks;
- focused test commands;
- gated pproxy interop;
- fuzz smoke;
- ignored load tests;
- benchmark commands.

`docs/RELEASE_READINESS.md`:

- checklist for alpha/beta tag;
- CI/local verification status;
- security review status;
- parity matrix status;
- known limitations.

## Acceptance criteria

- Documentation no longer makes unsupported protocol claims.
- Every supported feature in README has a test/doc reference.

---

# Workstream 10: Metrics/admin correctness tests

## Goal

Ensure observability is useful, bounded, and privacy-safe.

## Required tests

- `/metrics` renders without panicking after direct TCP session.
- `/metrics` renders after UDP direct association.
- `/metrics` renders after SOCKS5 UDP upstream relay.
- route decision counters increment for direct/upstream/reject.
- UDP active gauges return to zero after close.
- no client IP, target host, username, password, or payload appears as metric labels.
- admin upstream endpoint redacts credentials.
- admin UDP endpoint does not expose client/target addresses by default.
- admin route-explain does not expose secrets.

Suggested file:

```text
crates/eggress-runtime/tests/observability.rs
```

## Acceptance criteria

- Observability tests prove bounded-label and no-secret policy.

---

# Workstream 11: Dependency and supply-chain guardrails

## Goal

Enforce the dependency policy created during Phase 5.

## Tasks

1. Update `deny.toml` to ban production use of:
   - `openssl-sys`;
   - `native-tls`;
   - `aws-lc-sys` if policy remains ring-only;
   - `cmake` in production dependency graph if enforceable.
2. Document dev-only exceptions if `rcgen` pulls native dependencies only for tests.
3. Add a script or CI step for dependency-tree sanity:

```bash
cargo tree -i aws-lc-sys -e normal || true
cargo tree -i cmake -e normal || true
cargo tree -i openssl-sys -e normal || true
cargo tree -i native-tls -e normal || true
```

4. Ensure `unsafe_code = "forbid"` remains active.

## Acceptance criteria

- Dependency policy is enforceable, not only documented.
- Any exceptions are explicit.

---

# Recommended commit sequence

## Commit 1: Plan archive and CI status docs

- Clean up `plans/` policy/status.
- Add/update `docs/CI_STATUS.md`.
- Do not change runtime code.

## Commit 2: Property tests for codecs/parsers

- Add proptest coverage for SOCKS5 UDP, URI redaction, Trojan encoder, HTTP CONNECT parser.
- Keep test runtime modest.

## Commit 3: Runtime lifecycle invariants

- Add `lifecycle_invariants.rs`.
- Fix any lease/task/gauge bugs found.

## Commit 4: Observability privacy tests

- Add `observability.rs`.
- Verify metrics/admin do not leak secrets or high-cardinality labels.

## Commit 5: Fuzz smoke harnesses

- Add fuzz scaffolding and docs.
- Ensure harnesses compile.

## Commit 6: pproxy differential tests

- Add gated interop tests and skip behavior.
- Update parity docs with coverage.

## Commit 7: Security review and dependency guardrails

- Add `docs/SECURITY_REVIEW.md`.
- Update `deny.toml` and dependency policy enforcement.
- Fix high-severity issues.

## Commit 8: Bench/load tests

- Add lightweight benchmarks and ignored load tests.
- Document commands.

## Commit 9: Docs and release readiness

- Add parity/config/metrics/operations/testing/release docs.
- Add Phase 6 completion record.

---

# Required verification

Run normal checks:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Run focused checks:

```bash
cargo test -p eggress-protocol-socks codec
cargo test -p eggress-protocol-http connect
cargo test -p eggress-protocol-trojan
cargo test -p eggress-uri
cargo test -p eggress-routing
cargo test -p eggress-runtime lifecycle
cargo test -p eggress-runtime observability
cargo test -p eggress-runtime upstream_protocols
cargo test -p eggress-runtime udp_upstream
```

Run optional/gated checks:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test --test differential_pproxy -- --ignored
cargo test -- --ignored load
cargo bench
```

Run dependency sanity:

```bash
cargo tree -i aws-lc-sys -e normal || true
cargo tree -i cmake -e normal || true
cargo tree -i openssl-sys -e normal || true
cargo tree -i native-tls -e normal || true
```

Run fuzz smoke if added:

```bash
cargo fuzz run socks5_udp_datagram -- -runs=1000
cargo fuzz run uri_parse -- -runs=1000
```

---

# Definition of done

Phase 6 is complete only when:

1. Planning archive/status is coherent.
2. CI status is either visible or documented as unavailable with local verification fallback.
3. Property tests cover the main parser/codec/request-builder surfaces.
4. Fuzz smoke harnesses exist for at least SOCKS5 UDP and URI parsing.
5. Runtime lifecycle invariants cover TCP leases, UDP associations, reload, and shutdown.
6. Observability tests prove metrics/admin do not leak secrets or high-cardinality data.
7. Gated pproxy differential tests exist for the core supported subset.
8. Security review doc exists and high-severity findings are fixed or explicitly marked as release blockers.
9. At least one TCP and one UDP benchmark/load path exist and are documented.
10. Dependency policy is enforced by `deny.toml`, dependency-tree checks, or both.
11. Parity/config/metrics/operations/testing/release-readiness docs are current.
12. All normal verification commands pass locally.
13. No unsupported protocol is promoted to supported.
14. No unsafe Rust, OpenSSL/native-tls, or unapproved native build dependency is introduced.

## Completion record

When complete, add:

```text
docs/PHASE_6_HARDENING_COMPLETION.md
```

Include:

- commit list;
- final parity matrix;
- property/fuzz coverage summary;
- differential pproxy coverage summary;
- benchmark/load summary;
- security review summary;
- dependency policy verification;
- CI/local verification status;
- remaining release blockers, if any.
