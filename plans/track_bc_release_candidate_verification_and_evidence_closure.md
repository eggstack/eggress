# Track B/C release-candidate verification and evidence closure

## Objective

Validate the completed Track B/C implementation as a release candidate and close only defects exposed by that validation. This pass is not a feature-expansion phase. It must prove that the current modern pproxy compatibility subset works from clean installations, across supported operating systems and Python versions, with native outbound streams, explicit top-level `pproxy` packaging, deterministic cipher behavior, correct resource ownership, and retained differential/interoperability evidence tied to an exact commit.

The pass is complete only when the release candidate has a reproducible evidence bundle, all required CI lanes are green, every `drop_in` capability is backed by matching composition-specific evidence, and any failed or unexecuted claim is corrected in the manifest before release.

## Baseline

Current `main` includes:

- stream-native WS/WSS, raw/tunnel, and H2 upstream composition;
- H2 pool isolation by endpoint, TLS/auth context, and chain position;
- Rust and Python native outbound connectors;
- synchronous and asynchronous Python outbound stream wrappers;
- `ProxyConnection` using the native connector path rather than temporary listeners;
- a separate `eggress-pproxy-compat` distribution providing top-level `import pproxy`;
- deterministic AEAD dependency policy through `cryptography>=42,<47`;
- clean-wheel compatibility workflow definitions for Linux, macOS, and Windows;
- release packaging for canonical and compatibility wheels;
- a release evidence generator;
- 148 manifest capabilities and a generated parity report.

The remaining risk is not primarily missing implementation. It is unverified execution across platforms, packaging environments, external implementations, and all declared parity cells.

## Release-candidate rules

1. Pin one candidate SHA before running certification.
2. Do not amend or silently replace evidence after code changes; any corrective commit requires a new evidence bundle.
3. Do not report skipped, unavailable, or gated scenarios as passing.
4. Do not promote capabilities during this pass unless stronger evidence is added and reviewed.
5. Demote any `drop_in` capability whose required evidence cannot be executed or does not match actual behavior.
6. Keep source-tree tests separate from installed-wheel tests.
7. Treat commit messages as non-authoritative; only retained command output and generated reports count as certification evidence.

# Workstream 1 — candidate freeze and reproducible environment

## Tasks

1. Record candidate commit SHA, Rust toolchain, Cargo lock hash, Python versions, pproxy version, OS images, architecture, and workflow revisions.
2. Pin `pproxy==2.7.9` in all differential and contract environments.
3. Pin external interoperability tools where possible:
   - `trojan-go` or selected Trojan implementation;
   - reference Shadowsocks implementation;
   - H2 CONNECT reference server/client;
   - WebSocket reference implementation.
4. Define exact Python support matrix from package metadata.
5. Generate a machine-readable environment manifest under the evidence output directory.
6. Add a guard in `scripts/release_evidence.py` that rejects dirty worktrees, mismatched commit SHA, missing reference versions, or untracked wheel inputs.
7. Ensure evidence generation records tool versions and executable hashes.

## Acceptance criteria

- A single immutable candidate SHA is identified.
- All dependency/tool versions are recorded.
- The evidence script fails closed on dirty or mismatched inputs.
- The environment manifest can be regenerated deterministically.

# Workstream 2 — full Rust validation

## Required commands

