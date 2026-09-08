# Architecture Convergence Phase 1 — Correctness Closure

## Status

**PLANNED**

## Baseline

- Repository: `eggstack/eggress`
- Roadmap: `plans/ARCHITECTURE_CONVERGENCE_ROADMAP.md`
- Baseline reviewed commit: `93a205c387b502db1ceba8c9fd1a50c5de9e8e37`

## Objective

Close the demonstrated correctness and truthfulness defects before any structural refactor. This phase must produce behavioral fixes and regression coverage, not architectural expansion.

The pass owns five areas:

1. reload truthfulness for startup-captured listener state;
2. canonical secret redaction;
3. Python stream compatibility semantics;
4. Cargo package verification in the manual release helper;
5. active compatibility/capability documentation that contradicts current implementation.

## Scope constraints

- Do not implement generalized hot listener reconfiguration.
- Do not add a new config state container merely to preserve currently unsupported reload behavior.
- Do not add new release workflows or hosted CI jobs.
- Do not replace the Python stream layer wholesale.
- Do not create a new parity manifest.
- Do not broaden compatibility claims while correcting documentation.
- Prefer declaring a setting restart-required over retaining a false hot-reload claim.

---

## Workstream 1 — Make listener reload semantics truthful

### Problem

`classify_reload_config()` currently accepts changes to multiple listener properties even though runtime accept loops capture those properties from startup-prepared listener state. The snapshot/admin generation may therefore move forward while new connections still use startup values.

Relevant categories include, at minimum:

- `protocols`;
- listener auth material and auth mode;
- listener TLS certificate/key/ALPN configuration;
- Shadowsocks listener configuration;
- Trojan listener configuration/fallback;
- `connection_limit`;
- `fixed_target`;
- `local_bind`;
- UDP settings that are captured into listener/relay runtime state rather than read dynamically.

### Implementation direction

1. Audit every field in `eggress_config::compile::ListenerConfig` and `CompiledListenerUdpConfig` against the running listener code.
2. Classify each field into exactly one category:
   - socket topology / definitely restart-required;
   - startup-captured runtime behavior / restart-required for this phase;
   - dynamically read through the current snapshot/routing service / hot-reloadable.
3. Update `classify_listeners()` so every startup-captured field is rejected when materially changed.
4. Remove or correct log messages that describe those rejected settings as hot-swappable.
5. Update comments in `supervisor.rs`, `architecture/runtime.md`, `architecture/config.md`, and any active reload documentation to reflect the actual boundary.
6. Keep routing rules, upstream/group selection, health configuration, PAC/static content, and other already-proven snapshot-driven behavior hot-reloadable where the runtime demonstrably consumes current state.
7. Do not use coarse whole-struct equality if it makes unrelated future fields accidentally restart-required without review. Prefer explicit comparison groups with comments explaining ownership.

### Required tests

Add tests that validate classification and actual data-plane behavior, not only generation arithmetic.

At minimum:

- changing listener protocols is rejected;
- changing listener auth username/password/type is rejected;
- adding/removing/changing listener TLS material is rejected;
- changing Shadowsocks or Trojan listener configuration is rejected;
- changing `connection_limit`, `fixed_target`, or `local_bind` is rejected;
- changing startup-captured UDP limits/settings is rejected;
- a routing-only change remains accepted;
- an upstream/health-only change remains accepted;
- on a rejected reload, generation and active behavior remain unchanged;
- on an accepted routing reload, a new connection observes the new route.

Where practical, start the supervisor and establish a connection before and after reload so the test proves data-plane behavior rather than only inspecting `classify_reload_config()`.

### Acceptance criteria

- No field documented as hot-reloadable is known to be captured only at listener startup.
- Listener metadata reported through status/admin cannot claim that a rejected configuration became active.
- The reload suite contains at least one end-to-end accepted-reload behavior test and one end-to-end rejected-listener-change behavior test.
- Existing routing/health reload behavior remains intact.

---

## Workstream 2 — Centralize credential-bearing URI redaction

### Problem

`eggress-embed` walks TOML values and uses its own URI-scheme heuristic before applying a local redactor. The local scheme list can lag the set of supported/accepted URI forms. Secret handling must not depend on maintaining parallel protocol whitelists.

