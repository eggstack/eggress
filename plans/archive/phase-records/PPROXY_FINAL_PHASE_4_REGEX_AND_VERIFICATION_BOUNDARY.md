# pproxy Final Phase 4 — Regex and Verification Boundary

## Status

**IMPLEMENTED**

## Parent roadmap

[`PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md`](PPROXY_FINAL_CORRECTIVE_CLOSURE_ROADMAP.md)

## Objective

Codify the intended trust/resource boundary for pproxy-compatible regular expressions and rule files, preserve the useful existing bounds, and ensure the repository's verification apparatus remains proportionate: routine smoke checks stay small while expensive oracle/certification/fuzz/performance work remains specialized and opt-in.

This phase is primarily clarification and reduction. It must not introduce a regex sandbox, worker process, generalized policy engine, or new CI matrix.

## Implementation summary

### Threat-model decision

Production call-site discovery confirmed:

- **No unauthenticated network client can supply an arbitrary compatibility regex pattern at runtime.** All patterns originate from CLI arguments, configuration files, or rule files controlled by the local operator.
- **Regexes are compiled only during CLI translation and config load.** `CompatRegex::compile()` is called from `PproxyRuleFile::load()` (rule file parsing) and `compile_block_pattern()` (block flag validation), both of which execute at translation time. The runtime routing engine uses `regex::Regex` directly.
- **Strings matched at connection time are bounded destination attributes.** The routing engine matches against `request.target.host` (hostname string) and `request.target.port` (decimal port string), not arbitrary network payload.
- **Rule files are not re-read on reload.** `PproxyRuleFile` is used only during pproxy CLI translation. On SIGHUP, the runtime re-reads the TOML config, not the original pproxy rule file.
- **Native Eggress routing rules do not use the fancy fallback.** `host_regex` and `destination_port_regex` in TOML config are compiled with `regex::Regex` directly. The `fancy_regex` backend is confined to pproxy compatibility translation validation.
- **`compile_fancy` and `CompatRegex::is_match` have zero production callers.** They exist for test coverage and potential future use.

No contrary remote-controlled pattern path was found. The trusted-configuration model holds.

### Changes made

**Workstream A — Trust model codified:**
- `docs/architecture/pproxy-compat.md`: Added "Regex and rule trust model" section documenting the input-trust boundary, backend selection, bounds enforcement, and the confinement of fancy_regex to compatibility translation.
- `README.md`: Refined design goal from "Resource-bounded hostile-input handling" to "Resource-bounded hostile network input; trusted operator configuration may use compatibility features with documented computational cost".
- `docs/architecture/routing.md`: Clarified that native rules use `regex::Regex` while pproxy compatibility patterns may use `CompatRegex` with fancy fallback.

**Workstream B — Tests preserved and extended:**
- `regex_compat.rs`: Added `compile_pattern_at_length_boundary` test verifying exactly 4096-byte patterns compile successfully. Added `fancy_regex_backtrack_limit_applied` test verifying fancy patterns with backreferences work correctly.

**Workstream C — Confirmed:**
- No compatibility regex is evaluated against arbitrarily large network payload. Matching is confined to destination hostname and port strings.

**Workstream D — Backtrack limit explicitly configured:**
- `regex_compat.rs`: Defined `FANCY_REGEX_BACKTRACK_LIMIT = 1_000_000` constant matching the fancy_regex 0.14 default. Changed `fancy_regex::Regex::new()` calls to `fancy_regex::RegexBuilder::new(pattern).backtrack_limit(FANCY_REGEX_BACKTRACK_LIMIT).build()` in both `compile()` (fancy fallback path) and `compile_fancy()`. Replaced the misleading `fancy_regex_backtrack_limit_applied` test with `fancy_regex_backtrack_limit_exhaustion` (proves limit enforcement via a known pathological pattern), `fancy_regex_explicit_limit_matches_default` (confirms constant matches dependency default), and `fancy_regex_backtrack_limit_is_configured` (guards the constant value). The existing `rulefile_max_entries_enforced` test covers the 10,001-entry overflow boundary.

