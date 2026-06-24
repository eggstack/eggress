# Phase 6 Detailed Plan: Hardening and Parity Validation

## Purpose

Phase 6 is the stabilization phase after broader upstream protocol parity work. Its goal is to prove that Eggress is robust, observable, secure by default, and meaningfully comparable to `pproxy` for the supported subset.

This phase should not add large new protocols. It should harden what exists through fuzzing, differential tests, benchmarks, CI gates, documentation cleanup, security review, and operational validation.

---

# Scope

## Included

- fuzzing for parsers/codecs/config;
- property tests for route and scheduler invariants;
- local differential tests against Python `pproxy` for supported scenarios;
- load and latency benchmarks;
- UDP abuse/resilience tests;
- memory/task-leak checks;
- admin/metrics correctness tests;
- config reload stress tests;
- docs/README parity matrix cleanup;
- plan archive restoration/alignment;
- CI workflow hardening;
- release-readiness checklist.

## Excluded

Do not add:

- new proxy protocols;
- new routing features;
- QUIC/MASQUE;
- transparent proxying;
- native dependencies;
- unsafe Rust.

---

# Workstream 1: Restore and enforce plan archive policy

## Problem

`plans/README.md` says historical plans are never deleted, but earlier commits removed historical plan files.

## Required work

Choose and enforce the documented policy:

- restore removed historical plan files from Git history;
- place completed plans under `plans/archive/` or keep them under `plans/` with status labels;
- update `plans/README.md` with exact layout;
- ensure current active plans remain easy to identify.

Recommended layout:

```text
plans/
├── README.md
├── active/
│   └── <current active plans>
└── archive/
    ├── phase-1/
    ├── phase-2/
    ├── phase-3/
    ├── phase-4/
    └── phase-5/
```

If moving files is too noisy, retain flat `plans/` but add frontmatter/status at top of each plan.

## Acceptance criteria

- no historical plan is deleted without an archival replacement;
- repo policy matches actual file layout.

---

# Workstream 2: Fuzzing harnesses

## Targets

Add fuzz targets for:

- SOCKS5 handshake parser;
- SOCKS5 UDP datagram codec;
- SOCKS4/SOCKS4a parser if implemented;
- HTTP CONNECT response parser;
- Shadowsocks address/frame parser if implemented;
- Trojan frame parser if implemented;
- TOML config parser/validator with structured input generation;
- route matcher expression parser/evaluator;
- URI parser.

## Tooling

Use `cargo-fuzz` if acceptable. If avoiding extra fuzz workspace complexity, add `proptest` first and leave `cargo-fuzz` as optional CI/manual job.

Recommended layout:

```text
fuzz/
├── Cargo.toml
└── fuzz_targets/
    ├── socks5_udp_codec.rs
    ├── socks5_handshake.rs
    ├── http_connect.rs
    ├── config_parse.rs
    ├── route_match.rs
    └── uri_parse.rs
```

## Required properties

- no panic;
- no unbounded allocation;
- decode either rejects or round-trips valid encodings;
- parser never blocks;
- invalid config produces structured errors.

## CI

Do not run long fuzz campaigns in normal CI. Add smoke fuzz jobs with very short run counts or document manual commands.

## Acceptance criteria

- fuzz harnesses compile;
- smoke fuzz or proptest job runs in CI;
- at least SOCKS5 UDP codec and config parser have fuzz/property coverage.

---

# Workstream 3: Property tests and invariants

## Route invariants

Use `proptest` or deterministic generated cases for:

- rule order is stable;
- reject rules always stop routing;
- direct fallback occurs only when configured;
- unsupported UDP upstream never silently becomes direct;
- reload failure preserves previous snapshot/generation;
- source/listener/identity matchers are deterministic.

## Scheduler invariants

- first-available picks first eligible upstream;
- round-robin advances and wraps;
- least-connections respects active leases;
- unhealthy upstreams are skipped unless policy says use-unhealthy;
- pending lease dropped does not increment active;
- active lease release decrements exactly once.

## UDP invariants

