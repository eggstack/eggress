# Post-Parity Corrective and Reduction Roadmap

## Status

**COMPLETE**

Closed by [`POST_PARITY_FINAL_CORRECTIVE_PASS.md`](POST_PARITY_FINAL_CORRECTIVE_PASS.md),
which corrected the remaining aggregate-tier classification and concrete
metrics-lifecycle defects. No additional closure/certification plan is
created.

## Baseline

Planning baseline: `999f61be3feb26a0542df71dc33fa81408b3cd42`.

The bounded `pproxy==2.7.9` compatibility program is closed. This roadmap does **not**
reopen feature parity work. It is a defect-driven post-closure pass for issues
identified in review of current `main`:

1. session metrics can remain permanently elevated when a connection exits during
   handshake/authentication failure or timeout;
2. compatibility tier semantics are duplicated across Rust, Python, and the
   canonical manifest and have already drifted (`--log`, SOCKS BIND);
3. ~~SOCKS5 request parsing is permissive about the reserved byte and one public~~
   ~~encoder silently truncates overlong domain names;~~ **Resolved in Phase 3.**
4. routine verification is now appropriately small, but parity-record machinery
   and parts of release artifact validation remain more elaborate than the
   behavioral guarantees they provide; binary-size work needs to be measurement
   driven rather than another feature-topology redesign.

These are reproducible correctness and maintenance defects, which is consistent
with the repository rule that post-closure compatibility work requires a concrete
defect or explicit scope decision.

## Objective

Finish one bounded quality pass that makes Eggress harder to misreport, easier to
maintain, and no less capable.

The desired end state is:

- every accepted connection is finalized exactly once in metrics regardless of
  handshake outcome;
- one executable compatibility classification path owns tier semantics and Python
  does not maintain a divergent lookup table;
- the canonical manifest and runtime reporter agree on the classifications they
  expose;
- SOCKS5 parsers reject invalid reserved fields and public address encoding never
  silently rewrites a target;
- routine CI remains one Rust smoke workflow plus one Python smoke workflow;
- historical planning/certification prose is not treated as executable product
  truth;
- PyPI release verification keeps the tests that prove installability and ABI
  coverage while removing redundant archive/name bookkeeping where it adds no
  stronger guarantee;
- any additional binary-size change is justified by measured linked-code or
  dependency evidence and preserves the current feature surface.

## Non-goals

This roadmap must not:

- implement SSH, QUIC/HTTP3, SSR, legacy Shadowsocks ciphers/OTA, daemonization,
  general multi-hop UDP, backward TLS, macOS PF recovery, or arbitrary pproxy
  plugin internals;
- claim strict full pproxy Python drop-in parity;
- add a new parity percentage, certification dashboard, generated evidence
  bundle, or source-of-truth document;
- merge crates merely to reduce crate count;
- redesign routing, runtime snapshots, stream abstractions, or protocol
  composition;
- add mandatory external pproxy, soak, fuzz, benchmark, security-audit, or
  third-party interoperability jobs to routine CI;
- change the manual crates.io release policy;
- remove the PyPI multi-platform wheel build required to deliver the Python
  package;
- introduce a new allocator, nightly `build-std`, UPX, custom linker requirement,
  or dependency rewrite solely for binary size.

## Execution order

Execute the phases in this order.

### Phase 1 — Session metrics lifecycle correctness

Plan: `POST_PARITY_PHASE_1_SESSION_METRICS_LIFECYCLE.md`

**COMPLETED.** Fix the concrete production-observability defect first. The phase must create one
finalization path for successful and failed sessions and prove that active,
total, and failure counters remain balanced after authentication failure,
protocol failure, timeout, route failure, relay failure, and success.

### Phase 2 — Compatibility taxonomy single source

Plan: `POST_PARITY_PHASE_2_COMPATIBILITY_TAXONOMY_SINGLE_SOURCE.md`

**COMPLETED.** Remove duplicated classification logic that allowed the canonical manifest,
Rust reporter, and Python reporter to disagree. Correct the known `--log` and
SOCKS BIND drift and make aggregate classification semantics explicit.

### Phase 3 — SOCKS5 protocol correctness