**Workstream E — Verification docs cleaned:**
- `docs/DIFFERENTIAL_TESTING.md`: Replaced stale "Phase 41" labels with current "Differential Parity Harness" name (3 occurrences).

**Workstream F — Orphaned scripts:**
- 23 scripts are orphaned from active use (referenced only by historical plans). Per plan policy, they are retained as working specialized infrastructure but are not treated as routine ceremony. No active tests, workflows, or release processes reference them.

**Workstream G — CI non-regression:**
- Confirmed hosted CI remains the lean topology: one Ubuntu Rust smoke job, one path-scoped Python smoke job, one release-only publish workflow. No changes needed.

**Workstream H — Performance:**
- No regex compilation behavior changed in a way that would affect performance. The `RegexBuilder` path is equivalent to `Regex::new()` for patterns that use the fancy backend.

### Tests run

```bash
cargo test -p eggress-pproxy-compat regex   # 29 passed
cargo test -p eggress-pproxy-compat rule    # 21 passed
cargo fmt --all -- --check                  # clean
cargo clippy --workspace --all-targets -- -D warnings  # clean
cargo test --workspace --locked             # 2491 passed, 146 ignored
```

### Verification machinery actually removed/demoted

None. The orphaned strict/certification scripts are retained per the plan's conservative default. The Phase 41 labels were the only stale references corrected.

## Current regex baseline

`crates/eggress-pproxy-compat/src/regex_compat.rs` currently provides:

- fast-path compilation with Rust `regex`;
- fallback to `fancy_regex` for look-around/backreferences and other compatibility constructs;
- `MAX_PATTERN_LEN = 4096` bytes;
- `MAX_RULE_ENTRIES = 10_000` per rule file;
- explicit compile/match error types;
- diagnostics indicating use of the fancy backend.

The unresolved issue is not lack of any bound. It is that `fancy_regex` is a backtracking engine and the compatibility matcher has no independent match-time timeout. A hostile operator-supplied pattern can therefore consume substantial CPU even though pattern length and rule count are bounded.

For the current product, pproxy block/rule patterns originate from command-line/configuration/rule-file input controlled by the local operator. They are not directly supplied by an unauthenticated network peer. That distinction should be explicit.

## Governing rules

1. Treat compatibility regex/rule definitions as **trusted local/operator configuration**, not hostile network input, unless implementation discovery finds an actual remote path that contradicts this assumption.
2. Keep hostile **network protocol input** resource-bounded through the existing parser/relay limits. Do not weaken those protections.
3. Preserve the fast `regex` backend as the default path.
4. Preserve `fancy_regex` only for the compatibility constructs needed by the pproxy target; do not expand syntax support speculatively.
5. Preserve the existing 4096-byte pattern bound and 10,000-entry rule-file bound unless measurements/tests justify a smaller limit without breaking realistic compatibility.
6. Unsupported/invalid patterns must fail at translation/load time where possible.
7. Do not promise match-time hard real-time guarantees for the fancy backend if the implementation does not provide them.
8. Do not add a regex subprocess, thread-per-match timeout system, WASM sandbox, separate service, custom backtracking engine, or unsafe cancellation mechanism for this threat model.
9. Verification simplification must remove ceremony/duplication, not high-value correctness tests.
10. Hosted CI must remain the current lean Rust/Python smoke shape.
11. Specialized oracle, interoperability, fuzz, soak, benchmark, audit, and release checks remain trigger-based rather than routine gates.

## Required discovery before edits

Trace all production uses of `CompatRegex`, `compile_fancy`, and `PproxyRuleFile`.

Answer explicitly in the implementation summary:

- Can any unauthenticated network client supply an arbitrary compatibility regex pattern at runtime?
- Are regexes compiled only during CLI translation/config load/reload?
- What strings are matched at connection time (hostname, port text, other bounded fields)?
- Are rule files reread/recompiled on reload, and if so who controls the file path/content?
- Do native Eggress routing rules use the same fancy fallback, or is the backtracking engine confined to pproxy compatibility?