### Implementation direction

1. Make `eggress_uri::redact_proxy_uri()` the canonical tolerant authority/userinfo redactor for arbitrary URI-like strings.
2. In `eggress-embed`, remove the local `redact_uri()` implementation and the protocol-scheme whitelist used solely to decide whether a string may contain URI credentials.
3. When walking TOML string values for redaction, use a tolerant generic test such as the presence of `://`, then pass the string through the canonical redactor. The redactor must safely return unchanged text when no userinfo is present.
4. Keep direct secret-key redaction (`password`, token-like fields, etc.) because structured secret values are not necessarily URI strings.
5. Search the CLI, compatibility translator, diagnostics, evidence/report helpers, Python wrappers, and admin surfaces for secondary username/password URI redactors. Replace duplicate implementations where doing so is low risk and directly relevant to secret safety.
6. Do not conflate redaction with URI validation: malformed user input may still require best-effort credential scrubbing before errors are logged.

### Required tests

Cover at least:

- HTTP/SOCKS URI credentials;
- SSH credentials;
- SSR/optional compatibility URI credentials if accepted by the parser;
- H3/QUIC/other optional schemes accepted by the URI layer;
- username-only authorities;
- passwords containing raw `@` where the canonical redactor is designed to handle them;
- IPv6 bracketed authorities;
- malformed but obviously credential-bearing strings used in diagnostics;
- non-URI strings remain unchanged unless their TOML key itself is secret-bearing.

Add a direct regression test for `EggressConfig::to_redacted_toml()` demonstrating that an upstream URI using a previously omitted scheme cannot expose username/password text.

### Acceptance criteria

- `eggress-embed` has no independent proxy-scheme whitelist for credential redaction.
- There is one canonical URI authority redactor used by the active Rust logging/display paths covered by this pass.
- Tests prove redaction for at least one credential-bearing scheme that the old embed whitelist omitted.
- Existing structured secret-field redaction remains intact.

---

## Workstream 3 — Correct Python asyncio compatibility contracts

### Problem

`CompatibleStreamReader.readline()` delegates to `readuntil()` and therefore raises `IncompleteReadError` when EOF arrives after unterminated data, instead of returning that final data like `asyncio.StreamReader.readline()`. `__aiter__` is declared `async def`, which is incorrect for current async iterator protocol semantics.

### Implementation direction

1. Change `CompatibleStreamReader.__aiter__` to a normal method returning `self`.
2. Implement `readline()` so:
   - it returns through and including `\n` when present;
   - it returns remaining buffered bytes on EOF without `\n`;
   - it returns `b""` when already at clean EOF with no buffered data.
3. Preserve `readuntil()` semantics independently; do not weaken its documented error behavior merely to implement `readline()`.
4. Review `__anext__()` against the corrected `readline()` semantics so clean EOF stops iteration and a final unterminated line is yielded once.
5. Compare `read(0)`, `read(-1)`, `readexactly(0)`, `readexactly(n)` EOF behavior, writer close/wait semantics, and `get_extra_info()` against the limited compatibility contract claimed by this adapter. Fix only demonstrated contract mismatches uncovered by this focused review.

### Required tests

Add Python tests for:

- line ending with newline;
- final line without newline;
- clean EOF;
- multiple lines followed by unterminated final line;
- `async for` iteration over newline-terminated lines;
- `async for` yielding the final unterminated line exactly once;
- `readuntil()` still raising `IncompleteReadError` when its separator is missing at EOF;
- no warnings about `__aiter__` returning an awaitable.

Where feasible, compare the adapter and `asyncio.StreamReader` against the same byte sequences rather than encoding expectations only by hand.

### Acceptance criteria

- Adapter `readline()` behavior matches the stdlib behavior for the tested EOF cases.
- `__aiter__` synchronously returns the iterator.
- Existing pproxy compatibility tests still pass.
- No unrelated Python API redesign is introduced.

---

## Workstream 4 — Restore package verification to manual crates.io publication

### Problem

