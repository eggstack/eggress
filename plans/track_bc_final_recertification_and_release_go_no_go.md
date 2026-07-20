# Track B/C final recertification and release GO/NO-GO pass

## Objective

Perform one clean, immutable, end-to-end certification run for the declared **modern pproxy compatibility subset** after the cross-platform and release-workflow corrective commits that followed the prior operational-certification plan.

This pass is deliberately operational and corrective. It must not add protocol breadth, redesign public APIs, expand the compatibility claim, or introduce unrelated refactors. Its purpose is to:

- select and freeze one final release-candidate commit;
- execute every mandatory hosted CI, clean-install, differential, interoperability, security, and artifact lane against that exact commit;
- retain machine-readable evidence tied to the candidate SHA;
- reconcile the 148-capability manifest against the evidence actually produced;
- issue an unambiguous SHA-specific **GO** or **NO-GO** decision;
- publish only if all mandatory conditions pass without modifying the frozen candidate.

The previous operational-certification attempt is historical evidence only. It cannot certify the current implementation because corrective commits changed runtime behavior, platform-specific tests, wheel matrices, and release archive generation after that candidate was evaluated.

## Baseline

The implementation head observed before this plan was written was:

```text
0bee0b30969303fb8ed7bbcbc4b72018a9a02162
```

That sequence includes substantial post-certification corrections:

- Windows path normalization for TLS, Trojan, manifest, and configuration tests;
- Unix-only gating for Unix sockets, shell-dependent tests, SIGHUP reload tests, transparent listeners, and platform-specific helpers;
- reverse-client connect-timeout configuration and slower-runner timing corrections;
- reverse self-interoperability metric assertion correction;
- Python asyncio shutdown and cleanup corrections;
- wheel-build shell and interpreter selection fixes;
- native macOS x86_64 runner selection;
- removal of the unworkable Linux aarch64 wheel cross-build lane;
- release license-path corrections;
- BSD `tar` compatible archive creation with the license included in the initial archive.

The executing agent must resolve the actual `main` head after this plan lands. It must not assume the baseline SHA above remains current.

Current declared scope remains:

- 148 parity-manifest capabilities;
- 103 `drop_in`;
- 16 `compatible_with_warning`;
- 15 `native_equivalent`;
- 9 `intentional_non_parity`;
- 5 `unsupported`;
- canonical `eggress` Python distribution;
- separate `eggress-pproxy-compat` distribution providing explicit top-level `import pproxy`;
- supported wheel targets currently limited to Linux x86_64, macOS x86_64, macOS arm64, and Windows x86_64 unless the repository metadata deliberately changes before candidate freeze;
- no claim of strict full pproxy parity.

## Mandatory invariants

1. One immutable candidate SHA must be used for the entire run.
2. The candidate must be identified by full SHA, tree SHA, and annotated RC tag.
3. Any source, manifest, workflow, packaging, lockfile, or generated-report change invalidates the candidate.
4. Any candidate-invalidating change requires a new SHA, new tag, new evidence directory, and complete rerun.
5. No mandatory scenario may be counted as passing because a test exists, compiles, or was executed against an older SHA.
6. Missing, skipped, unavailable, or timed-out mandatory scenarios are release failures unless the affected capability is demoted and a new candidate is certified.
7. No workflow may validate a moving `main` reference when a frozen commit or tag can be used.
8. Source-tree imports may not substitute for installed-wheel validation.
9. Cross-compilation may establish buildability but not runtime compatibility; matching native runners must perform clean-install smoke tests for shipped wheels.
10. The final decision is binary. No provisional, conditional, or partial GO is permitted.

# Workstream 0 — preflight and scope lock

## Tasks

1. Inspect the current repository state and list every commit after this plan.
2. Verify whether any post-plan commit changes implementation, workflows, manifests, packaging, or release documentation.
3. Resolve and record:
   - current `main` SHA;
   - current tree SHA;
   - `Cargo.lock` SHA-256;
   - parity-manifest SHA-256;
   - composition-matrix SHA-256;
   - generated parity-report SHA-256;
   - canonical Python version;
   - compatibility-distribution version;
   - CLI/crate version;
   - supported Python classifiers;
   - supported binary targets;
   - supported wheel targets.