Run and retain output for:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --workspace --doc
```

Add targeted runs for:

- stream-native WS/raw/H2 chains;
- H2 pool isolation and GOAWAY/RST recovery;
- Trojan listener/upstream and malformed-input tests;
- reverse runtime and interop tests;
- UDP tests;
- configuration/parity matrix validation;
- embed outbound connector tests;
- Python native stream PyO3 tests.

## Corrective criteria

Any failure must be classified as:

- product defect;
- test defect;
- platform assumption;
- environmental dependency;
- flaky timing/resource issue.

Fix product and test defects with direct regression coverage. Do not increase timeouts without establishing the failing state transition.

## Acceptance criteria

- Full workspace test suite passes on Linux.
- Platform compile checks pass for macOS and Windows.
- No ignored release-critical Rust test remains without an executed evidence lane.
- No duplicate helper, stale API call, feature-gated compile, or warning regression remains.

# Workstream 3 — Python source and wheel matrix

## Matrix

Run supported Python versions on:

- Ubuntu x86_64;
- macOS arm64 and/or x86_64 according to release targets;
- Windows x86_64;
- Linux aarch64 if wheels are published.

## Source-tree tests

Run:

```bash
python -m pytest python/tests tests/compat -q
```

with:

- asyncio debug mode for the semantic suite;
- warnings elevated where practical;
- `PYTHONMALLOC=debug` on at least one Linux lane;
- repeated loop/resource tests;
- cryptography installed at the declared version range.

## Clean-wheel tests

For every wheel target:

1. Build the canonical `eggress` wheel.
2. Build `eggress-pproxy-compat` separately.
3. Create a clean virtual environment outside the repository.
4. Install wheels only; do not add repository paths to `PYTHONPATH`.
5. Run with `--import-mode=importlib`.
6. Verify:
   - `import eggress`;
   - `import pproxy`;
   - `from pproxy import Connection, Server`;
   - protocol/cipher module imports;
   - type-stub/package metadata presence;
   - version compatibility between both wheels;
   - native extension resolution from installed wheel.
7. Run native sync/async outbound stream tests.
8. Run representative unchanged pproxy programs.
9. Uninstall and verify the compatibility namespace disappears without damaging `eggress`.

## Acceptance criteria

- All declared Python versions pass.
- Both wheels install together from clean local artifacts.
- No source-tree import masks packaging failures.
- The top-level `pproxy` namespace comes only from the compatibility distribution.
- Canonical `eggress` remains usable with or without the compatibility wheel.
- Version mismatch between wheels fails with an actionable diagnostic.

# Workstream 4 — native outbound stream verification

## Required behavior

Test `OutboundConnector`, `OutboundStream`, `AsyncOutboundStream`, and `ProxyConnection` for:

- direct TCP;
- HTTP CONNECT;
- SOCKS4/4a;
- SOCKS5;
- Shadowsocks;
- Trojan;
- WS/WSS, raw/tunnel, and H2 chains where supported;
- representative two-hop and three-hop compositions;
- IPv4, IPv6, and domain targets;
- timeout and cancellation;
- half-close and full close;
- read/write after close;
- concurrent reads/writes where allowed;
- address metadata;
- context managers;
- loop affinity;
- GIL release;
- destructor/resource warnings;
- repeated create/connect/close cycles;
- no hidden listener bind or listening socket.

## Instrumentation requirements

1. Add a test-only listener/socket census to prove Python outbound connections do not create local listeners.
2. Track live Rust stream/task counts in test builds.
3. Verify cancellation drops pending dial/handshake tasks.
4. Run high-concurrency stress with bounded file descriptors and memory.
5. Test interpreter shutdown with live and closed streams.

## Acceptance criteria

- Native Python connections do not start temporary listeners.
- Sync and async streams have deterministic close semantics.
- Cancellation and timeout release sockets/tasks promptly.
- No descriptor, task, thread, or reference leak is detected.
- Exceptions match documented compatibility classes and timing.

# Workstream 5 — top-level pproxy contract certification

## Tasks

1. Run the C1 extractor and behavioral probes against pinned pproxy and the installed compatibility distribution.
2. Compare:
   - exports;
   - import paths;
   - signatures/defaults;
   - coroutine classification;
   - constructors;
   - object attributes;
   - exception types;
   - `repr`, equality, hashing, truthiness;
   - protocol/cipher registries;
   - server/connection lifecycle.
3. Execute all repository examples and a curated corpus of unchanged pproxy programs.
4. Add import tests for:
   - `pproxy`;
   - `pproxy.server`;
   - `pproxy.proto`;
   - `pproxy.cipher`.
5. Verify unsupported symbols fail through stable, documented exceptions rather than import-time crashes.
6. Verify canonical pproxy and compatibility pproxy cannot be installed together without a clear conflict policy.

## Acceptance criteria

- The certified subset is explicitly enumerated.
- Every certified import/program runs unchanged.
- Unsupported or structural-only objects are excluded from drop-in claims.
- Classification rationales match actual installed-wheel behavior.
- No `internal_observed` classification hides a registry-reachable or returned public object.

# Workstream 6 — cipher policy and behavior

## Tasks

1. Verify package metadata always installs the declared supported AEAD dependency for the compatibility wheel.
2. Test the canonical wheel both with and without the cipher extra.
3. Add known-answer vectors for all supported AEAD methods.
4. Verify nonce increment, overflow handling, tag verification, wrong-key behavior, truncated ciphertext, and repeated-object lifecycle.
5. Compare supported cipher object behavior against pinned pproxy where pproxy exposes equivalent methods.
6. Confirm unsupported legacy ciphers remain explicit and cannot be selected silently.
7. Validate dependency bounds against the oldest and newest supported `cryptography` versions.

## Acceptance criteria

- Compatibility-wheel cipher behavior is deterministic.
- Supported AEAD operations pass known-answer and negative tests.
- Optional canonical behavior is documented and tested.
- Legacy cipher stubs are not counted as behavioral drop-in support.

# Workstream 7 — differential and external interoperability evidence

## Required suites

Execute and retain results for:

- pinned pproxy CLI differential suite;
- HTTP/SOCKS protocol differential scenarios;
- advanced chain differential scenarios where pproxy supports them;
- reverse/backward scenarios;
- Trojan against pproxy and at least one independent implementation;
- Shadowsocks TCP/UDP against an independent implementation;
- WebSocket tunnel against an independent implementation;
- H2 CONNECT against an independent implementation;
- standalone and SOCKS5 UDP behavior;
- platform-specific Unix/transparent behavior where release claims apply.

## Evidence requirements

For each scenario record:

- scenario ID;
- candidate SHA;
- reference implementation/version/hash;
- platform/architecture;
- command lines with secrets redacted;
- start/end timestamps;
- result status;
- normalized observations;
- raw artifact hashes;
- skip/failure reason;
- capability and composition IDs.

A missing external binary must yield `not_executed`, never `pass`.

## Acceptance criteria

- All release-blocking external scenarios execute successfully.
- Evidence artifacts are retained in CI and locally reproducible.
- Every `drop_in` protocol/chain capability references a passing scenario or approved equivalent evidence.
- Any unavailable scenario causes demotion or release blocking.

# Workstream 8 — fuzz, security, and resource smoke

## Tasks

Run bounded fuzz-smoke jobs for:

- URI parsing;
- SOCKS5 handshake;
- HTTP headers/body framing;
- Trojan accept parser;
- WebSocket handshake;
- H2 CONNECT authority;
- reverse handshake;
- UDP framing.

Run security/resource tests for:

- authentication timing paths;
- malformed and oversized frames;
- DNS/private-network policy;
- pool/queue bounds;
- plugin callback backpressure;
- H2 pool isolation;
- repeated Python stream lifecycle;
- process and tempfile cleanup in external harnesses.

## Acceptance criteria

- No crash, panic, unbounded allocation, or obvious leak in smoke duration.
- All fuzz targets compile in CI.
- External test harnesses leave no child processes or temporary files.
- Resource counters return to baseline after stress tests.

# Workstream 9 — evidence bundle and release audit

## Evidence bundle layout

Generate under a commit-specific directory such as:

```text
release-evidence/<commit-sha>/
  environment.json
  manifest-audit.json
  composition-audit.json
  rust/
  python/
  wheels/
  differential/
  interoperability/
  fuzz/
  SHA256SUMS
  summary.md
