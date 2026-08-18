# pproxy Final Corrective Closure Pass

## Status

Ready for implementation.

## Baseline

This corrective pass is based on Eggress `main` at:

- `c02724c297f669e30e34abfba8879fb8ac326b10`
- `fix: close pproxy phase 10 compatibility claims`

The frozen compatibility oracle remains:

- `pproxy==2.7.9`
- upstream repository: `https://github.com/qwj/python-proxy`
- tag commit: `09d4752f17ed6787e1a073c93980eec019887ee3`

The existing implementation is largely retained. This pass is not a new parity roadmap and must not reopen already completed protocol work unless a failing acceptance test demonstrates a concrete defect.

## Objective

Close the remaining evidence, feature-boundary, and documentation inconsistencies identified after implementation of the strict-parity phases so the repository has one defensible final compatibility statement.

The pass has four narrowly scoped work packages:

1. replace weak reverse/backward evidence with real pproxy 2.7.9 payload-level interoperability tests;
2. restore the intended default-feature boundary for SSR/plugin compatibility;
3. reconcile the final compatibility matrix, capability manifest, README, and supporting docs with the actual remaining exclusions/differences;
4. make the Rust 1.85 MSRV decision explicit and intentional.

No new protocol families, generalized certification framework, broad CI expansion, or unrelated refactor is in scope.

---

# Finding 1 — Reverse/backward implementation exists, but external evidence is insufficient

## Current state

The Phase 5 architecture is directionally correct:

- `crates/eggress-protocol-reverse/src/compat_pproxy.rs` is separated from the native Eggress reverse protocol;
- pproxy raw authentication bytes are not newline-framed;
- reconnect/backoff logic exists;
- jump-aware transport setup exists for HTTP CONNECT and SOCKS5;
- compatibility translation marks reverse entries as `pproxy_compat` rather than mutating the native reverse wire protocol.

However, the current gated tests in `crates/eggress-runtime/tests/reverse_interop.rs` do not satisfy the Phase 5 acceptance criteria.

The present external tests mainly demonstrate that a process starts or a connection is attempted. In particular:

- a successful TCP connect to the Eggress control port is not proof that a pproxy-compatible backward handshake completed;
- checking connection/reconnect counters is not payload interoperability;
- scanning a guessed port range after launching pproxy with port `0` is nondeterministic and does not prove the intended pproxy endpoint was found;
- the tests do not demonstrate bidirectional payload equality;
- the tests do not cover a real jump-through topology;
- the tests do not prove recovery after the active backward channel is deliberately broken.

Phase 10 explicitly states that Eggress-to-Eggress roundtrips are regression evidence only. A strict interoperability claim therefore needs a real pproxy process and an actual payload relay.

## Required implementation

### 1. Use the exact pinned pproxy interpreter

Do not depend on an arbitrary `pproxy` executable found on `PATH`.

Introduce or reuse one canonical helper for external pproxy tests:

```text
EGRESS_PPROXY_PYTHON=/path/to/oracle/python
```

The helper must launch:

```text
$EGRESS_PPROXY_PYTHON -m pproxy ...
```

and verify before testing that the interpreter imports the expected frozen package/environment.

If an existing pproxy interpreter helper can be shared with `crates/eggress-cli/tests/interoperability_pproxy.rs`, prefer reuse over adding another environment-variable convention.

External tests must fail clearly when their explicit gate is enabled but the oracle interpreter is unavailable. They may remain ignored by default.

### 2. Allocate explicit ports

Do not launch pproxy on `:0` and search an arbitrary port range.

For each test:

1. obtain a free loopback port with the existing testkit helper;
2. pass that exact port to pproxy;
3. wait only for that exact port/readiness condition;
4. bound readiness with a timeout;
5. capture stderr for failure diagnostics.

This removes accidental matches and nondeterminism.

### 3. Test the compatibility adapter, not the native reverse protocol

The strict external tests must exercise `PproxyBackwardClient` and/or `PproxyBackwardServer` from `compat_pproxy.rs` through the real runtime/translation path where practical.

Do not claim pproxy interoperability from tests that instantiate only the native `ReverseServer` / `ReverseClient` framing.

Native reverse tests must remain unchanged and continue to prove native Eggress behavior separately.

### 4. Add real payload-level interop in both directions

Use a local TCP echo fixture and a deterministic binary payload containing non-text bytes, for example at least 1 KiB with values spanning `0x00..0xff`.