- wrong-client packets do not pin/touch association;
- target-flow limit bounds active flows;
- idle cleanup frees target slots;
- unsupported upstream pending leases are dropped;
- upstream active leases are released on flow cleanup.

## Acceptance criteria

- property tests cover core state-machine invariants without relying on sleeps.

---

# Workstream 4: Differential tests against pproxy

## Goal

Compare Eggress against Python `pproxy` for the supported subset, locally only.

## Supported matrix

Start with:

- HTTP CONNECT inbound to direct target;
- SOCKS5 CONNECT inbound to direct target;
- SOCKS5 UDP ASSOCIATE direct UDP echo;
- SOCKS5 inbound through HTTP upstream;
- SOCKS5 inbound through SOCKS5 upstream;
- SOCKS5 UDP inbound through SOCKS5 UDP upstream if pproxy can act as upstream;
- auth success/failure cases.

Do not compare unsupported protocols as failures; mark them explicitly.

## Test design

Create local test infrastructure:

```text
crates/eggress-cli/tests/differential_pproxy.rs
```

Each test should:

1. start local echo target;
2. start pproxy in comparable mode;
3. start Eggress in comparable mode;
4. send identical client request to both;
5. compare response bytes and error class;
6. clean up processes.

Use environment gate:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1
```

If Python/pproxy is unavailable, skip with clear message.

## Acceptance criteria

- local differential tests exist for the main supported parity claims;
- README states exactly which pproxy scenarios are covered.

---

# Workstream 5: Benchmarks and load tests

## Benchmark targets

Add Criterion or custom benchmark binaries for:

- direct TCP CONNECT throughput;
- SOCKS5 inbound handshake throughput;
- HTTP CONNECT parser throughput;
- UDP direct echo throughput;
- SOCKS5 UDP upstream relay throughput;
- route matcher evaluation throughput;
- config reload latency;
- admin metrics render latency.

## Load tests

Add ignored tests or examples:

- many concurrent TCP connections;
- many UDP associations;
- many target flows per association up to limit;
- reload under active traffic;
- shutdown under active TCP and UDP flows.

Do not run heavy load in normal CI.

## Metrics to report

- requests/sec or packets/sec;
- p50/p95 latency for local echo;
- memory high-water if easy;
- active task count before/after;
- reload pause time.

## Acceptance criteria

- benchmark commands are documented;
- at least one TCP and one UDP benchmark exist;
- load tests have clear run instructions and safety limits.

---

# Workstream 6: Security review pass

## Review areas

- credential redaction in logs/admin/errors;
- admin endpoint exposure and bind defaults;
- non-loopback listener warnings with no auth;
- UDP amplification risk defaults;
- multicast/broadcast/unspecified target rejection;
- header injection in HTTP CONNECT upstream auth;
- TOML config secret handling;
- denial-of-service via large headers/datagrams/config expressions;
- DNS resolution timeouts and cache behavior;
- task leaks on malformed clients.

## Required artifacts

Create:

```text
docs/SECURITY_REVIEW.md
```

with:

- threat model;
- reviewed surfaces;
- findings;
- mitigations;
- deferred risks.

## Tests

- no password appears in debug/admin output;
- admin `/config` redacts secrets;
- oversized headers/datagrams rejected;
- non-loopback unauthenticated UDP listener warns or rejects according to policy;
- invalid route expression depth rejected.

## Acceptance criteria

- security review doc exists and all high-severity findings are fixed or explicitly deferred.

---

# Workstream 7: CI hardening

## Required CI jobs

Add or verify jobs for:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Add feature matrix if features exist.

Add OS matrix only if practical:

- Linux primary;
- macOS optional;
- Windows optional if socket semantics permit.

## Extra checks

- `cargo udeps` optional/manual;
- `cargo machete` optional/manual;
- MSRV check if MSRV is declared;
- docs link check if docs grow.

## Acceptance criteria

- GitHub status checks appear for PRs/main;
- completion docs no longer rely only on commit-message claims;
- CI failures are actionable.

---

# Workstream 8: Documentation and parity matrix

## README updates

Add stable parity table:

| Feature | Eggress | pproxy target | Status |
|---|---|---|---|
| HTTP inbound direct | yes | yes | supported |
| SOCKS5 inbound direct | yes | yes | supported |
| SOCKS5 UDP direct | yes | yes | supported |
| HTTP upstream | yes | yes | supported |
| SOCKS4 upstream | yes/no | yes | status |
| SOCKS5 upstream TCP | yes | yes | supported |
| SOCKS5 upstream UDP | one-hop | yes | supported subset |
| Shadowsocks TCP | status | yes | status |
| Shadowsocks UDP | status | yes | status |
| Trojan TCP | status | yes | status |

## Docs to add/update

- `docs/PARITY_MATRIX.md`;
- `docs/OPERATIONS.md`;
- `docs/METRICS.md`;
- `docs/CONFIG_REFERENCE.md`;
- `docs/SECURITY_REVIEW.md`;
- `docs/TESTING.md`.

## Acceptance criteria

- docs match executable behavior and tests;
- unsupported behavior is not implied as supported.

---

# Workstream 9: Release-readiness checklist

Create:

```text
docs/RELEASE_READINESS.md
```

Checklist:

- CI green;
- security review complete;
- parity matrix updated;
- supported config reference complete;
- examples run locally;
- metrics documented;
- fuzz smoke run complete;
- differential tests run where environment available;
- no known task/memory leaks in load tests;
- docs mention limitations.

## Acceptance criteria

- a maintainer can decide whether Eggress is ready for tagged alpha/beta release.

---

# Recommended commit sequence

## Commit 1: Plan archive restoration/policy

- Restore/archive historical plans.
- Update `plans/README.md`.

## Commit 2: Fuzz/property harness foundation

- Add proptest/fuzz scaffolding.
- Add SOCKS5 UDP codec and config parser fuzz/property tests.

## Commit 3: Route/scheduler/lease invariant tests

- Add generated invariant tests.
- Fix any lease/accounting bugs found.

## Commit 4: Differential pproxy tests

- Add gated local interop tests.
- Document environment requirements.

## Commit 5: Benchmarks/load tests

- Add one TCP and one UDP benchmark.
- Add ignored load tests for reload/shutdown under active traffic.

## Commit 6: Security review and fixes

- Add `docs/SECURITY_REVIEW.md`.
- Add redaction/DoS tests.
- Fix findings.

## Commit 7: CI hardening

- Ensure all required jobs run.
- Add status badge/docs if appropriate.

## Commit 8: Docs and release readiness

- Add parity/config/metrics/testing/operations docs.
- Add release-readiness checklist.
- Add phase completion record.

---

# Required tests

- fuzz/proptest smoke in CI;
- route/scheduler invariants;
- differential pproxy tests gated by env;
- credential redaction tests;
- UDP abuse limit tests;
- reload under active traffic;
- shutdown under active TCP/UDP;
- admin/metrics output consistency;
- docs examples compile or run where feasible.

---

# Verification commands

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Focused/manual:

```bash
cargo test proptest
cargo test differential -- --ignored
cargo bench
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test --test differential_pproxy
```

Fuzz smoke, if `cargo-fuzz` is added:

```bash
cargo fuzz run socks5_udp_codec -- -runs=1000
cargo fuzz run config_parse -- -runs=1000
```

---

# Definition of done

Phase 6 is complete only when:

1. Historical plan policy is enforced in repo layout.
2. Parser/config fuzz or property harnesses exist and run in smoke mode.
3. Route/scheduler/lease invariants are tested.
4. Differential pproxy tests cover the main supported subset.
5. TCP and UDP benchmarks exist and are documented.
6. Security review doc exists and high-severity findings are fixed/deferred explicitly.
7. CI exposes required status checks.
8. Parity matrix, config reference, metrics docs, testing docs, and operations docs are current.
9. Release-readiness checklist exists.
10. All tests, lint, audit, and applicable interop checks pass.
11. No unsafe Rust, OpenSSL dependency, or native dependency is introduced.

## Completion record

When complete, add:

```text
docs/PHASE_6_HARDENING_AND_PARITY_VALIDATION_COMPLETION.md
```

with verification outputs, benchmark summary, fuzz/property coverage summary, and remaining known limitations.
