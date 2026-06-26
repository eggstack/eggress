# Phase 6 Final Polish Plan: Upstream Metrics Wiring and Closure Cleanup

## Purpose

Phase 6 is effectively closed. The hardening pass added property tests, fuzz harnesses, runtime lifecycle invariants, observability tests, security review, dependency guardrails, pproxy differential tests, benches, load tests, and a comprehensive documentation set. A follow-up review strengthened observability/lifecycle semantics and clarified dependency and CI limitations.

This final polish plan closes the last notable item: upstream-open metrics are registered but not wired into TCP upstream-open call sites. It also updates the relevant observability tests and docs so the final completion record is fully aligned with executable behavior.

This plan is deliberately narrow. Do not add new protocols, new routing behavior, Shadowsocks support, new benchmark suites, or broader Phase 7 scope.

---

# Current known item

The Phase 6 review documented this gap:

- `eggress_upstream_open_total` is registered in the metrics registry with HELP/TYPE output but is not called from the TCP chain executor.
- `eggress_upstream_open_failures_total` has the same gap.
- HTTP CONNECT, SOCKS5, SOCKS4/SOCKS4a, Trojan, and experimental Shadowsocks handlers do not currently increment these counters after successful/failed upstream open attempts.
- Observability tests currently assert HELP/TYPE registration for those metrics, not value increments.

This is now the only meaningful polish item before closing Phase 6 completely.

---

# Non-goals

Do not implement:

- new proxy protocols;
- Shadowsocks stream encryption;
- Shadowsocks UDP interoperability;
- multi-hop UDP;
- QUIC/MASQUE/CONNECT-UDP;
- transparent proxying;
- new admin endpoints;
- new metrics beyond upstream-open success/failure wiring;
- native TLS/OpenSSL;
- unsafe Rust;
- hosted CI billing fixes.

---

# Workstream 1: Locate existing upstream-open metric API

## Goal

Use the existing metrics API rather than inventing new metric names.

## Tasks

1. Inspect `crates/eggress-metrics/src/lib.rs`.
2. Identify the existing APIs, expected labels, and reason/outcome strings for:
   - `record_upstream_open`;
   - `record_upstream_failure`.
3. Confirm Prometheus output names. Note that `prometheus-client` may append `_total` to registered counters; tests should use existing helper behavior from `observability.rs`.
4. Confirm label cardinality is bounded.

## Acceptance criteria

- No new metric family is introduced unless the existing one is unusable.
- Label values remain bounded: protocol, outcome, failure reason, possibly upstream/group ID only if already supported and bounded.
- No client IP, target host, username, password, payload, or arbitrary error string is used as a label.

---

# Workstream 2: Decide the correct call site

## Goal

Record upstream-open metrics exactly once per upstream open attempt.

## Preferred call site

The preferred call site is the TCP chain execution layer, not each protocol crate. The protocol crates should remain pure protocol implementations and not depend on metrics.

Likely places to inspect:

```text
crates/eggress-server/src/execute.rs
crates/eggress-core/src/chain.rs
crates/eggress-routing/src/*
```

The final call site should have access to:

- selected upstream ID or protocol;
- group ID if available;
- protocol kind;
- success/failure result;
- failure category/reason;
- metrics registry handle.

## Design rule

Record:

- success only after the upstream connection/handshake has completed and the relay is ready to begin;
- failure when an upstream open attempt fails before relay begins;
- no metric for direct routing;
- no metric for rejected route decisions; those belong to route-decision metrics;
- no duplicate metric for the same upstream attempt.

## Acceptance criteria

- A successful HTTP/SOCKS5/SOCKS4/Trojan upstream open increments success once.
- A failed upstream open increments failure once.
- Direct routes do not increment upstream-open counters.

---

# Workstream 3: Wire successful upstream-open metrics

## Goal

Increment `eggress_upstream_open_total` for successful TCP upstream opens.

## Required protocols

At minimum:

- HTTP CONNECT upstream;
- SOCKS4/SOCKS4a upstream;
- SOCKS5 TCP upstream;
- Trojan TCP upstream.

Experimental Shadowsocks:

- If Shadowsocks route execution is disabled by capability/config, no success metric is needed.
- If an experimental handler can still be invoked by explicit opt-in, record it as protocol `shadowsocks` but do not mark it supported in docs.

## Suggested outcome labels

Use existing label conventions. If not already defined, use stable low-cardinality strings:

- protocol: `http`, `socks4`, `socks5`, `trojan`, `shadowsocks`;
- outcome: `success`.

Do not include target host or endpoint address unless the existing API already includes a bounded upstream ID.

## Required tests

Extend `crates/eggress-runtime/tests/observability.rs`:

1. HTTP upstream TCP echo increments upstream-open success.
2. SOCKS5 upstream TCP echo increments upstream-open success.
3. SOCKS4 upstream TCP echo increments upstream-open success.
4. Trojan upstream TCP echo increments upstream-open success if the runtime Trojan path is already tested and cheap enough to reuse.

If Trojan runtime fixture is too heavy, add at least protocol-level unit coverage and document why runtime value assertion is deferred. Preferred: runtime test.

## Acceptance criteria

- Observability tests assert metric **values**, not only HELP/TYPE registration.

---

# Workstream 4: Wire failed upstream-open metrics

## Goal

Increment `eggress_upstream_open_failures_total` for failed upstream open attempts.

## Failure cases to cover

At minimum:

- TCP connect refused or immediately closed upstream;
- HTTP CONNECT non-2xx response;
- SOCKS5 upstream method rejection or connect failure;
- SOCKS4 non-success reply;
- Trojan TLS/connect/auth failure if test fixture exists.