```

## Tasks

1. Extend `scripts/release_evidence.py` to ingest CI result JSON and artifact hashes.
2. Validate all artifact paths and checksums.
3. Generate a capability-by-capability audit for all 148 entries.
4. Verify generated parity report matches the audited manifest.
5. Identify every release blocker, waiver, skip, or demotion.
6. Produce a final go/no-go document tied to the candidate SHA.
7. Ensure release workflow uploads the evidence bundle alongside wheels and checksums.

## Acceptance criteria

- Evidence bundle is complete, redacted, checksummed, and tied to one SHA.
- Every manifest capability has a final evidence disposition.
- Reports contain no stale counts, superseded claims, or unexecuted pass claims.
- Release artifacts include both wheels and the evidence bundle.

# Workstream 10 — corrective loop and final classification

For every failure:

1. Add a minimal reproduction.
2. Fix the defect in the narrowest layer.
3. Add a regression test.
4. Rerun affected local suites.
5. Rerun the full candidate certification because the SHA changed.
6. Update manifest and reports only after evidence passes.

Do not batch unrelated cleanup into this pass.

## Final release gates

The candidate may receive a go recommendation only when:

- all mandatory GitHub workflows are green;
- full Rust and Python suites pass;
- canonical and compatibility wheels install cleanly on supported platforms;
- `import pproxy` and representative programs work unchanged for the certified subset;
- native Python outbound streams pass lifecycle and leak tests;
- supported AEAD behavior is deterministic;
- all required external suites have retained passing evidence;
- the 148-capability audit has no unsupported `drop_in` claim;
- no high-severity correctness or security defect is open;
- release docs accurately call the result modern/subset parity unless broader tracks are complete.

## Out of scope

- SSH implementation;
- QUIC/H3 implementation;
- SSR or legacy Shadowsocks implementation;
- multi-hop UDP expansion;
- listener-side WS/raw/H2;
- live-path plugin execution unless currently classified as a release blocker;
- new protocol breadth;
- performance optimization unrelated to a release-blocking regression.

## Recommended commit sequence

1. evidence-script and candidate-environment hardening;
2. Rust/Python/wheel test corrections;
3. native stream lifecycle/resource fixes;
4. compatibility-package contract fixes;
5. external harness corrections and execution;
6. manifest/report demotions or promotions based on evidence;
7. final evidence bundle and go/no-go documentation.

The implementation agent should prefer multiple reviewable commits over one omnibus release commit. Every code change after candidate freeze invalidates prior certification output and requires regeneration.