4. Confirm all release and support documentation agrees on the removal of Linux aarch64 wheels.
5. Confirm Linux aarch64 binaries, containers, or source builds are described separately from Python wheel support where applicable.
6. Confirm no documentation still advertises unsupported wheel targets or strict full parity.
7. Record explicit non-goals for this pass:
   - no SSH implementation;
   - no QUIC/H3 implementation;
   - no SSR implementation;
   - no legacy Shadowsocks/OTA expansion;
   - no advanced-transport listener implementation;
   - no general multi-hop UDP expansion;
   - no live-path plugin redesign;
   - no unrelated cleanup.

## Exit gate

A signed-off preflight record identifies the exact candidate scope and contains no unresolved support-matrix or version contradiction.

# Workstream 1 — freeze the final candidate

## Tasks

1. Select the exact candidate SHA from current `main` only after preflight passes.
2. Verify a clean working tree.
3. Verify all generated files are current before freezing:
   - parity report;
   - capability counts;
   - composition matrix validation output;
   - URI corpus validation output;
   - type stubs if generated;
   - lockfiles;
   - release metadata.
4. Create a new annotated RC tag, for example:

```text
v0.1.0-rc.2
```

5. The tag message must include:
   - full candidate SHA;
   - previous invalidated candidate/tag if applicable;
   - manifest counts;
   - supported wheel targets;
   - pinned `pproxy==2.7.9` reference;
   - statement that this is a modern compatibility subset.
6. Record:
   - commit SHA;
   - tree SHA;
   - tag object SHA;
   - tagger identity;
   - timestamp;
   - `Cargo.lock` SHA-256;
   - manifest and composition hashes.
7. Create a candidate identity artifact under:

```text
target/release-evidence/<candidate-sha>/candidate.json
```

8. Prevent tag reuse or force-update.
9. All later workflow dispatches and evidence records must reference the full SHA and tag.

## Exit gate

One immutable candidate identity is available and no certification job targets a moving branch.

# Workstream 2 — hosted CI execution on the frozen SHA

## Required Linux lanes