If discovery reveals a genuine remote-controlled arbitrary-pattern path, stop this plan before documenting the trusted-config assumption and escalate the threat-model finding in the implementation summary. Do not silently continue under the wrong model.

## Workstream A — Codify the trust model

Update the smallest appropriate active documents, likely:

- `docs/architecture/pproxy-compat.md`
- `docs/architecture/routing.md` if needed to distinguish native rules from compatibility rules
- `docs/PPROXY_PARITY_SPEC.md` only if it remains an active reference after Phase 2; otherwise do not revive it
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` only if a short caveat is useful to users
- `AGENTS.md` only if the repository-wide hostile-input statement needs precise scope

Required statement, in substance:

```text
pproxy compatibility regex/rule definitions are trusted local configuration.
The fast regex backend is preferred. Python-like constructs may select a
backtracking fancy_regex backend. Pattern length and rule count are bounded,
but fancy matching does not provide a hard per-match timeout; operators must
not load untrusted rule sets.
```

Do not describe this as a vulnerability exemption. It is an explicit input-trust boundary.

If the README/design goals say all hostile input is universally bounded, refine the wording so network-facing parsers remain resource-bounded while trusted configuration may use compatibility features with documented computational cost.

## Workstream B — Preserve and test the existing bounds

Ensure focused tests cover:

- pattern length exactly at/under the allowed boundary succeeds when syntactically valid;
- pattern length over 4096 fails with `PatternTooLong` before either backend compilation;
- rule files stop at 10,000 accepted entries and emit/fail according to the established behavior for excess entries;
- ordinary patterns select `RegexBackend::Fast`;
- a representative look-around/backreference pattern selects `RegexBackend::Fancy`;
- unsupported syntax fails with a structured compile diagnostic rather than falling through to an unvalidated string matcher.

Do not add huge slow tests to routine CI. The boundary test can construct strings/files efficiently and should complete quickly.

## Workstream C — Keep fancy-regex matching off remotely supplied arbitrary text where architecture already allows it

The primary route matcher is expected to evaluate configured patterns against bounded destination attributes such as hostnames/port strings. Confirm this assumption.

If an existing compatibility regex is applied to arbitrarily large client payloads, HTTP bodies, decrypted tunnel streams, or other unbounded network data, narrow that use to the intended destination/routing field if doing so preserves pproxy semantics. This is a correctness/resource-bound fix and is allowed.

Do not add arbitrary truncation of hostnames/targets that changes valid protocol behavior; rely on existing protocol/address bounds where appropriate.

## Workstream D — Do not over-engineer match-time cancellation

Evaluate whether the current `fancy_regex` version exposes a simple built-in backtracking limit or other bounded option already usable without architecture changes. If and only if such a supported option exists and can be enabled with a tiny local change while preserving required pproxy constructs, measure/test it.

Otherwise, explicitly record the decision to retain the current backend under the trusted-configuration model.

Rejected by default:

- spawning a process per rule set;
- running every match on a blocking worker with timeout and abandoning threads;
- instrumenting custom cancellation inside third-party regex internals;
- switching to a different large regex engine solely for theoretical timeout support;
- removing `fancy_regex` and thereby dropping currently supported pproxy constructs;
- accepting regexes from remote admin endpoints without a separate product decision.

## Workstream E — Verification apparatus audit

Use `docs/CI_STATUS.md` as policy authority. Inspect repository scripts/docs/workflows for places that still imply any of the following are mandatory for ordinary implementation changes:

- full pproxy certification;
- regenerated parity evidence bundles;
- strict-manifest synchronization;
- cross-platform matrices;
- benchmarks or size checks;
- cargo audit/deny on unrelated changes;
- fuzz/soak runs;
- external pproxy installation.

Correct stale instructions to the current trigger-based policy. Prefer deleting stale commands from active workflow instructions or labeling them specialized rather than creating more policy documents.

Do not delete the underlying specialized tests/harnesses merely because they are not routine gates.

## Workstream F — Testkit/oracle simplification stop conditions

The repository contains substantial oracle/testkit infrastructure. Do a bounded ownership check only for obviously orphaned machinery discovered during the documentation audit.

A component may be removed only if all are true:

1. no active test, script, documented specialized workflow, or release process calls it;
2. it duplicates capability already provided by a simpler retained path;
3. removal does not reduce differential coverage needed for Phase 5;
4. references can be removed cleanly in the same change.

Do **not** refactor `eggress-testkit` or the oracle schema merely to reduce line count. The default action is to retain working specialized infrastructure but stop treating it as routine ceremony.

## Workstream G — Hosted CI non-regression

No new workflow should be needed.

Verify that ordinary hosted CI remains:

```text
.github/workflows/ci.yml
  one Ubuntu Rust smoke job: fmt + clippy + workspace tests