Plan: `POST_PARITY_PHASE_3_SOCKS5_PROTOCOL_CORRECTNESS.md`

**COMPLETED.** Tighten RFC-required reserved-field handling and remove silent address
truncation. Keep this strictly at the codec/handshake boundary.

### Phase 4 — Verification and size reduction

Plan: `POST_PARITY_PHASE_4_VERIFICATION_AND_SIZE_REDUCTION.md`

**COMPLETED.** Reduce low-value verification duplication, keep the current smoke CI floor, and
measure release artifacts before changing build topology. Binary-size changes
are conditional on evidence and must stop if the available gain is marginal or
requires architecture churn.

## Cross-phase rules

### Behavioral evidence outranks planning prose

The active compatibility contract remains:

- `docs/parity/pproxy_capability_manifest.toml`;
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`;
- executable runtime, Python, and differential tests.

Plans are handoff records only. Do not create tests that require a phase plan or
completion document to contain a particular sentence, commit hash, command
transcript, or implementation summary.

### Keep routine CI small

The expected routine hosted checks remain:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

and the existing path-scoped Python 3.12 smoke workflow.

Only add a lean-build compile check if Phase 4 confirms it is cheap and directly
protects a documented supported build command. Do not turn specialized
certification suites into mandatory jobs.

### No parity inflation

A fix may demote a classification when current behavior is weaker than the
manifest. Correctness of the compatibility report is more important than
preserving a favorable label.

### No speculative size work

Before any binary-size code change, record:

```bash
cargo build -p eggress-cli --release
cargo build -p eggress-cli --profile release-cli-small
cargo build -p eggress-cli --release --no-default-features --features common
cargo tree -p eggress-cli -e normal
cargo tree -d
```

If `cargo-bloat` is available locally, record:

```bash
cargo bloat -p eggress-cli --release --bin eggress --crates
cargo bloat -p eggress-cli --release --bin pproxy --crates
```

Do not add `cargo-bloat` as a required repository dependency or CI tool.

## Roadmap acceptance criteria

This roadmap is complete only when all of the following are true:

1. Every phase plan is implemented or explicitly closed as unnecessary based on
   the phase's documented stop conditions.
2. Failed handshakes cannot leave `eggress_connections_active` elevated after the
   session has ended.
3. Successful and failed sessions pass through one metrics-finalization contract;
   tests demonstrate balanced active/total/failure counters.
4. `--log` has the same compatibility tier in the canonical manifest, Rust
   reporter, Python reporter, and maintained human matrix.
5. SOCKS4/SOCKS5 BIND classifications no longer diverge between the canonical
   contract and Python compatibility reporting.
6. Python no longer owns an independent hand-maintained compatibility taxonomy
   when the native compatibility layer can supply the classification.
7. Aggregate compatibility classification, if retained, has a documented and
   tested severity order in which a material compatibility warning cannot be
   hidden by a better `native_equivalent` result.
8. SOCKS5 TCP request parsing rejects a non-zero RSV byte deterministically.
9. Public SOCKS5 address encoding either rejects an overlong domain or makes
   overlength construction impossible; it never silently truncates the target.
10. Routine Rust and Python smoke CI remain no broader than necessary for the
    changed surfaces.
11. No external pproxy oracle, soak, fuzz, benchmark, audit, or release evidence
    suite becomes a routine push gate.
12. Release workflow simplification, if performed, retains five wheel targets,
    the sdist, install-smoke coverage, Python stable-ABI boundary coverage, and
    OIDC publication.
13. Any binary-size change includes before/after byte counts from isolated release
    builds and preserves default/full behavior.
14. If no material size reduction is available without architecture churn, the
    phase records that result and makes no speculative rewrite.
15. The final broad gate passes:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

16. For Python-facing changes, a clean wheel build/install and:

```bash
python -m pytest python/tests tests/compat -q
```

passes.
17. The canonical manifest validator passes with zero hard errors after any
    compatibility classification changes.
18. No new user-facing feature or parity claim is introduced by this roadmap.

## Handoff rule

Implementers should update the relevant active documentation only when observable
behavior or maintained policy changes. Do not append implementation transcripts
to these plans and do not create another closure/certification plan merely to
record that the work passed. A commit/PR summary naming the focused tests is
sufficient.
