# Track B/C operational certification and release decision

## Objective

Execute the final operational certification for the declared modern pproxy compatibility subset against one frozen candidate commit. This phase does not add protocol breadth or redesign existing APIs. It proves that the current release candidate builds, installs, imports, runs, interoperates, and cleans up correctly across the supported release matrix, and it produces a retained, reviewable evidence bundle tied to the exact candidate SHA.

The phase ends with one of two outcomes:

- **GO**: all mandatory lanes pass, all required external scenarios have retained passing evidence, every `drop_in` capability is supported by matching evidence, and release artifacts are reproducible and internally consistent;
- **NO-GO**: any mandatory lane fails, any release-blocking scenario is unexecuted, any artifact is mismatched, or any parity claim exceeds the available evidence.

No conditional or provisional release recommendation is permitted. A corrective code commit invalidates the frozen candidate and requires a new certification run from the beginning.

## Baseline

Current `main` includes:

- stream-native WS/WSS, raw/tunnel, and H2 upstream composition;
- H2 pool isolation by endpoint, TLS/authentication context, and chain position;
- native Rust and Python outbound connectors;
- synchronous and asynchronous Python outbound stream wrappers;
- `ProxyConnection` using the native connector path;
- a separate `eggress-pproxy-compat` distribution providing top-level `import pproxy`;
- deterministic supported AEAD behavior through `cryptography>=42,<47`;
- clean-wheel compatibility workflows for Linux, macOS, and Windows;
- release packaging for canonical and compatibility wheels;
- hardened release-evidence generation;
- release-candidate lifecycle, resource, cipher-vector, and fuzz-smoke verification;
- a 148-capability parity manifest.

The remaining work is operational proof, artifact retention, and final evidence-to-claim reconciliation.

# Workstream 1 — freeze and identify the release candidate

1. Select the exact candidate SHA from `main`.
2. Create an annotated release-candidate tag such as `vX.Y.Z-rc.N` pointing to that SHA.
3. Record:
   - full commit SHA;
   - tree SHA;
   - tag object SHA;
   - `Cargo.lock` SHA-256;
   - canonical and compatibility package versions;
   - pinned `pproxy==2.7.9` reference;
   - Rust toolchain version;
   - supported Python versions;
   - supported target triples.
4. Verify the worktree is clean and the tag resolves to the expected commit.
5. Generate a candidate manifest under `target/release-evidence/<sha>/candidate.json`.
6. Prohibit force-updating or reusing the tag after certification begins.

Exit gate: one immutable candidate identity is available to every workflow and evidence file.

# Workstream 2 — hosted Rust validation

Run hosted CI against the frozen SHA, not an unpinned moving branch.

Required Linux lanes:

- `cargo fmt --all -- --check`;
- `cargo check --workspace --all-targets`;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo test --workspace --all-features`;
- release-mode build for all shipped binaries;
- `cargo test --manifest-path fuzz/Cargo.toml --no-run`;
- all in-tree fuzz-smoke suites;
- manifest, composition matrix, generated-report, URI-corpus, and documentation consistency validators;
- `cargo audit` or the repository's pinned vulnerability policy gate;
- SBOM generation and validation.

Required macOS lanes:

- workspace build and tests;
- native arm64 wheel build and clean installation;
- x86_64 build or cross-build where supported;
- Unix-domain and platform-specific tests;
- architecture-mismatch behavior tests.

Required Windows lanes:

- workspace build and tests for supported targets;
- wheel build and clean installation;
- process, socket, cleanup, and async lifecycle tests;
- unsupported Unix/transparent features must fail with documented diagnostics rather than compile or runtime ambiguity.

All workflow run IDs, job IDs, conclusions, runner images, and artifact IDs must be recorded.

Exit gate: every mandatory hosted Rust/platform job is green for the exact candidate SHA.

# Workstream 3 — Python source and wheel certification

## Source-tree matrix

Run the full Python suite for every supported Python minor version, currently 3.9 through 3.13 unless packaging metadata is deliberately narrowed.

Required checks:

- complete `python/tests` suite;
- C1 contract tests;
- native outbound stream verification;
- asyncio semantic suite with debug mode;
- protocol/cipher/plugin/wrapper tests;
- server lifecycle and resource tests;
- type-stub/import surface checks;
- no unexpected skips other than documented platform/reference conditions;
- warnings treated according to an explicit allowlist.

## Clean canonical-wheel matrix

For each supported OS/architecture/Python combination:

1. Build the canonical `eggress` wheel from the frozen SHA.
2. Create a fresh virtual environment outside the repository.
3. Install only the built wheel and declared runtime dependencies.
4. Verify no source-tree paths are importable.
5. Run:
   - `import eggress`;
   - version and capability checks;
   - native outbound TCP echo through direct, HTTP, SOCKS5, WS, raw, and H2 supported paths;
   - sync and async stream lifecycle;
   - server start/stop and context-manager smoke;
   - cipher known-answer vectors;
   - resource cleanup and descriptor/socket leak smoke.

## Clean compatibility-wheel matrix

1. Build `eggress-pproxy-compat` from the same candidate.
2. Install the canonical and compatibility wheels in a fresh environment.
3. Verify:
   - `import pproxy` works without path manipulation or `sys.modules` injection;
   - `pproxy.__version__` and compatibility metadata match policy;
   - `from pproxy import Connection, Server` works;
   - documented protocol and cipher imports work;
   - the pinned public contract subset passes;
   - representative unchanged pproxy programs execute successfully;
   - uninstalling the compatibility wheel removes only the provided namespace and does not damage `eggress`.
4. Verify dependency resolution installs the declared supported cipher dependency deterministically.
5. Verify mismatched canonical/compatibility versions fail clearly or are rejected by dependency metadata.

Exit gate: every supported wheel installs and passes its clean-environment subset on every supported platform.

# Workstream 4 — pproxy differential execution

Run all release-blocking scenarios against pinned `pproxy==2.7.9` in clean environments.

Mandatory scenario groups:

- CLI help/version/defaults and invalid-input behavior;
- HTTP CONNECT and ordinary forwarding;
- SOCKS4, SOCKS4a, and SOCKS5;
- authentication success/failure;
- IPv4, IPv6, and domain targets;
- half-close, timeout, refusal, malformed input, and shutdown;
- supported scheduler/routing cases;
- supported multi-hop TCP chains;
- WS/WSS, raw/tunnel, and H2 compositions claimed as `drop_in`;
- Trojan TCP listener/upstream behavior;
- standalone and supported proxied UDP cases;
- reverse/backward scenarios currently classified as release-blocking;
- Python top-level import and supported program contracts.

Requirements:

- every scenario has an explicit ID mapped to capability/composition IDs;
- pproxy and Eggress observations are captured separately;
- outputs are normalized only through documented rules;
- skipped, unavailable, or timed-out scenarios remain `not_executed` or `fail` as appropriate;
- no scenario is marked passing based only on test discovery or command construction;
- retries are recorded and may not hide deterministic failures;
- flaky scenarios must be corrected or removed from release-blocking classification.

Exit gate: every release-blocking differential scenario has a retained passing result for the frozen SHA.

# Workstream 5 — third-party interoperability execution

Execute independent implementation tests where pproxy is not a sufficient wire oracle.

## Trojan

Run the retained suite against a pinned external Trojan implementation such as `trojan-go`:

- Eggress client to external server;
- external client to Eggress server where supported;
- valid authentication;
- invalid authentication;
- SNI and custom CA behavior;
- half-close;
- abrupt shutdown;
- concurrent connections;
- large and fragmented payloads.

## Shadowsocks

Run against pinned standard Shadowsocks client/server implementations:

- supported AEAD TCP methods;
- supported AEAD UDP methods;
- IPv4, IPv6, and domain targets;
- authentication/key mismatch;
- fragmentation and large payloads.

## WebSocket

Run against an independent WebSocket proxy/tunnel implementation or standards-based fixture:

- HTTP upgrade correctness;
- path and host handling;
- WSS certificate/SNI behavior;
- fragmentation/control-frame handling;
- prior-hop stream composition.

## HTTP/2

Run against an independent H2 CONNECT implementation:

- TLS ALPN;
- h2c only if claimed;
- authentication;
- concurrent streams;
- flow control;
- GOAWAY and reset recovery;
- prior-hop composition;
- pool isolation.

Pin external binary versions and hashes. Capture installation/source provenance. If an external implementation is unavailable, the corresponding capability cannot receive final external certification.

Exit gate: all required third-party scenarios have retained passing evidence or the affected parity tier is demoted before release.

# Workstream 6 — security and resource certification

Run release-mode security and resource tests against the frozen SHA:

- malformed protocol corpus;
- bounded header/handshake/frame sizes;
- authentication failure bursts;
- DNS rebinding/private-address policy;
- slowloris and stalled-peer behavior;
- queue and pool saturation;
- H2 stream/pool exhaustion;
- plugin callback backpressure for the supported structural surface;
- repeated native outbound stream creation and teardown;
- repeated compatibility package server/connection lifecycle;
- cancellation during connect, read, write, close, and shutdown;
- descriptor, socket, task, thread, and temporary-file leak detection;
- process-child cleanup for all external harnesses;
- sanitizer or Miri lanes where practical for changed unsafe-sensitive components;
- bounded fuzz runs for the release-critical parsers in addition to in-tree smoke tests.

Define explicit thresholds and tolerances before execution. Do not convert a failure into a warning after observing the result without a documented policy change and new candidate.

Exit gate: no high-severity correctness/security defect and no unexplained resource growth.

# Workstream 7 — artifact and reproducibility verification

Build final candidate artifacts only from the frozen tag:

- platform binaries;
- canonical Python wheels;
- compatibility wheel;
- source distribution;
- crates if publishing;
- containers if maintained;
- SBOMs;
- provenance/attestation files;
- checksums and signatures.

Verify:

- versions agree across all artifacts;
- compatibility package dependency pins select the matching canonical package;
- wheels contain expected modules, stubs, and `py.typed` markers;
- no development files, secrets, local paths, or test artifacts are included;
- binaries report the candidate version and commit where supported;
- artifact hashes remain stable across documented reproducible-build lanes or divergences are explained;
- release archive contents match the published manifest;
- installation instructions work from downloaded artifacts, not the repository checkout.

Exit gate: artifact inventory, hashes, signatures, SBOM, and package metadata are internally consistent.

# Workstream 8 — evidence bundle assembly

Use the hardened evidence generator with:

- `--require-clean`;
- `--expected-commit <full-sha>`;
- `--verify-tracked-inputs`;
- pinned reference metadata;
- every test result artifact;
- every wheel/binary/package artifact;
- all scenario statuses;
- explicit skip reasons;
- hosted workflow metadata.

The final bundle must contain:

- candidate identity and environment metadata;
- command inventory;
- workflow run/job records;
- raw and normalized test results;
- differential reports;
- interoperability reports;
- resource/security reports;
- artifact inventory and hashes;
- SBOMs and signatures;
- manifest/composition snapshot;
- capability-to-evidence index;
- redaction report;
- summary with pass/fail/not-executed counts;
- final go/no-go decision.

Store the immutable bundle as a GitHub Actions artifact and attach it to the release candidate or final release. A concise human-readable certification record may be committed, but bulky raw artifacts should not be added to normal source history unless repository policy requires it.

Exit gate: a reviewer can reproduce the decision from the retained bundle without relying on commit messages or undocumented local state.

# Workstream 9 — capability-to-evidence audit

For all 148 manifest capabilities:

1. Verify the tier, applicable roles, traffic kinds, platforms, and entry surfaces.
2. Verify every `drop_in` capability references evidence produced for the frozen candidate.
3. Verify each composition claim maps to a passing scenario for that exact topology.
4. Verify parser-only, structural-only, synthetic-only, or unexecuted capabilities are not `drop_in`.
5. Verify external-wire claims use independent interoperability evidence where required.
6. Verify Python claims distinguish canonical package, compatibility package, construction-only objects, and live runtime behavior.
7. Verify unsupported and intentional non-parity entries have accurate diagnostics and migration guidance.
8. Regenerate the report and ensure all counts match documentation.

Any capability lacking required evidence must be demoted before release, followed by a new candidate freeze and certification run.

Exit gate: zero unreferenced or over-classified release-blocking capabilities.

# Workstream 10 — release decision and publication

Prepare a signed go/no-go record containing:

- candidate SHA and tag;
- artifact hashes;
- hosted CI conclusions;
- test totals;
- external scenario totals;
- skipped/not-executed scenarios;
- known limitations;
- manifest counts;
- residual risks;
- reviewer names or automation identities;
- final decision.

## GO conditions

All of the following are mandatory:

- hosted required workflows green on the frozen SHA;
- clean canonical and compatibility wheel installs pass across supported platforms;
- pinned pproxy differential release-blockers pass;
- required third-party interoperability scenarios pass;
- no high-severity open correctness/security defects;
- no unexplained resource leaks;
- artifact hashes, metadata, SBOM, and signatures valid;
- all `drop_in` capabilities reference candidate-specific evidence;
- documentation accurately states modern subset limitations;
- final evidence bundle retained and reviewable.

## NO-GO conditions

Any of the following blocks release:

- missing or red hosted job;
- required external scenario unavailable or unexecuted;
- architecture/package/import mismatch;
- candidate SHA or tracked-input drift;
- stale/missing generated report;
- compatibility wheel mismatch;
- cipher dependency ambiguity;
- resource or cleanup regression;
- over-classified parity entry;
- evidence bundle generation guard failure.

On NO-GO, create a focused corrective plan from the observed failures. Do not expand scope beyond those failures. Any code or manifest change creates a new candidate and restarts this plan.

## Required deliverables

- immutable candidate tag and identity record;
- hosted CI run links and exported metadata;
- complete clean-wheel matrix results;
- pproxy differential result bundle;
- third-party interoperability result bundle;
- security/resource/fuzz result bundle;
- final artifact inventory, hashes, SBOM, signatures, and provenance;
- capability-to-evidence index;
- regenerated parity report;
- final go/no-go record;
- release notes using the exact phrase **modern pproxy compatibility subset**, not strict full parity.

## Acceptance criteria

- The exact frozen candidate is reproducibly identified and unchanged throughout certification.
- All mandatory hosted workflows pass.
- All supported wheels install and operate in clean environments on their declared platforms and Python versions.
- `import pproxy` works only through the explicit compatibility distribution and passes the supported contract subset.
- Native outbound streams pass lifecycle, cancellation, concurrency, and leak tests without temporary listeners.
- Supported AEAD operations pass known-answer and round-trip tests with deterministic dependencies.
- Every required pproxy differential and third-party interoperability scenario has retained passing evidence.
- No unavailable scenario is represented as passing.
- Every `drop_in` capability references candidate-specific evidence.
- Artifact versions, hashes, SBOM, and signatures agree.
- The evidence bundle is complete, redacted, immutable, and reviewable.
- A binary GO or NO-GO decision is recorded.

## Out of scope

- adding SSH, QUIC/H3, SSR, legacy Shadowsocks/OTA, or broader UDP support;
- adding advanced transport listener roles;
- implementing live-path plugin execution;
- redesigning reverse proxying;
- changing the pinned pproxy target;
- adding new release features after candidate freeze.

## Handoff sequencing

1. Freeze and tag the candidate.
2. Run hosted Rust and Python matrices.
3. Build and clean-install all artifacts.
4. Run pinned pproxy differential scenarios.
5. Run third-party interoperability suites.
6. Run security/resource/fuzz certification.
7. Assemble artifacts and evidence.
8. Audit all 148 capability records.
9. Record GO or NO-GO.
10. Publish only on GO; otherwise create a narrowly scoped corrective plan and restart with a new candidate.