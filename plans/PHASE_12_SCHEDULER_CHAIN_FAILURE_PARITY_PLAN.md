# Phase 12 Detailed Plan: Scheduler, Chaining, and Failure Semantics Parity

## Purpose

Phase 12 aligns Eggress behavior with Python `pproxy` for chain selection, scheduler behavior, retry/fallback behavior, multi-hop TCP chaining, and client-visible failure semantics. By this phase, the main protocol surface should already be classified or implemented. This phase is about semantics and compatibility, not new protocol invention.

The goal is predictable pproxy-like behavior for supported and compatible paths, while preserving Eggress safety rules.

---

# Prerequisites

Required:

- Phase 7 pproxy parity spec complete;
- Phase 8 CLI/URI compatibility available;
- Phase 9/10 Shadowsocks TCP/UDP status resolved;
- Phase 11 remaining protocol audit complete;
- `docs/PARITY_MATRIX.md` current.

---

# Non-goals

Do not implement:

- new protocol handlers;
- Python bindings;
- PyPI packaging;
- unsafe transparent behavior;
- public-internet tests;
- broad admin UI changes;
- new metrics with high-cardinality labels.

---

# Workstream 1: Scheduler parity audit

## Goal

Compare pproxy scheduling behavior with Eggress schedulers.

## Required output

Add to:

```text
docs/PPROXY_PARITY_SPEC.md
docs/PARITY_MATRIX.md
```

Scheduler behaviors to inspect:

- first/first-available;
- round-robin;
- random;
- least-connections;
- health-aware skip behavior;
- fallback behavior when all upstreams fail;
- retry behavior within a group;
- behavior under active leases.

## Required probes

Use local pproxy/Eggress differential probes where possible:

- two healthy upstreams, round-robin distribution;
- one healthy/one refused upstream;
- all refused upstreams;
- least-connections under concurrent sessions;
- random scheduler sanity distribution if supported.

## Acceptance criteria

- Differences are documented before code changes.

---

# Workstream 2: Scheduler implementation corrections

## Goal

Close any scheduler semantics gap that Phase 12 chooses to support.

## Requirements

- scheduler state must persist across selections;
- round-robin must not reset per connection;
- least-connections must use active lease counts;
- failed pending leases must not become active;
- active leases must release exactly once;
- health filtering must match documented policy;
- direct fallback must occur only when configured.

## Tests

Add or extend:

```text
crates/eggress-routing/tests/scheduler_parity.rs
crates/eggress-runtime/tests/scheduler_runtime.rs
```

Required tests:

- round-robin sequence across multiple sessions;
- least-connections chooses lower active count;
- failed upstream releases pending lease;
- direct fallback only when configured;
- reject when all upstreams fail and fallback is reject;
- health-unavailable upstream skipped if policy says so.

## Acceptance criteria

- Scheduler behavior is deterministic where it should be deterministic.
- Probabilistic behavior has bounded sanity tests only.

---

# Workstream 3: Multi-hop TCP chain parity

## Goal

Verify and correct multi-hop TCP behavior across supported protocols.

## Required chain combinations

Based on enabled protocols:

- SOCKS5 -> HTTP;
- HTTP -> SOCKS5;
- SOCKS4 -> SOCKS5;
- SOCKS5 -> SOCKS4;
- SOCKS5 -> Trojan;
- SOCKS5 -> Shadowsocks after Phase 9;
- TLS-wrapped variants if compatibility syntax supports them.

Do not include unsupported protocols.

## Runtime tests

Add:

```text
crates/eggress-runtime/tests/multihop_tcp.rs
```

Each test should:

1. Start local echo target.
2. Start one or more synthetic upstream proxies.
3. Configure Eggress chain.
4. Connect through inbound SOCKS5 or HTTP CONNECT.
5. Assert echo payload.
6. Assert metrics/leases return to zero.

## Acceptance criteria

- Supported multi-hop TCP chains are tested end to end.
- Unsupported multi-hop combinations reject clearly.

---

# Workstream 4: Failure semantics mapping

## Goal

Make client-visible failure behavior predictable and documented.

## Areas to map

### SOCKS5 replies

Map internal failures to SOCKS5 reply codes for:

- policy denied;
- DNS failure;
- connection refused;
- network unreachable;
- host unreachable;
- upstream auth failure;
- timeout;
- unsupported protocol/transport;
- internal error.

### HTTP CONNECT replies

Map to HTTP status codes for:

- policy denied;
- upstream auth failure;
- target unreachable;
- timeout;
- malformed request;
- unsupported protocol.

### SOCKS4 replies

Map to SOCKS4 success/reject status.

## Required docs

Create or update:

```text
docs/FAILURE_SEMANTICS.md
```

## Tests

Add:

```text
crates/eggress-runtime/tests/failure_semantics.rs
```

Required cases:

- SOCKS5 refused target returns expected code;
- SOCKS5 policy denied returns expected code;
- HTTP CONNECT refused target returns expected status;
- HTTP CONNECT policy denied returns expected status;
- upstream auth failure does not expose credentials;
- unsupported UDP route does not direct fallback silently.

## Acceptance criteria

- Client-visible failure behavior is stable and documented.

---

# Workstream 5: Retry and fallback behavior

## Goal

Match or intentionally diverge from pproxy retry/fallback behavior.

## Questions to answer

- Does pproxy retry another upstream after connect failure in the same group?
- Does pproxy retry on handshake failure?
- Does pproxy retry on auth failure?
- Does pproxy fall back to direct automatically?
- Does pproxy retry UDP upstreams?

## Implementation requirements if supported

- retry budget bounded;
- retries do not duplicate active leases;
- metrics count each attempt;
- final error maps correctly;
- no silent direct fallback unless policy permits.

## Tests

- first upstream refused, second healthy;
- first upstream auth failure, second healthy if policy retries;
- all upstreams fail;
- fallback direct configured;
- fallback direct not configured.

## Acceptance criteria

- Retry/fallback semantics are documented and tested.

---

# Workstream 6: Differential parity tests

## Goal

Compare Eggress and pproxy scheduler/chain/failure behavior where feasible.

## Tests

Extend gated differential suite:

```text
crates/eggress-cli/tests/differential_pproxy.rs
```

Scenarios:

1. round-robin distribution if pproxy supports comparable scheduler;
2. first healthy upstream behavior;
3. multi-hop TCP echo through two proxy hops;
4. refused target failure class;
5. auth failure class;
6. unsupported route behavior.

## Acceptance criteria

- Compatible semantics have differential evidence or documented exception.

---

# Workstream 7: Metrics and observability semantics

## Goal

Ensure scheduler/chain/failure behavior is observable without leaking secrets.

## Requirements

- route decision counters reflect selected/rejected/fallback outcomes;
- upstream open success/failure counters count attempts, not sessions;
- failure reasons remain bounded;
- no target/client/credential labels;
- retry attempts are countable if retry implemented;
- final session report carries rule/group/upstream metadata where available.

## Tests

Extend observability tests for:

- retry attempts counted;
- fallback direct counted as direct fallback;
- all-upstream-failed increments upstream failures;
- no raw error labels.

## Acceptance criteria

- Observability reflects actual semantics.

---

# Workstream 8: Documentation and compatibility matrix

## Required docs

Update or create:

```text
docs/FAILURE_SEMANTICS.md
docs/PARITY_MATRIX.md
docs/PPROXY_PARITY_SPEC.md
docs/METRICS.md
docs/OPERATIONS.md
docs/PPROXY_MIGRATION.md
README.md
```

## Required content

- scheduler behavior table;
- retry/fallback behavior table;
- multi-hop supported combinations;
- failure mapping table;
- differential coverage notes;
- intentional non-parity notes.

---

# Recommended commit sequence

1. Scheduler/failure audit docs.
2. Routing scheduler parity tests and corrections.
3. Multi-hop TCP runtime tests and fixes.
4. Failure semantics docs and tests.
5. Retry/fallback behavior corrections.
6. Observability semantics tests.
7. Differential parity tests.
8. Docs and completion record.

---

# Required verification

```bash
cargo fmt --all -- --check
cargo test -p eggress-routing scheduler
cargo test -p eggress-runtime scheduler_runtime
cargo test -p eggress-runtime multihop_tcp
cargo test -p eggress-runtime failure_semantics
cargo test -p eggress-runtime observability
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Optional/gated:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored scheduler
```

---

# Definition of done

Phase 12 is complete only when:

1. Scheduler semantics are audited against pproxy.
2. Supported scheduler behavior is tested.
3. Multi-hop TCP chains for supported protocols are tested.
4. Failure mapping is documented and tested.
5. Retry/fallback behavior is explicit and bounded.
6. Observability reflects route/upstream attempts accurately.
7. Differential tests cover compatible semantics where feasible.
8. Intentional semantic non-parity is documented.
9. No unsupported protocol is promoted.
10. Workspace checks pass locally.

## Completion record

Add:

```text
docs/PHASE_12_SCHEDULER_CHAIN_FAILURE_PARITY_COMPLETION.md
```

Include scheduler decisions, multi-hop matrix, failure mapping table, tests, differential coverage, and blockers for Phase 13 Rust embed API.
