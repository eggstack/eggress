# Phase 7 Detailed Plan: Formal `pproxy` Parity Specification

## Purpose

Phase 7 creates the compatibility contract for true `pproxy` parity. Do not implement major new protocol behavior in this phase. The output is a precise, testable specification describing what Python `pproxy` does, what Eggress already matches, what Eggress must still implement, and what Eggress will intentionally not replicate.

True parity work should not proceed by guesswork. Every later phase depends on this spec and matrix.

---

# Current baseline

Eggress is post-Phase-6 hardening. Supported baseline:

- SOCKS5 inbound TCP CONNECT;
- SOCKS5 inbound UDP ASSOCIATE direct;
- one-hop SOCKS5 UDP upstream relay;
- HTTP CONNECT inbound;
- HTTP forward-proxy path;
- SOCKS4/SOCKS4a inbound;
- direct TCP relay;
- HTTP CONNECT upstream;
- SOCKS4/SOCKS4a upstream;
- SOCKS5 upstream;
- Trojan TCP upstream;
- metrics/admin/reload/security hardening.

Known gaps:

- Shadowsocks TCP is experimental/partial, not parity;
- Shadowsocks UDP is experimental/non-interop, not parity;
- full pproxy CLI grammar is not modeled;
- full pproxy URI grammar is not modeled;
- pproxy scheduler/fallback semantics need comparison;
- Python bindings are not started.

---

# Non-goals

Do not implement:

- Shadowsocks stream encryption;
- new protocol handlers;
- pproxy-compatible CLI execution;
- Python bindings;
- scheduler changes;
- large runtime refactors.

This phase is spec, fixtures, matrix, and reusable test harness only.

---

# Workstream 1: Inventory Python `pproxy`

## Goal

Capture the relevant `pproxy` behavior in a durable local document.

## Required output

Create:

```text
docs/PPROXY_PARITY_SPEC.md
```

Required sections:

1. Scope and version of `pproxy` inspected.
2. Local/listener protocols.
3. Remote/upstream protocols.
4. Supported URI schemes and examples.
5. Chaining syntax.
6. Scheduler/load-balancing behavior.
7. Authentication behavior.
8. UDP behavior.
9. Encryption protocol behavior.
10. CLI flags and common invocation forms.
11. Python-library usage surface, if applicable.
12. Error and failure behavior relevant to clients.
13. Behaviors Eggress will intentionally reject.

## Investigation method

Use all applicable sources:

- local source inspection of Python `pproxy` if vendored or installed;
- upstream docs/README examples;
- local black-box probes through differential tests;
- existing Eggress `docs/PARITY_MATRIX.md` and test files.

## Acceptance criteria

- Spec names the `pproxy` version or commit inspected.
- Spec distinguishes documented behavior from observed behavior.
- Ambiguous behavior is marked `needs-probe`, not assumed.

---

# Workstream 2: Define compatibility tiers

## Goal

Prevent future overclaiming.

## Required tiers

Add these definitions to `docs/PPROXY_PARITY_SPEC.md` and `docs/PARITY_MATRIX.md`:

- **Compatible**: Eggress behavior matches pproxy for tested scenarios.
- **Supported**: Eggress supports the feature, but pproxy equivalence is not claimed.
- **Partial**: usable subset exists but not enough for compatibility.
- **Experimental**: code exists but no compatibility/stability promise.
- **Intentional non-parity**: deliberately not replicated with rationale.
- **Unsupported**: not implemented.

## Acceptance criteria

- Every row in the parity matrix uses one of these tiers.
- No row says compatible without a runtime or differential test reference.

---

# Workstream 3: Expand parity matrix

## Required output

Update:

```text
docs/PARITY_MATRIX.md
```

Required columns:

```markdown
| Feature | pproxy behavior | Eggress behavior | Tier | Runtime test | Differential test | Notes |
```

Minimum feature categories:

- inbound TCP protocols;
- inbound UDP protocols;
- upstream TCP protocols;
- upstream UDP protocols;
- chain behavior;
- scheduler behavior;
- auth behavior;
- CLI compatibility;
- URI compatibility;
- config/reload behavior;
- Python library/bindings behavior.

## Acceptance criteria

- Matrix can drive Phases 8–12 directly.
- Unknowns are explicitly marked `needs-probe`.

---

# Workstream 4: Normalize differential harness primitives

## Goal

Prepare the test infrastructure needed for true compatibility claims.

## Target file

Refactor or extend:

```text
crates/eggress-cli/tests/differential_pproxy.rs
```

## Required helper primitives

- `start_tcp_echo()`;
- `start_udp_echo()`;
- `start_eggress_from_toml()`;
- `start_pproxy(args)`;
- `socks5_connect()`;
- `http_connect()`;
- `socks5_udp_associate()`;
- `compare_tcp_echo()`;
- `compare_udp_echo()`;
- `assert_coarse_failure_equivalent()`;
- cleanup process/task guard.

All tests remain gated:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored
```

If Python or `pproxy` is absent, tests skip or fail with a clear message only under the explicit gate.

## Acceptance criteria

- Existing differential tests still pass or skip cleanly.
- New helper API reduces duplication.
- No public-internet dependency.

---

# Workstream 5: Add black-box probe tests for unclear behavior

## Goal

Use local probes to clarify ambiguous pproxy behavior before implementation.

## Candidate probes

- HTTP CONNECT status codes for refused upstream.
- SOCKS5 reply code for refused target.
- UDP ASSOCIATE lifetime when TCP control closes.
- SOCKS5 auth success/failure shape.
- Chained upstream failure behavior.
- Scheduler behavior with one healthy and one unhealthy upstream.
- URI parsing of credentials and chained protocols.

## Output

Record findings in:

```text
docs/PPROXY_PARITY_SPEC.md
```

Do not force Eggress to mimic unsafe or malformed behavior yet.

## Acceptance criteria

- Ambiguous rows in parity matrix get a documented `observed` note or remain `needs-probe` with reason.

---

# Workstream 6: Intentional non-parity list

## Goal

Document behavior Eggress will not replicate.

## Required output

In `docs/PPROXY_PARITY_SPEC.md`, add:

```markdown
## Intentional non-parity
```

Candidate items:

- unsafe transparent/system behavior;
- weak legacy ciphers if not supported;
- malformed input leniency;
- public DNS/internet-dependent tests;
- raw target/client metric labels;
- insecure TLS defaults;
- unbounded scheduler labels;
- ambiguous URI shorthands if they conflict with Eggress safety.

Each item needs rationale and user-facing error behavior.

---

# Recommended commit sequence

1. Add `docs/PPROXY_PARITY_SPEC.md` skeleton and tier definitions.
2. Expand `docs/PARITY_MATRIX.md` with tier/test columns.
3. Refactor differential test helpers.
4. Add black-box probes for known ambiguous pproxy behavior.
5. Fill intentional non-parity section.
6. Add completion record.

---

# Required verification

```bash
cargo fmt --all -- --check
cargo test -p eggress-cli --test differential_pproxy -- --ignored # with gate if available
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

If pproxy is unavailable, record skip behavior.

---

# Definition of done

Phase 7 is complete only when:

1. `docs/PPROXY_PARITY_SPEC.md` exists and names inspected pproxy version/source.
2. Compatibility tiers are defined.
3. `docs/PARITY_MATRIX.md` uses the tier taxonomy.
4. Every compatible claim has a runtime or differential test reference.
5. Differential harness primitives are reusable.
6. Ambiguous pproxy behavior is probed or marked `needs-probe`.
7. Intentional non-parity is documented.
8. No new unsupported protocol is promoted.
9. Normal workspace checks pass locally.

## Completion record

Add:

```text
docs/PHASE_7_PPROXY_PARITY_SPEC_COMPLETION.md
```

Include commit list, inspected pproxy version, unresolved `needs-probe` items, and next-phase blockers.