.github/workflows/python-test.yml
  one path-scoped Ubuntu/Python smoke job

.github/workflows/publish-python.yml
  release-only Python artifact build/publish
```

Do not add fancy-regex stress jobs, oracle jobs, size gates, security matrices, or documentation-generation jobs.

A regex dependency version change, if any, should use the existing dependency-change verification policy (`cargo deny`/`cargo audit` as applicable) but must not make those checks permanent routine workflows.

## Workstream H — Optional focused performance sanity check

If regex implementation behavior changes, run a local informational micro-check comparing a representative fast pattern and representative fancy pattern against realistic destination strings. This may be a unit benchmark or a one-off local measurement.

Do not add a benchmark gate. The purpose is only to catch an accidental order-of-magnitude regression in the ordinary path.

## Verification

Focused Rust tests:

```bash
cargo test -p eggress-pproxy-compat regex
cargo test -p eggress-pproxy-compat rule
```

Run the actual test names/modules found in the crate rather than inventing filters if these filters do not select them.

If routing integration changes:

```bash
cargo test -p eggress-routing
cargo test -p eggress-runtime
```

Final broad gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Python tests are required only if compatibility Python rule helpers or shared behavior changes:

```bash
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests tests/compat -q
```

Run `cargo deny` / `cargo audit` only if dependency versions/features change or release preparation requires them.

## Acceptance criteria

Phase 4 is complete only when all are true:

- production call-site discovery confirms whether compatibility regex patterns are operator-controlled configuration or identifies/escalates any contrary remote-controlled pattern path;
- active documentation explicitly states the trust boundary for pproxy compatibility regex/rule sets;
- the distinction between resource-bounded hostile network input and trusted computationally expensive configuration is clear enough that maintainers do not infer a nonexistent hard regex timeout guarantee;
- `MAX_PATTERN_LEN = 4096` remains enforced before regex compilation unless a deliberately tested compatible bound change is made;
- `MAX_RULE_ENTRIES = 10_000` remains enforced unless a deliberately tested compatible bound change is made;
- focused tests cover pattern-length overflow, rule-entry overflow, fast-backend selection, fancy-backend selection, and structured compile failure;
- no compatibility regex is accidentally evaluated against arbitrarily large network payload data when pproxy semantics only require destination/routing attributes;
- a built-in/simple backend match limit is used only if it exists, is low-complexity, and passes compatibility tests; otherwise the lack of a hard per-match timeout is explicitly documented under the trusted-config model;
- no regex worker process, sandbox service, abandoned-timeout thread scheme, custom regex engine, or new runtime subsystem is added;
- `fancy_regex` is not removed if doing so would reduce the current supported pproxy regex feature set;
- active verification documentation no longer implies specialized oracle/certification/fuzz/benchmark/audit work is mandatory for unrelated routine changes;
- specialized differential/testkit infrastructure remains available where useful for Phase 5 and future changed-surface investigations;
- any deleted verification component satisfies the explicit orphan/duplication stop conditions and has no remaining active references;
- hosted CI remains the current lean Rust/Python smoke topology with no new routine jobs;
- focused and broad workspace tests pass;
- this plan is updated in place to `IMPLEMENTED` with the final threat-model decision, any retained limitations, implementation commit(s), tests run, and any verification machinery actually removed/demoted.