Do not overfit to one exact error string. Map to bounded reason strings.

Suggested reason labels:

- `tcp_connect`;
- `handshake`;
- `auth_failed`;
- `proxy_rejected`;
- `timeout`;
- `io`;
- `unsupported_protocol`.

Use whatever taxonomy already exists if present.

## Required tests

Extend `observability.rs` or add `upstream_metrics.rs`:

1. Refusing upstream increments failure counter.
2. HTTP 407/403 increments failure counter with bounded reason.
3. SOCKS5 method rejection increments failure counter if fixture exists.
4. SOCKS4 reject reply increments failure counter if fixture exists.

Also assert:

- no raw error text appears as a metric label;
- no target host/client IP/credential appears as a metric label;
- failure counter is not incremented for direct routes.

## Acceptance criteria

- Upstream failures are visible in `/metrics` with bounded labels.

---

# Workstream 5: Strengthen observability tests

## Goal

Promote upstream-open observability tests from registration checks to value checks.

## Tasks

1. Locate current observability tests that assert HELP/TYPE registration for `eggress_upstream_open_total` and `eggress_upstream_open_failures_total`.
2. Replace or augment those assertions with value-based checks.
3. Reuse the existing helper functions:
   - `metric_value`;
   - `metric_value_with_labels`;
   - `label_keys`.
4. Account for `prometheus-client` counter name suffix behavior consistently.
5. Keep tests deterministic using polling where metrics flush/update is asynchronous, if applicable.

## Required assertions

- success value increases after successful upstream relay;
- failure value increases after failed upstream open;
- labels are bounded;
- no secret/high-cardinality label keys appear;
- direct routes do not increment upstream-open counters.

## Acceptance criteria

- The documented metrics wiring gap is gone.
- Tests prove values, not mere registration.

---

# Workstream 6: Update docs and completion records

## Required docs

Update:

```text
docs/PHASE_6_HARDENING_COMPLETION.md
docs/METRICS.md
docs/SECURITY_REVIEW.md
```

Optional if relevant:

```text
docs/RELEASE_READINESS.md
docs/PARITY_MATRIX.md
```

## Required changes

1. Remove the “Known Production Wiring Gap” section for upstream-open metrics, or replace it with a closure note.
2. Add a note that upstream-open success/failure counters are live and value-tested.
3. In `docs/METRICS.md`, document:
   - metric names;
   - label keys;
   - allowed label values;
   - forbidden labels;
   - example output.
4. In `docs/SECURITY_REVIEW.md`, confirm upstream failure metrics use bounded reason labels and do not expose raw error strings or credentials.
5. In `docs/PHASE_6_HARDENING_COMPLETION.md`, add a final polish record with commit SHA and verification commands.

## Acceptance criteria

- Docs accurately state that upstream-open metrics are wired.
- No doc still says these counters are registration-only.

---

# Workstream 7: Final local verification

## Required commands

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Focused checks:

```bash
cargo test -p eggress-runtime observability
cargo test -p eggress-runtime lifecycle
cargo test -p eggress-runtime upstream_protocols
cargo test -p eggress-metrics
```

Optional:

```bash
cargo bench --no-run
cargo test -- --ignored load
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
```

## Hosted CI

Do not claim hosted CI passed unless status contexts or workflow runs are visible. If GitHub Actions remains billing-blocked, keep the existing local-verification language.

## Acceptance criteria

- Local verification commands are recorded in the completion doc.
- Hosted CI status is represented honestly.

---

# Recommended commit sequence

## Commit 1: Wire upstream-open metrics

- Add success/failure metric calls at the TCP upstream-open boundary.
- Keep protocol crates metrics-free.
- Use bounded labels/reasons.

## Commit 2: Promote observability tests to value assertions

- Add success/failure upstream-open metric tests.
- Assert direct routes do not increment upstream-open counters.
- Assert no secrets/high-cardinality labels.

## Commit 3: Documentation and completion cleanup

- Update metrics docs.
- Update security review.
- Update Phase 6 completion record.
- Record local verification.

If the implementation is small, commits 1 and 2 may be combined, but keep docs separate so review is clean.

---

# Definition of done

This final polish pass is complete only when:

1. `eggress_upstream_open_total` increments for successful TCP upstream opens.
2. `eggress_upstream_open_failures_total` increments for failed TCP upstream opens.
3. Metrics are recorded exactly once per upstream open attempt.
4. Direct routes do not increment upstream-open counters.
5. Success/failure labels are bounded and do not include client, target, credentials, payload, or raw error strings.
6. Observability tests assert upstream-open counter values.
7. Existing privacy/label tests still pass.
8. Docs no longer describe upstream-open metrics as registration-only.
9. Phase 6 completion record includes this final polish closure.
10. Normal local verification passes.
11. Hosted CI status remains honestly documented if still unavailable.
12. No new feature scope, unsupported protocol promotion, native dependency, or unsafe Rust is introduced.

## Closure note template

Add to `docs/PHASE_6_HARDENING_COMPLETION.md`:

```markdown
## Final polish closure

Implemented by commit(s):

- `<sha>` — upstream-open success/failure metrics wired into TCP upstream-open boundary
- `<sha>` — observability value assertions and docs cleanup

The previously documented upstream-open metrics wiring gap is closed. Success and failure counters are live, value-tested, and use bounded labels. Hosted CI remains `<visible/unavailable>`; local verification `<passed/failed>` on `<date>`.
```