The active release documentation requires Cargo package dry-run verification, but `scripts/publish-remaining.sh` invokes `cargo publish --no-verify`. A recent package-layout defect demonstrated that package verification is valuable for this workspace.

### Implementation direction

1. Remove `--no-verify` from `scripts/publish-remaining.sh`.
2. Make dry-run mode perform actual Cargo verification for each package.
3. Preserve dependency-order publication and the project's manual release policy.
4. Do not add crates.io publication to GitHub Actions.
5. If package verification makes the helper's current dependency-index assumptions invalid in dry-run mode, adjust the helper minimally so each package is verified in an order/state Cargo can resolve. Do not build a custom package manager.
6. Ensure comments and `docs/release/RELEASE_PROCESS.md` agree with helper behavior.
7. Consider a lightweight script self-check that asserts the publish command does not contain `--no-verify`; do not add a hosted workflow for this.

### Required verification

Run, for a representative leaf crate, intermediate crate, and top-level public crate:

```bash
cargo publish --dry-run -p <crate>
```

If feasible within local registry resolution, run the helper's own dry-run mode. If all 26 dry-runs are excessively slow, the implementation summary may record representative verification plus the command/path review; this phase does not require creating new CI automation.

### Acceptance criteria

- No active crates.io publish helper passes `--no-verify`.
- Active release docs and helper behavior agree.
- The helper remains manual and dependency ordered.
- No new release workflow is added.

---

## Workstream 5 — Correct active compatibility/capability truth sources

### Problem

Current implementation and human-facing compatibility documentation have drifted. Examples observed during review include Trojan fallback implementation versus stale incomplete documentation, and transparent-proxy diagnostics that overstate TPROXY support relative to the capability ledger.

### Implementation direction

1. Treat `docs/parity/pproxy_capability_manifest.toml` as the canonical active capability contract per `AGENTS.md`.
2. For every claim changed in this pass, inspect current implementation and focused tests before editing the manifest.
3. Correct human-facing diagnostic strings that overstate platform support. Do not upgrade the manifest merely to match an optimistic message.
4. If Trojan fallback is implementation- and test-backed at the required compatibility tier, update the active capability record and associated practical docs consistently.
5. If TPROXY remains incomplete, diagnostics must distinguish supported REDIRECT/original-destination behavior from unimplemented/incomplete TPROXY behavior.
6. Update only directly affected active docs. Historical plan/completion files are not sources of truth and need not be rewritten.

### Required tests/checks

- existing Trojan fallback regression tests pass;
- compatibility diagnostic tests assert the corrected platform wording;
- any manifest validation/check already present for changed entries passes;
- run the relevant differential/oracle test only if a compatibility tier is upgraded.

### Acceptance criteria

- Active diagnostic text does not claim TPROXY is fully supported unless code and capability manifest prove it.
- Trojan fallback status in active capability documentation agrees with tested implementation.
- No new capability manifest is introduced.

---

## Phase verification

Run the focused suites while iterating. At phase closure run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-runtime --test reload
cargo test -p eggress-embed
cargo test -p eggress-pproxy-compat
cargo test --workspace --locked
```

For Python-facing fixes, also run the active Python smoke suite from a built extension as documented in `AGENTS.md`.

Run external pproxy differential checks only if compatibility tiers/claims are materially upgraded in Workstream 5.

## Phase acceptance criteria

Phase 1 is complete only when:

- all startup-captured listener changes are either correctly made dynamic or, preferably for this phase, explicitly restart-required;
- reload regression coverage exercises running data-plane behavior;
- canonical URI redaction protects schemes previously omitted by embed-local logic;
- Python stream EOF/iteration semantics are corrected and tested;
- crates.io helper publication no longer bypasses verification;
- active compatibility/platform documentation agrees with tested implementation;
- broad Rust and relevant Python gates pass;
- no structural refactor from later phases is pulled forward unless strictly required to fix a correctness defect.

## Closure record

When implemented, update this file in place with:

- `IMPLEMENTED` status;
- implementation commit range;
- concise list of listener fields classified restart-required;
- location of redaction and asyncio regression tests;
- release-helper verification performed;
- any capability manifest entries changed and the evidence used.