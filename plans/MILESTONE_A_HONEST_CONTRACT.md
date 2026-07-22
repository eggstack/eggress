# Milestone A — Honest pproxy Compatibility Contract

## Status

**REOPENED — corrective pass in progress.** The corrective pass
(`plans/MILESTONES_A_C_CORRECTIVE_PASS.md`) is underway. Manifest integrity and report
generation infrastructure are in place, but paired oracle/candidate behavioral evidence
has not been generated. 81 records still need behavioral validation. This milestone
cannot be closed until the acceptance matrix in the corrective pass plan is fully
satisfied.

## Parent roadmap

`plans/PPROXY_FULL_DROP_IN_ROADMAP.md`

## Objective

Replace the current capability-oriented compatibility claim with a frozen, mechanically validated, behavior-oriented contract against exactly `pproxy==2.7.9`.

Milestone A does not primarily add proxy features. It creates the source of truth that later milestones must satisfy and prevents candidate-only tests, import stubs, native equivalents, or incomplete external evidence from being presented as drop-in compatibility.

## Completion outcome

At milestone completion, the repository must contain:

- a pinned and reproducible pproxy 2.7.9 oracle definition;
- a complete strict compatibility manifest;
- a known-upstream-defects registry;
- paired oracle/candidate observation runners;
- normalized machine-readable observations;
- differential comparators;
- immutable upstream example and test fixtures;
- CI tiers for fast, external, platform, and release differential testing;
- documentation and validator rules that reserve `drop_in` for strict behavioral matches.

No implementation milestone may claim completion without evidence produced by this machinery.

## Scope

### In scope

- compatibility vocabulary and governance;
- oracle provenance and environment reproducibility;
- Python namespace and callable inventory;
- CLI option and process inventory;
- protocol, transport, cipher, plugin, composition, and platform inventory;
- structured observations;
- differential result classification;
- manifest-to-test traceability;
- retained CI evidence;
- upstream test/example import and licensing records;
- release-document consistency policy.

### Out of scope

- implementing missing SSH, QUIC/H3, SSR, or legacy cipher behavior;
- changing the canonical `eggress` API;
- replacing Python structural stubs;
- finalizing asyncio stream adapters;
- declaring any new full-parity capability merely because it has been inventoried.

Implementation gaps discovered here must be recorded and routed to Milestones B through F.

## Existing repository assets to reuse

The repository already contains useful foundations that should be extended rather than discarded:

- `docs/parity/PPROXY_PARITY_SPEC.md`;
- `docs/parity/pproxy_capability_manifest.toml`;
- `docs/parity/PPROXY_PARITY_REPORT.md`;
- `docs/parity/composition_matrix.toml`;
- `tests/compat/pproxy_target.toml`;
- `tests/compat/pproxy_manifest.toml`;
- `scripts/validate_pproxy_parity_manifest.py`;
- `crates/eggress-testkit` oracle, manifest, observation, probe, schema, supervisor, and CI modules;
- Python differential tests under `python/tests`;
- CLI differential and external interoperability tests;
- release evidence generation and release-document consistency checks.

Milestone A must preserve existing subset reports for historical and modern-Eggress purposes while introducing a separate strict profile. Do not mutate the existing capability report into a format that loses its current release history.

## Proposed repository layout

Create or formalize the following structure:

```text
compat/
  pproxy-2.7.9/
    README.md
    provenance.toml
    hashes.toml
    requirements-oracle.txt
    requirements-optional.txt
    known-defects.toml
    namespace-baseline.json
    cli-baseline.json
    examples/
    tests/

docs/parity/
  pproxy_2_7_9_strict_manifest.toml
  PPROXY_2_7_9_STRICT_REPORT.md
  PPROXY_COMPATIBILITY_POLICY.md
  PPROXY_ORACLE_MAINTENANCE.md

tests/strict_api/
tests/strict_cli/
tests/strict_protocol/
tests/strict_process/
tests/strict_dependencies/
tests/strict_platform/

target/compat/strict/
  oracle/
  candidate/
  differential/
  summary.json
```