Run on native Linux x86_64:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --workspace
cargo check --manifest-path fuzz/Cargo.toml --bins
cargo test --manifest-path fuzz/Cargo.toml --no-run
```

Also execute:

- every in-tree fuzz-smoke suite;
- manifest validator;
- composition-matrix validator;
- generated parity-report consistency check;
- documented test-name existence check;
- URI-corpus validator;
- documentation consistency tests;
- Python source matrix lanes assigned to Linux;
- `cargo audit` or the repository's documented vulnerability-policy command;
- SBOM generation and validation.

## Required macOS lanes

### macOS arm64

- full supported workspace build and tests;
- native canonical wheel build;
- native compatibility wheel build;
- clean installation outside the checkout;
- wheel import and runtime smoke;
- native outbound sync/async stream smoke;
- cipher known-answer tests;
- Unix-domain socket tests;
- release archive generation using BSD `tar`;
- archive extraction and license-presence verification.

### macOS x86_64

Use a native Intel runner where available, currently expected to be `macos-13` or an explicitly supported replacement.

- build and test the supported workspace subset;
- build x86_64 canonical wheel;
- build compatibility wheel;
- install both on the same native x86_64 runner;
- run import and representative runtime smoke;
- create and extract release archives;
- verify executable architecture and package tags.

Cross-building on arm64 is not sufficient for this lane.

## Required Windows x86_64 lanes

- full workspace build and tests for the supported feature set;
- clippy/check where supported;
- canonical wheel build;
- compatibility wheel build;
- clean installation in a new Windows virtual environment;
- `import eggress` and `import pproxy`;
- native outbound sync/async stream tests;
- process cleanup and shutdown tests;
- TLS and Trojan path-handling tests;
- manifest path-normalization tests;
- reverse-client timeout tests;
- unsupported Unix/transparent features produce documented diagnostics;
- Windows release ZIP creation and extraction;
- license presence and binary execution checks.

## Workflow evidence requirements

For every workflow and job, record:

- workflow name;
- run ID;
- job ID;
- candidate SHA;
- triggering tag/ref;
- runner image;
- OS version;
- architecture;
- conclusion;
- start/end timestamps;
- artifact IDs;
- rerun attempt number;
- logs or exported summaries.

A rerun must retain the failed attempt. It may not replace or hide it.

## Exit gate

Every required native hosted lane is green against the exact frozen SHA. No required platform is represented only by cross-compilation.

# Workstream 3 — correct and certify wheel support

## Supported wheel matrix

Unless deliberately changed before candidate freeze, certify exactly:

| Platform | Architecture | Required native install smoke |
|---|---:|---:|
| Linux | x86_64 | yes |
| macOS | x86_64 | yes |
| macOS | arm64 | yes |
| Windows | x86_64 | yes |

Linux aarch64 wheels are not part of this candidate. Do not leave stale classifiers, tables, release notes, artifact expectations, or checksums suggesting otherwise.

## Canonical wheel procedure

For each supported target:

1. Build from the frozen tag.
2. Create a clean virtual environment outside the repository.
3. Install only the built wheel and declared dependencies.
4. Run with `--import-mode=importlib` where tests are used.
5. Verify repository paths are absent from `sys.path` or otherwise cannot mask packaging defects.
6. Verify:
   - `import eggress`;
   - version agreement;
   - capability metadata;
   - type-marker presence;
   - native extension architecture;
   - direct native outbound TCP echo;
   - HTTP, SOCKS5, WS, raw, and H2 paths claimed for the wheel smoke subset;
   - sync and async close semantics;
   - server start/stop;
   - supported AEAD operations;
   - no temporary local listener in outbound connection paths.

## Compatibility wheel procedure

For each supported target:

1. Build `eggress-pproxy-compat` from the same candidate.
2. Install the matching canonical wheel and compatibility wheel in a clean environment.
3. Verify:
   - `import pproxy` works without source path injection;
   - `from pproxy import Connection, Server` works;
   - documented `pproxy.proto` and `pproxy.cipher` imports work;
   - compatibility metadata reports the intended subset;
   - representative unchanged pproxy programs execute;
   - dependency resolution selects the exact compatible Eggress release range;
   - mismatched package versions fail clearly;
   - uninstalling the compatibility wheel removes its namespace but leaves `eggress` functional.

## Artifact inspection

Inspect every wheel for:

- expected native extension;
- expected package modules;
- `.pyi` files;
- `py.typed`;
- license and metadata;
- dependency metadata;
- correct platform tag;
- absence of source-tree paths, build caches, credentials, and test artifacts.

## Exit gate

Every advertised wheel builds and runs on a matching native runner, and every unadvertised target is absent from release metadata.

# Workstream 4 — release workflow and archive verification

The recent fixes to release archive construction must be tested as behavior, not reviewed only as YAML.

## Binary archive procedure

For every shipped binary target:

1. Build from the frozen tag.
2. Produce the release archive using the actual workflow command.
3. Extract it in a fresh directory.
4. Verify:
   - expected binary filename;
   - executable permission on Unix;
   - expected architecture;
   - `--version` output;
   - license file present;
   - no duplicate or nested incorrect paths;
   - no stale files from previous build steps;
   - checksum matches inventory.

## BSD tar verification

On macOS:

- ensure archive creation succeeds using BSD `tar`;
- ensure no `tar rf` append to a compressed archive remains;
- ensure the license is included during initial `tar czf` creation;
- extract and verify both binary and license.

## Windows ZIP verification

- create with the workflow's PowerShell path;
- extract with a clean Windows toolchain;
- verify binary and license;
- execute `--version` from the extracted directory.

## Release workflow dry run

Run the release workflow against the RC tag without publishing a final stable release where possible. Verify:

- binary jobs;
- canonical wheel jobs;
- compatibility wheel job;
- sdist job;
- checksum generation;
- SBOM generation;
- artifact collection;
- release-note rendering;
- no final publishing step runs unless explicitly authorized.

## Exit gate

Every release artifact is buildable, extractable, internally consistent, and licensed on its native platform.

# Workstream 5 — full Python source and lifecycle matrix

Run Python 3.9 through 3.13 only where the package metadata and dependency support still declare those versions.

Required suites include:

- complete `python/tests`;
- C1 API contract tests;
- outbound stream verification;
- `ProxyConnection` tests;
- server lifecycle tests;
- asyncio semantic tests under debug mode;
- protocol/cipher/plugin/wrapper tests;
- AEAD known-answer vectors;
- wheel import smoke;
- compatibility-package import/program tests;
- resource leak tests;
- architecture mismatch behavior tests.

## Native outbound stress requirements

Exercise at least:

- 1,000 sequential connect/write/read/close cycles;
- 100 concurrent sync connections where the API permits;
- 100 concurrent async connections;
- cancellation during connect;
- cancellation during read;
- cancellation during write or drain;
- cancellation during close;
- half-close from each side;
- server reset;
- timeout;
- DNS failure;
- proxy authentication failure;
- repeated event-loop creation/destruction;
- garbage collection after explicit and implicit close paths.

Record before/after:

- file descriptors or handles;
- open sockets;
- Python tasks;
- Rust runtime threads;
- child processes;
- temporary files;
- internal connection counters.

Define tolerances before execution. Any unexplained monotonic growth is a failure.

## Exit gate

All supported Python and lifecycle lanes pass with no undocumented skip, warning, or resource-growth pattern.

# Workstream 6 — pinned pproxy differential certification

Use a clean environment with exactly:

```text
pproxy==2.7.9
```

Capture package hash and installation metadata.

## Mandatory scenario groups

- CLI help, version, defaults, and invalid input;
- HTTP CONNECT;
- HTTP forward proxy behavior;
- SOCKS4 and SOCKS4a;
- SOCKS5;
- supported authentication paths;
- IPv4, IPv6, and domain targets;
- timeout, refusal, malformed input, half-close, and shutdown;
- supported routing and scheduler behavior;
- supported TCP chain compositions;
- WS/WSS, raw/tunnel, and H2 compositions currently classified `drop_in`;
- Trojan TCP listener and upstream behavior claimed against pproxy;
- supported UDP cases;
- reverse/backward behavior marked release-blocking;
- supported Python import and program contracts.

## Evidence rules

Each scenario must record:

- scenario ID;
- mapped capability IDs;
- mapped composition cell if applicable;
- reference command/config;
- Eggress command/config;
- normalized observation schema;
- raw stdout/stderr;
- exit state;
- duration;
- retry count;
- result: pass, fail, not_executed, or not_applicable;
- exact reason for non-pass status.

Normalization rules must be committed and documented. They may remove nondeterministic ports, timing jitter, and equivalent formatting, but may not erase semantic differences.

## Exit gate

Every release-blocking differential scenario passes against the frozen candidate. Any failed or unexecuted claim requires demotion plus a new candidate, or code correction plus a new candidate.

# Workstream 7 — independent interoperability certification

## Trojan

Use a pinned external implementation such as `trojan-go` and retain its version and binary hash.

Required directions and cases:

- Eggress client to external server;
- external client to Eggress server where supported;
- correct credentials;
- incorrect credentials;
- SNI mismatch;
- custom CA trust;
- half-close;
- abrupt shutdown;
- fragmented payloads;
- large payloads;
- concurrent sessions;
- process cleanup.

## Shadowsocks

Use pinned independent standard Shadowsocks implementations.

Required cases:

- supported AEAD TCP methods;
- supported AEAD UDP methods;
- IPv4, IPv6, and domain targets;
- incorrect key;
- fragmentation;
- large payloads;
- client/server directionality claimed by the manifest.

## WebSocket/WSS

Use an independent standards-based tunnel fixture or implementation.

Required cases:

- upgrade request/response correctness;
- path and Host behavior;
- WSS certificate and SNI;
- fragmentation;
- ping/pong/control-frame handling where applicable;
- close behavior;
- prior-hop stream composition.

## HTTP/2 CONNECT

Use an independent H2 CONNECT implementation.

Required cases:

- TLS ALPN;
- authentication;
- concurrent streams;
- flow control;
- stream reset;
- GOAWAY;
- pool recovery;
- prior-hop composition;
- pool isolation by chain position and authentication/TLS context.

## Exit gate

All external-wire capabilities claimed as release-certified have retained independent passing evidence. Unavailable implementations do not count as passing.

# Workstream 8 — security, fuzz, and resource closure

## Deterministic CI security coverage

Run:

- all in-tree fuzz-smoke suites;
- malformed HTTP corpus;
- malformed Trojan corpus;
- malformed WebSocket corpus;
- malformed Shadowsocks corpus;
- malformed SOCKS corpus;
- malformed URI and TOML corpus;
- bounded header/frame/handshake limits;
- authentication failure bursts;
- queue saturation;
- H2 pool and stream saturation;
- stalled-peer and slowloris cases;
- DNS rebinding/private-address policy tests;
- child-process termination tests;
- temporary-file cleanup tests.

## Bounded fuzz execution

For every release-critical fuzz target:

- compile on the frozen SHA;
- run a documented bounded duration or iteration count;
- retain crash artifacts even if empty;
- record sanitizer/runtime versions;
- record corpus size and final coverage statistics where available.

At minimum include:

- URI parsing;
- SOCKS handshake and UDP datagrams;
- HTTP CONNECT response/authority/header parsing;
- Trojan request and accept parsing;
- Shadowsocks framing;
- TOML config parsing/compilation;
- WebSocket handshake/error paths;
- H2 CONNECT authority/header parsing;
- routing evaluation.

## Exit gate

No crash, hang, unbounded allocation, high-severity security defect, or unexplained resource leak remains.

# Workstream 9 — capability-to-evidence audit

Audit all 148 manifest entries against the final candidate evidence.

For each capability, verify:

- ID and category;
- public surface;
- parser status;
- translator status;
- config status;
- runtime status;
- CLI status;
- Python status;
- applicable roles;
- applicable traffic kinds;
- supported platforms;
- tier;
- evidence type;
- candidate-specific evidence references;
- warnings and migration notes.

## `drop_in` requirements

Every `drop_in` entry must have:

- passing runtime evidence;
- matching role and traffic kind;
- platform evidence for every advertised platform;
- composition-specific evidence for chain claims;
- clean-installed package evidence for Python claims;
- independent interoperability evidence for external wire claims where required;
- no known semantic difference outside documented normalization.

Parser-only, structural-only, construction-only, synthetic-only, skipped, or stale evidence cannot support `drop_in`.

## Reconciliation

1. Regenerate `docs/parity/PPROXY_PARITY_REPORT.md`.
2. Verify all counts match:
   - README;
   - release readiness docs;
   - release notes;
   - certification docs;
   - compatibility-package metadata.
3. Verify no stale references remain to invalidated candidates or unsupported wheel targets.
4. If any capability is demoted, create a new candidate and repeat all relevant certification lanes; do not edit the manifest after the evidence bundle is finalized.

## Exit gate

Zero over-classified or unreferenced release-blocking capabilities remain.

# Workstream 10 — immutable evidence bundle

Use `scripts/release_evidence.py` with fail-closed guards:

```bash
python3 scripts/release_evidence.py \
  --require-clean \
  --expected-commit <full-candidate-sha> \
  --verify-tracked-inputs \
  --reference pproxy==2.7.9 \
  --output target/release-evidence/<full-candidate-sha> \
  ...