Required scenarios:

#### A. pproxy backward worker -> Eggress compatibility endpoint

- launch the relevant pproxy `+in` topology against an Eggress pproxy-compatible backward endpoint;
- establish the exposed local/external listener path;
- send the deterministic payload through the resulting proxy/reverse path;
- read exactly the same payload back from the local echo target;
- assert byte-for-byte equality;
- assert the process remains healthy for a second independent connection where the topology supports it.

#### B. Eggress compatibility backward worker -> pproxy endpoint

- launch pproxy on an explicitly allocated loopback port;
- connect the Eggress compatibility backward worker to that endpoint;
- establish the externally reachable side of the topology;
- send the same deterministic payload through the complete path;
- assert byte-for-byte equality.

The exact orientation should follow the frozen upstream `ProxyBackward.start_server_run()` and `start_backward_client()` behavior. Do not invent a native Eggress interpretation of `bind://` or `+in` if the tagged source does not use it that way.

### 5. Add one documented jump-through topology

Required minimum:

- backward connection reached through one HTTP CONNECT jump;
- local HTTP proxy fixture only; no public internet dependency;
- pproxy and Eggress must agree on the jump target and final backward endpoint;
- payload equality must be verified end-to-end.

If the implementation already supports SOCKS5 jump-through, retain local regression coverage, but only one externally paired jump topology is required for this corrective pass.

### 6. Add forced disconnect/reconnect verification

Required scenario:

1. establish a working backward channel;
2. successfully relay a payload;
3. deliberately terminate the active control/backward connection or oracle process side without shutting down the Eggress service;
4. observe the compatibility worker reconnect within the configured bounded retry interval;
5. relay a second payload successfully after reconnect;
6. assert no duplicate runaway workers are created.

Do not accept a reconnect-counter increment alone as completion; post-reconnect payload success is required.

### 7. Verify deterministic shutdown

For every external reverse test:

- keep explicit child-process handles;
- terminate and `wait()` on children;
- cancel Eggress workers through their existing cancellation tokens;
- bound joins with timeouts;
- ensure temporary listeners are dropped;
- avoid detached tasks that continue reconnecting after test completion.

No new global task supervisor is needed.

## Reverse/backward acceptance criteria

1. External pproxy tests launch the pinned oracle interpreter, not an arbitrary `pproxy` from `PATH`.
2. No reverse interoperability test scans a guessed port range.
3. At least one payload-level test passes for pproxy -> Eggress compatibility reverse/backward operation.
4. At least one payload-level test passes for Eggress -> pproxy compatibility reverse/backward operation.
5. Both directions verify byte-for-byte payload equality through a local echo target.
6. At least one HTTP-jump backward topology passes end-to-end against real pproxy 2.7.9.
7. A forced disconnect is followed by a successful reconnect and second payload relay.
8. Native Eggress reverse tests remain passing and continue using the native stronger protocol.
9. External child processes and reconnect tasks are deterministically cleaned up.
10. The capability manifest does not promote reverse/backward evidence above what the executed tests actually prove.

---

# Finding 2 — SSR/plugin compatibility is documented as opt-in but enabled by the CLI default `full` feature

## Current state

`crates/eggress-cli/Cargo.toml` currently defines:

```toml
default = ["full"]
full = ["common", "extended", "operations", "reverse", "pproxy-compat", "pproxy-legacy"]
```

This means the pproxy SSR/plugin compatibility path is compiled into the normal default CLI even though the repository documentation repeatedly describes `pproxy-legacy` as an opt-in compatibility boundary.

This is a scope and binary-size inconsistency, not a request to remove SSR support.

`legacy-crypto`, `ssh`, `quic`, and `pproxy-daemon` must remain explicit opt-ins as already designed.

## Required implementation

### 1. Remove `pproxy-legacy` from default `full`

Expected CLI feature topology:

```toml
default = ["full"]
full = ["common", "extended", "operations", "reverse", "pproxy-compat"]
pproxy-legacy = ["eggress-runtime/pproxy-legacy"]
```

Do not remove the feature itself.

### 2. Audit default-feature propagation

Search all workspace crates for:

- `default = ["full"]`;
- `full = [...]`;
- `pproxy-legacy`;
- `legacy-crypto`;
- `ssh`;
- `quic`;
- `pproxy-daemon`.

The intended result is:

- common modern runtime functionality may remain in `full`;
- pproxy translation/runtime compatibility itself may remain in the default CLI if that is the existing product choice;
- SSR/plugin legacy compatibility must require `pproxy-legacy` explicitly;
- stream-cipher/OTA compatibility must require `legacy-crypto` explicitly;
- SSH must require `ssh` explicitly;
- QUIC/H3 must require `quic` explicitly;
- daemon compatibility must require `pproxy-daemon` explicitly.

Do not force these optional features into `full` indirectly through another crate.

### 3. Add feature-off regression tests

At minimum verify:

```bash
cargo check -p eggress-cli --no-default-features --features common
cargo check -p eggress-cli
cargo check -p eggress-cli --features pproxy-legacy
cargo check -p eggress-cli --features legacy-crypto
cargo check -p eggress-cli --features ssh
cargo check -p eggress-cli --features quic
```

Add focused tests ensuring an SSR URI gets a structured unsupported/feature-required diagnostic when `pproxy-legacy` is absent and is accepted when explicitly enabled.

Do not add a combinatorial feature-matrix CI job.

## Feature-boundary acceptance criteria

1. Default `cargo build -p eggress-cli` does not enable `pproxy-legacy`.
2. `pproxy-legacy` remains available as an explicit feature.
3. SSR/plugin compatibility fails closed with a clear diagnostic when the feature is absent.
4. SSR/plugin compatibility still works with `--features pproxy-legacy`.
5. `legacy-crypto`, `ssh`, `quic`, and `pproxy-daemon` remain explicit opt-ins.
6. No broad feature-matrix CI workflow is introduced.
7. README and architecture text describe the actual default feature topology exactly.

---

# Finding 3 — Final compatibility wording hides real remaining supported differences/exclusions

## Current state

The active matrix correctly records several bounded differences, but the closing summary text in the matrix/README currently compresses the remaining exclusions too aggressively.

At minimum, the final public claim must account for these distinct boundaries:

- macOS PF original-destination recovery remains intentional non-parity;
- `cast5-cfb`, `idea-cfb`, `rc2-cfb`, and `seed-cfb` remain unavailable legacy cipher names;
- SSR UDP is not implemented by the bounded SSR compatibility path;
- external/custom SSR plugins are not supported; only the six built-in pproxy 2.7.9 plugin names are in scope;
- QUIC/H3 UDP association / pproxy UDP-over-QUIC composition is not supported;
- backward/reverse TLS composition remains unsupported where the compatibility translator currently rejects it;
- SSH behavior is optional and deliberately warning-bearing because pproxy-compatible host-key verification is disabled;
- daemon behavior is optional/platform-qualified rather than an unconditional default capability;
- legacy stream ciphers/OTA are optional compatibility behavior, not native secure defaults.

These do not all need to be labeled `intentional_non_parity`. Some are legitimate `supported_difference` or feature/platform boundaries. The requirement is consistency and specificity.

## Required implementation

### 1. Reconcile the active machine-readable manifest first

Treat:

- `docs/parity/pproxy_capability_manifest.toml`

as the detailed active inventory.

For each item above:

- verify an explicit record exists;
- verify its status matches reality;
- verify the evidence field references the actual focused test/source evidence;
- do not use self-roundtrip evidence to justify a strict interoperability claim;
- do not promote ignored/unexecuted external tests to `matched`.

### 2. Reconcile the maintained human-readable matrix

Update:

- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`

so the table and final prose agree.

The final paragraph must not say that only PF plus four cipher names remain if other normal pproxy configurations still produce a supported difference or explicit refusal.

A suitable shape is:

> Eggress provides broad behavior-oriented compatibility with pproxy 2.7.9 across the documented HTTP/SOCKS, routing, CLI, modern encrypted-proxy, UDP, reverse, Python, and optional transport workflows. Remaining boundaries are explicitly listed below and include feature-gated legacy/SSH/QUIC behavior, unsupported SSR UDP/external plugins, unsupported QUIC UDP association, unsupported backward-TLS compositions, macOS PF original-destination recovery, and four unavailable legacy cipher names.

This is example wording only. Final text must reflect the manifest after implementation.

### 3. Reconcile top-level README and migration/docs

At minimum inspect and update where necessary:

- `README.md`;
- `docs/PPROXY_PARITY_SPEC.md`;
- `docs/PPROXY_MIGRATION.md`;
- `docs/COMPATIBILITY_EVIDENCE.md`;
- `docs/release/MIGRATION_FROM_PPROXY_FINAL.md`;
- `docs/architecture/pproxy-compat.md`;
- relevant SSH/QUIC/SSR architecture docs.

Do not rewrite historical plan files except for a short completion/status note where they incorrectly present themselves as active authority.

### 4. Preserve one active source-of-truth hierarchy

The final hierarchy should remain:

1. exact frozen upstream 2.7.9 source/provenance;
2. `pproxy_capability_manifest.toml` for detailed active records;
3. `PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md` for maintained human-readable summary;
4. README/migration docs as user-facing summaries.

Historical strict manifests and older phase reports remain provenance only.

## Claim-reconciliation acceptance criteria

1. Every remaining pproxy 2.7.9 incompatibility or observable behavior difference is represented in the active manifest.
2. SSR UDP and external SSR plugins are not silently implied supported.
3. QUIC/H3 UDP association is not silently implied supported.
4. Backward/reverse TLS composition is documented consistently with translator/runtime behavior.
5. macOS PF and the four unavailable legacy cipher names remain explicit.
6. Optional SSH, QUIC/H3, legacy crypto, SSR plugins, and daemon behavior are described as feature/platform qualified.
7. README, matrix, migration docs, and manifest no longer contradict one another.
8. No aggregate `100% parity` percentage is introduced.
9. Phase 10 is only described as fully closed after the external reverse evidence and documentation reconciliation are complete.

---

# Finding 4 — Workspace MSRV increased to Rust 1.85 because of the optional SSH tail; the decision should be explicit

## Current state

The workspace now declares:

```toml
rust-version = "1.85"
```

The increase was introduced with the optional `russh`-based SSH transport. This is technically reasonable, but it is a project-wide compatibility decision caused by an optional tail feature and should be recorded intentionally rather than left as incidental implementation history.

## Required decision

Choose one of the following and document it explicitly.

### Preferred option A — Keep workspace MSRV 1.85

Use this if maintaining a single modern toolchain floor is more valuable than preserving Rust 1.75-era compatibility.

Required actions:

- retain `rust-version = "1.85"`;
- document in `README.md` or the relevant development/architecture document that Rust 1.85 is the supported MSRV;
- state that the increase accommodates the maintained optional SSH dependency stack and modern workspace dependencies;
- remove stale references to 1.75 anywhere in active docs/skills/tests;
- ensure CI/toolchain config actually tests a compiler compatible with the declared MSRV policy where an MSRV check already exists.

Do not add a new complicated MSRV matrix solely for this pass.

### Option B — Restore a lower base MSRV only if it is straightforward

Only pursue this if Cargo/package structure can isolate the optional SSH dependency without pinning an obsolete or weaker SSH stack and without splitting the repository into artificial compatibility packages.

Do not:

- pin an outdated `russh` solely to recover 1.75;
- add a C/OpenSSL SSH dependency;
- duplicate core crates;
- create a parallel workspace;
- spend substantial implementation effort to preserve an old MSRV.

If isolation is not trivial, retain 1.85.

## MSRV acceptance criteria

1. The repository has one explicit documented MSRV decision.
2. Active docs, skills, and Cargo metadata agree on that MSRV.
3. The decision does not require an obsolete SSH stack or C/OpenSSL dependency.
4. No large CI matrix is added.
5. If 1.85 is retained, the change is described as intentional project policy rather than an accidental side effect.

---

# Implementation order

Execute in this order to avoid updating claims before evidence is real.

## Step 1 — Repair reverse/backward external tests

Primary files:

- `crates/eggress-runtime/tests/reverse_interop.rs`;
- `crates/eggress-protocol-reverse/src/compat_pproxy.rs` only if a real interop failure demonstrates a wire bug;
- shared pproxy test helpers in `eggress-testkit` or existing CLI interoperability helpers if reuse is clean.

Do not change production reverse code merely to satisfy a test harness mistake.

## Step 2 — Correct default feature topology

Primary files:

- `crates/eggress-cli/Cargo.toml`;
- any crate manifest found to indirectly force optional parity-tail features into defaults;
- focused compatibility feature-off tests.

## Step 3 — Resolve MSRV policy

Primary files:

- root `Cargo.toml`;
- `README.md` and active developer guidance/skills containing MSRV statements;
- existing toolchain/MSRV checks if present.

## Step 4 — Reconcile claims

Primary files:

- `docs/parity/pproxy_capability_manifest.toml`;
- `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`;
- `README.md`;
- migration/parity/compatibility docs listed above.

Only promote reverse/backward evidence after the real external tests have actually executed successfully.

## Step 5 — Mark this corrective plan complete

After all acceptance criteria pass, add a short result block to this file containing:

- implementation commit range;
- exact external pproxy environment used;
- commands executed;
- remaining intentional exclusions;
- final compatibility claim wording.

Do not add another roadmap unless new defects are discovered outside this pass.

---

# Verification

## Routine local checks

Keep verification proportional to the changed areas.

```bash
cargo fmt --check
cargo test -p eggress-protocol-reverse
cargo test -p eggress-runtime --test reverse_runtime --test reverse_interop
cargo test -p eggress-pproxy-compat
cargo test -p eggress-testkit --lib canonical_manifest
python3 scripts/validate_pproxy_parity_manifest.py --strict \
  docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py \
  docs/parity/pproxy_capability_manifest.toml \
  --check-matrix docs/parity/composition_matrix.toml