The exact fixture location may change to comply with licensing or packaging constraints, but the logical separation between oracle inputs, candidate inputs, and generated evidence is mandatory.

## Workstream A1 — Freeze the oracle

### Tasks

1. Pin the target to `pproxy==2.7.9` in one canonical configuration file.
2. Record the exact package source:
   - PyPI version;
   - source distribution hash;
   - wheel hash where available;
   - upstream repository and source commit if it can be conclusively mapped;
   - retrieval date;
   - package metadata;
   - license.
3. Record the Python interpreter versions on which the oracle is executable.
4. Record the optional dependency set used by each protocol or feature.
5. Create a hermetic oracle environment bootstrap command.
6. Make every external differential runner verify the installed oracle version and package hash before executing.
7. Fail closed on mismatched versions unless a clearly named local-development override is set.
8. Include the resolved environment and hashes in every retained evidence bundle.

### Required artifacts

- `compat/pproxy-2.7.9/provenance.toml`
- `compat/pproxy-2.7.9/hashes.toml`
- `compat/pproxy-2.7.9/requirements-oracle.txt`
- validator tests for version and hash mismatch

### Acceptance criteria

- A clean environment can reproduce the oracle without manual package selection.
- A wrong pproxy version causes a hard failure before probes run.
- Evidence identifies the oracle package, interpreter, dependencies, OS, and architecture.
- No test silently falls back to a system-installed pproxy.

## Workstream A2 — Define compatibility vocabulary and policy

### Tasks

Create `docs/parity/PPROXY_COMPATIBILITY_POLICY.md` defining at least:

- `drop_in`;
- `behavioral_match`;
- `wire_match`;
- `source_compatible`;
- `migration_compatible`;
- `native_equivalent`;
- `known_upstream_defect`;
- `platform_constraint`;
- `unsupported`;
- `not_applicable`.

The strict profile may use `drop_in`, `known_upstream_defect`, `platform_constraint`, and `not_applicable` as terminal release states. It may use implementation-progress states internally, but a full certification cannot contain unresolved progress states.

Document these governing rules:

1. Candidate-only tests do not prove compatibility.
2. Importability does not prove functional behavior.
3. Equivalent Rust-native APIs do not prove Python source compatibility.
4. A skipped external test is not a pass.
5. Documentation cannot override differential evidence.
6. A known upstream defect requires a reproducible upstream-only failure and explicit approval.
7. Security hardening that changes observable behavior must be outside strict mode or explicitly classified.
8. The canonical `eggress` namespace is not constrained by strict compatibility defaults.

### Acceptance criteria

- The policy is referenced from contributor guidance and release documents.
- Validators use the policy’s state vocabulary.
- Full-release checks reject any unresolved or non-drop-in entry.
- Existing subset claims remain clearly distinguished from strict certification.

## Workstream A3 — Build the complete strict manifest

### Manifest design

Create `docs/parity/pproxy_2_7_9_strict_manifest.toml` as a separate strict source of truth.

Each record must include fields equivalent to:

```toml
id = "python.pproxy.Connection.tcp_connect"
category = "python_api"
kind = "method"
module = "pproxy.server"
owner = "track-b"
status = "gap"
platforms = ["linux", "macos", "windows"]
python_versions = ["3.9", "3.10", "3.11", "3.12", "3.13"]
oracle_probe = "api.connection_tcp_connect"
candidate_probe = "api.connection_tcp_connect"
comparator = "async_callable_and_return_contract"
implementation_refs = []
test_refs = []
evidence_refs = []
depends_on = ["python.asyncio_stream_adapter"]
notes = ""
```

Exact field names may follow existing manifest conventions, but the information must be represented and validated.

### Required inventory domains

#### Python namespace

Inventory:

- every module in the pproxy distribution;
- public exports;
- commonly imported internal exports observed in upstream tests/examples;
- aliases;
- constants;
- exceptions;
- classes and constructors;
- functions;
- methods and properties;
- module metadata and import side effects.

#### Callable behavior

Record:

- `inspect.signature`;
- coroutine function status;
- async-generator status;
- positional and keyword acceptance;
- default values;
- return object shape;
- exception timing.

#### CLI

Record every option, alias, default, repeatability rule, output stream, exit status, and process side effect.

#### Protocol and transport matrix

Record each protocol by:

- listener role;
- upstream role;
- TCP role;
- UDP role;
- reverse role;
- chain-hop role;
- supported modifiers;
- required optional dependencies;
- platforms.

#### Cipher and plugin matrix

Record every cipher name, alias, implementation mode, key derivation, IV/nonce behavior, packet behavior, and plugin hook.

#### Composition matrix

Record valid and invalid combinations rather than only isolated components:

- listener × upstream;
- protocol × transport;
- hop × hop;
- TCP × UDP;
- reverse × transport;
- cipher × plugin;
- platform × transparent mode;
- option × option CLI interactions.

#### Failure and lifecycle matrix

Record:

- malformed input;
- dependency absence;
- DNS and connection failure;
- authentication failure;
- timeout;
- cancellation;
- EOF and half-close;
- repeated close;
- signal handling;
- event-loop shutdown;
- resource exhaustion.

### Generation strategy

Use a hybrid approach:

1. Generate discoverable namespace/signature facts from the oracle.
2. Hand-author semantic records for protocol, process, platform, and failure behavior.
3. Validate that generated inventories have no orphaned symbols.
4. Require review for every ignored private symbol.

### Acceptance criteria

- Runtime namespace enumeration reports no unclassified public symbol.
- Every existing capability-manifest entry maps to one or more strict records.
- The strict manifest contains no aggregate record broad enough to hide multiple independent callable contracts.
- Every record has an oracle probe or an explicit reason why a black-box scenario is required instead.
- Every record has an implementation owner and milestone assignment.

## Workstream A4 — Build paired observation runners

### Architecture

Each scenario must run twice in isolated subprocesses:

```text
oracle runner    -> pproxy==2.7.9 environment
candidate runner -> eggress + eggress-pproxy-compat environment
```

Both runners receive the same scenario input and emit the same observation schema.

### Required observation fields

At minimum:

- scenario ID;
- environment metadata;
- import result;
- stdout and stderr;
- warnings;
- exit status;
- duration with tolerance metadata;
- object type and qualified name;
- signature;
- coroutine/awaitable status;
- selected attributes;
- return-shape description;
- exception class, normalized message category, and operation stage;
- spawned task count;
- opened file descriptors where measurable;
- listener addresses;
- network transcript or transcript hash;
- generated files and side effects;
- cleanup result.

### Normalization rules

Normalize only unstable values that are not part of the compatibility contract:

- ephemeral ports;
- temporary paths;
- process IDs;
- monotonic timestamps;
- random nonces where the comparator validates structure rather than exact bytes;
- platform-specific path separators when upstream itself varies.

Do not normalize away:

- exception type;
- sync/async differences;
- tuple versus custom object returns;
- output stream differences;
- ordering differences;
- missing attributes;
- protocol bytes;
- exit status;
- failure stage.

### Acceptance criteria

- The same runner protocol can execute oracle and candidate scenarios.
- A runner crash is distinguishable from a candidate behavior mismatch.
- Observation schemas are versioned.
- Schema drift fails tests.
- Every result is retained under oracle, candidate, and differential paths.

## Workstream A5 — Differential comparators

### Comparator classes

Implement reusable comparators for:

- exact JSON equality;
- normalized textual output;
- namespace set equality;
- signature equality;
- callable-kind equality;
- exception compatibility;
- attribute protocol compatibility;
- ordered event sequences;
- byte transcript equality;
- protocol-semantic equality where dynamic values differ;
- process side effects;
- lifecycle/resource bounds;
- timing tolerance.