```

## Required contents

- candidate identity JSON;
- tag and tree identity;
- environment and toolchain metadata;
- workflow run/job export;
- command inventory;
- Rust test results;
- Python source-matrix results;
- native wheel build/install results;
- compatibility-wheel results;
- pproxy differential results;
- third-party interoperability results;
- fuzz/security/resource results;
- artifact inventory;
- wheel and binary hashes;
- archive-content listings;
- SBOMs;
- signatures or attestations where configured;
- manifest and composition snapshots;
- generated parity report;
- capability-to-evidence index;
- redaction report;
- pass/fail/not-executed totals;
- final decision record.

## Retention

- upload the complete bundle as an immutable GitHub Actions artifact;
- attach or link it to the RC or final release;
- retain workflow run IDs and artifact IDs in the decision record;
- commit only concise human-readable summaries, not bulky raw logs, unless repository policy says otherwise;
- never overwrite an evidence directory for another SHA.

## Exit gate

An independent reviewer can reconstruct every release claim and decision from retained artifacts without relying on commit messages or local state.

# Workstream 11 — final release decision

Create a new decision file specific to this candidate, for example:

```text
docs/release/RELEASE_DECISION_v0.1.0-rc.2.md
```

Do not reuse or silently update the historical `rc.1` decision as though it certified the new candidate.

## Required decision content

- candidate SHA and tag;
- tree SHA;
- artifact hashes;
- supported target matrix;
- explicitly unsupported targets, including Linux aarch64 wheels if still excluded;
- hosted workflow run IDs and conclusions;
- Rust and Python test totals;
- differential scenario totals;
- third-party interoperability totals;
- fuzz/security/resource totals;
- skipped/not-executed scenarios;
- manifest counts;
- known limitations;
- residual risks;
- evidence-bundle location and hash;
- final GO or NO-GO;
- reviewer or automation identities;
- timestamp.

## GO conditions

Every item is mandatory:

- candidate identity is immutable and clean;
- every required hosted job passes on the candidate SHA;
- every shipped wheel builds and runs on a matching native platform;
- every release archive builds and extracts correctly, including BSD-tar macOS archives;
- canonical and compatibility packages install cleanly;
- `import pproxy` passes the certified subset without path tricks;
- pinned pproxy differential release blockers pass;
- required third-party interoperability scenarios pass;
- resource/security/fuzz gates pass;
- no high-severity open defect;
- all `drop_in` capabilities reference candidate-specific evidence;
- evidence bundle is complete, immutable, and retained;
- documentation accurately says **modern pproxy compatibility subset** rather than strict full parity.

## NO-GO conditions

Any one blocks release:

- candidate changed after freeze;
- missing, cancelled, neutral, skipped, or red mandatory workflow;
- native wheel smoke absent for a shipped wheel;
- archive cannot be produced or extracted on its native platform;
- incorrect or missing license in an artifact;
- unsupported target still advertised;
- required differential or external scenario unavailable or unexecuted;
- version mismatch between canonical and compatibility distributions;
- source-tree import masks a packaging defect;
- resource-growth regression;
- fuzz crash or security failure;
- stale generated report;
- over-classified parity entry;
- evidence guard violation;
- missing evidence artifact.

## Corrective handling

On NO-GO:

1. Write a concise failure ledger grouped by root cause.
2. Implement only the corrections necessary to resolve observed failures.
3. Do not add unrelated features or refactors.
4. Update tests first or together with each correction.
5. Invalidate the candidate and RC tag; never retarget the tag.
6. Create a new candidate tag and rerun this entire plan.

# Required deliverables

- candidate identity artifact;
- immutable RC tag;
- hosted workflow run and job inventory;
- Linux/macOS/Windows native validation results;
- canonical wheel matrix results;
- compatibility wheel matrix results;
- release archive extraction results;
- pproxy differential evidence;
- Trojan/Shadowsocks/WS/H2 external evidence;
- security/fuzz/resource evidence;
- artifact hashes, SBOMs, and provenance;
- 148-entry capability-to-evidence index;
- regenerated parity report;
- immutable evidence bundle;
- new SHA-specific release decision document;
- updated release-readiness status.

# Acceptance criteria

- One candidate SHA is unchanged throughout certification.
- All required workflows pass against that SHA.
- Every advertised wheel is built and executed on a matching native runner.
- Linux aarch64 wheel support is either correctly absent everywhere or restored through a proper manylinux-native build and independently certified before freeze.
- macOS release archives succeed with BSD `tar`, contain the binary and license, and extract correctly.
- Windows tests no longer rely on Unix paths, signals, shells, or socket features.
- Native outbound streams pass lifecycle, cancellation, concurrency, and leak testing.
- Canonical and compatibility distributions install from built wheels in clean environments.
- Pinned pproxy release-blocking differential scenarios pass.
- Required independent interoperability scenarios pass.
- Every `drop_in` capability references evidence generated for the candidate.
- No mandatory scenario is skipped or inferred.
- The evidence generator passes all clean/SHA/input guards.
- The final decision document contains an unconditional GO or NO-GO.
- A GO decision is issued only for the phrase **modern pproxy compatibility subset**.

# Recommended execution order

1. Preflight and scope lock.
2. Candidate freeze and annotated tag.
3. Hosted Rust/platform CI.
4. Native canonical and compatibility wheel matrix.
5. Release archive dry run and extraction.
6. Full Python source/lifecycle matrix.
7. Pinned pproxy differential suite.
8. Independent interoperability suites.
9. Security, fuzz, and resource runs.
10. Capability-to-evidence audit.
11. Evidence-bundle assembly.
12. Final GO/NO-GO record.

Do not begin publication while any earlier step remains incomplete or while a workflow fix is still being tested on a moving branch.