```

Run broader workspace tests if production code outside the focused paths changes:

```bash
cargo test --workspace
python3 -m pytest python/tests
```

## External reverse oracle

Use the pinned interpreter and a dedicated explicit gate. Reuse the repository's existing external-interoperability convention where possible.

Example shape:

```bash
EGRESS_PPROXY_PYTHON=.venv-oracle/bin/python \
EGRESS_REQUIRE_REVERSE_INTEROP=1 \
  cargo test -p eggress-runtime --test reverse_interop -- --ignored --test-threads=1
```

If the final implementation unifies this with `EGRESS_REQUIRE_EXTERNAL_INTEROP=1`, document the single canonical command and delete redundant gate logic rather than supporting multiple equivalent gates indefinitely.

## Feature checks

```bash
cargo check -p eggress-cli --no-default-features --features common
cargo check -p eggress-cli
cargo check -p eggress-cli --features pproxy-legacy
cargo check -p eggress-cli --features legacy-crypto
cargo check -p eggress-cli --features ssh
cargo check -p eggress-cli --features quic
cargo check -p eggress-cli --features pproxy-daemon
```

These are local closure checks, not a requirement to create an always-on CI Cartesian matrix.

---

# Final acceptance criteria

This corrective pass is complete only when all of the following are true:

1. Reverse/backward compatibility has real payload-level pproxy 2.7.9 interoperability evidence in both directions.
2. The reverse tests use explicit allocated ports and the pinned pproxy interpreter; no arbitrary port scanning remains.
3. At least one real HTTP-jump backward topology passes against pproxy.
4. Forced backward-channel loss is followed by a successful reconnect and second payload relay.
5. Native Eggress reverse protocol behavior remains separate and passing.
6. `pproxy-legacy` is not enabled by the default CLI `full` feature unless the repository deliberately reverses the opt-in product decision and updates all docs accordingly; preferred resolution is to restore opt-in behavior.
7. `legacy-crypto`, `ssh`, `quic`, and `pproxy-daemon` remain explicit opt-ins.
8. The active manifest accurately records SSR UDP/external-plugin, QUIC UDP-association, backward-TLS, macOS PF, and unavailable-cipher boundaries.
9. README and maintained matrix no longer imply that PF plus four cipher names are the only remaining limitations when other supported differences/refusals exist.
10. Workspace MSRV policy is explicit and consistent; preserving Rust 1.85 is acceptable and preferred over pinning obsolete dependencies.
11. No new broad CI/certification framework, public-internet integration dependency, or unrelated architecture refactor is introduced.
12. Phase 10/final closure is marked complete only after the real external evidence has executed successfully.
13. The final project claim remains precise and behavior-oriented rather than numeric: broad pproxy 2.7.9 compatibility with explicitly enumerated feature/platform boundaries.

## Non-goals

This corrective pass must not expand into:

- macOS PF implementation;
- the four unavailable legacy cipher primitives;
- SSR UDP implementation;
- arbitrary third-party SSR plugin loading;
- QUIC UDP association/MASQUE/WebTransport;
- new reverse protocol features beyond what the frozen pproxy behavior requires;
- restoring Rust 1.75 at significant maintenance cost;
- redesigning CI/release automation;
- introducing a new parity percentage or certification subsystem.

If any of those are desired later, treat them as separate product decisions rather than blockers for this corrective closure.