Each strict-manifest record must name a comparator. Avoid scenario-specific comparison logic hidden in test bodies.

### Difference classification

Every mismatch must be classified as one of:

- candidate defect;
- oracle execution defect;
- harness defect;
- known upstream defect;
- platform constraint;
- manifest/specification defect;
- approved normalization difference.

Unclassified mismatches fail the suite.

### Acceptance criteria

- Comparators produce machine-readable and human-readable differences.
- A known-defect suppression requires a registry entry and regression test.
- Comparator changes invalidate or version prior evidence.
- No comparator can convert missing candidate behavior into a pass through broad normalization.

## Workstream A6 — Known upstream defects registry

### Tasks

Create `compat/pproxy-2.7.9/known-defects.toml`.

Each record must include:

- stable defect ID;
- affected scenario IDs;
- reproducible oracle-only failure;
- platforms and Python versions;
- upstream source reference;
- expected candidate policy;
- approval rationale;
- expiry or review condition;
- regression test.

Resolve the existing inconsistency around external HTTP and SOCKS5 reference-side failures. The repository must establish whether these are:

- harness failures;
- environment failures;
- genuine pproxy defects;
- unsupported oracle scenarios;
- candidate mismatches.

Release documents must use the resulting single classification.

### Acceptance criteria

- There is one canonical classification for each known oracle anomaly.
- Release decision and certification generation consume the registry.
- A defect entry cannot be created without an oracle-only reproduction.
- Suppressed scenarios remain visible in reports.

## Workstream A7 — Import upstream examples and tests

### Tasks

1. Identify all upstream examples and test files relevant to the frozen release.
2. Preserve them unchanged when licensing permits.
3. Otherwise preserve hashes and fetch them during a gated oracle setup step.
4. Record provenance and license notices.
5. Add wrappers around, not modifications within, upstream fixtures.
6. Classify fixtures by API, CLI, protocol, dependency, and platform requirements.
7. Execute the same fixture against oracle and candidate environments.

### Acceptance criteria

- Upstream fixture content is immutable and hash-checked.
- Candidate-specific edits cannot make an upstream fixture pass.
- Fixture setup and environmental skips are explicit.
- At least the canonical upstream API client and server examples are included in the fast external suite.

## Workstream A8 — Validator and report integration

### Tasks

Extend or add validators to enforce:

- unique strict IDs;
- valid states;
- valid owners and milestones;
- existing probe IDs;
- existing comparator IDs;
- existing test references;
- no orphaned generated namespace symbols;
- no strict `drop_in` record without differential evidence;
- no full certification with unresolved records;
- report generation from the manifest and evidence;
- release-document consistency.

Generate:

- `docs/parity/PPROXY_2_7_9_STRICT_REPORT.md`;
- `target/compat/strict/summary.json`;
- per-category summaries;
- unresolved-gap lists grouped by milestone and owner.

### Acceptance criteria

- Reports are generated, not manually maintained.
- CI checks report/manifest/evidence consistency.
- A changed strict record without changed evidence fails the appropriate validation.
- Full-release mode fails if any required scenario was skipped.

## Workstream A9 — CI tiering

### Required tiers

#### Tier 0 — static validation

Runs on every change:

- format and schema validation;
- manifest consistency;
- probe and test reference resolution;
- generated inventory drift checks using committed baselines.

#### Tier 1 — candidate-fast

Runs on every change:

- candidate-only unit/integration tests;
- observation schema tests;
- comparator tests;
- simulated oracle fixtures.

This tier supports development but does not certify compatibility.

#### Tier 2 — oracle differential

Runs on compatibility-related changes and scheduled CI:

- clean oracle environment;
- paired API and CLI probes;
- unchanged upstream examples;
- common HTTP/SOCKS/direct scenarios.

#### Tier 3 — external protocol differential

Runs scheduled and before release:

- pproxy client to Eggress server;
- Eggress client to pproxy server;
- wire transcripts;
- optional dependency profiles.

#### Tier 4 — platform and privileged

Runs in disposable environments:

- transparent proxying;
- system proxy mutation;
- daemon behavior;
- platform-specific sockets and signals.

#### Tier 5 — release certification

Runs from a clean tagged commit with all required dependencies and no skips. Produces retained signed or hashed evidence.

### Acceptance criteria

- Hosted CI results are visible and required for compatibility release branches.
- Release certification cannot be generated from a dirty tree.
- Gated test absence is reported as incomplete, not passing.
- Artifacts include logs, observations, differences, environment metadata, and checksums.

## Workstream A10 — Documentation and claim cleanup

### Tasks

Audit:

- `README.md`;
- parity reports;
- release notes;
- package descriptions;
- Python binding documentation;
- compatibility wheel metadata;
- CLI help text.

Ensure current claims accurately say that Eggress is a modern pproxy compatibility subset and that the Python namespace is not yet a complete behavioral replacement.

Add a section describing the strict roadmap and certification status without presenting planned work as implemented.

### Acceptance criteria

- No active document makes an unqualified full drop-in claim.
- Historical documents are marked historical rather than rewritten misleadingly.
- Generated strict reports identify the exact oracle version.
- Release language is automatically checked where practical.

## Sequencing

### Stage 1 — Contract and provenance

Complete A1 and A2 first. These establish vocabulary and the frozen target.

### Stage 2 — Inventory and schemas

Complete A3 and the observation schema portion of A4.

### Stage 3 — Runners and comparators

Complete A4 and A5 with representative API, CLI, and network scenarios.

### Stage 4 — Fixtures and defect classification

Complete A6 and A7. Resolve existing reference-side anomalies here.

### Stage 5 — Enforcement

Complete A8, A9, and A10. Turn the contract into required CI behavior.

## Required tests

At minimum, add tests covering:

- wrong oracle version;
- wrong package hash;
- missing oracle dependency;
- namespace inventory drift;
- signature drift;
- coroutine-kind drift;
- return-shape drift;
- exception-stage drift;
- stdout/stderr drift;
- byte-transcript drift;
- oracle process crash;
- candidate process crash;
- schema-version mismatch;
- comparator-version mismatch;
- orphan manifest probe;
- orphan test reference;
- `drop_in` without evidence;
- known-defect record without reproduction;
- full certification with a skipped scenario;
- release-report inconsistency.

## Milestone acceptance criteria

Milestone A is complete only when all of the following are true:

1. The pproxy 2.7.9 oracle is pinned by version and hash.
2. Oracle and candidate environments are isolated and reproducible.
3. The strict manifest inventories all discoverable Python symbols and all manually specified CLI/protocol/process domains.
4. Every strict record has an owner, milestone, probe, comparator, and evidence policy.
5. Paired runners produce versioned normalized observations.
6. Differential comparators produce structured mismatch classifications.
7. Upstream examples run unchanged in the paired harness.
8. Existing reference-side anomalies have one canonical classification.
9. CI distinguishes candidate-only confidence from oracle-backed compatibility evidence.
10. No strict `drop_in` entry can exist without passing differential evidence.
11. Generated strict reports and release documentation are consistency-checked.
12. Public claims remain qualified until final certification.

## Handoff notes

Implementation should begin in `eggress-testkit`, the manifest validator, and compatibility documentation before changing runtime code.

The first implementation commit should establish the policy, oracle provenance, and schema. The second should add the minimal paired runner and representative probes. Only after those are stable should the team generate the full inventory, because generated records must target a stable schema.

Do not attempt to close existing compatibility gaps during this milestone unless a small change is required to make the candidate observable. Record gaps accurately and assign them to later milestones instead.
