# Eggfetch 0.2 Corrective Closure

## Status

**READY FOR IMPLEMENTATION — 2026-09-22**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Corrective baseline: `b5193688b2478bb0e9c24279d29e5a3097c03d67`
- Parent plan: [`EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md`](EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md) (**IMPLEMENTED — response-parser Outcome B**)
- Purpose: close the three residual defects found after the Eggfetch 0.2 migration without reopening the completed architecture or changing supported capability.

## Scope

This is one narrow corrective pass with exactly three workstreams:

1. remove the newly introduced 64 KiB outbound CONNECT request-head compatibility limit;
2. retire the now-unnecessary `RUSTSEC-2026-0009` exception by updating the transitive `time` lockfile resolution to a fixed Rust-1.89-compatible release;
3. reconcile the stale `plans/README.md` active-handoff state with the implemented parent plan and this corrective handoff.

No other dependency migration, protocol refactor, parser replacement, capability expansion, API redesign, performance campaign, or parity work is authorized.

---

## Why this corrective exists

The parent migration landed cleanly in the intended Outcome B shape:

- `eggfetch-http-connect 0.2.0` owns outbound H1 CONNECT authority rendering and request serialization;
- Eggress keeps target/credential validation, status policy, response parsing, routing, TLS, chaining, and H2/H3 behavior;
- MSRV moved to Rust 1.89;
- direct Base64 usage moved to 0.23;
- hosted Rust and Python CI are green;
- focused and broad implementation evidence is recorded in the parent plan.

Post-implementation review found three residual issues.

### Residual A — request serialization now rejects a previously accepted input class

Before `b5193688`, the Eggress CONNECT request builder had no independent request-head byte limit. It grew its output buffer as needed.

The migration now supplies:

```rust
const MAX_CONNECT_REQUEST_HEAD: usize = 64 * 1024;
...
ConnectRequest {
    ...
    max_head_bytes: MAX_CONNECT_REQUEST_HEAD,
}
```

to `eggfetch_http_connect::encode_connect_request()`.

That means sufficiently large but otherwise valid credentials/targets which previously serialized can now fail as `HttpError::HeaderTooLarge`. The parent plan explicitly required no accepted-input regression and stated that a new materially smaller request limit must not be introduced when no pre-existing bound exists.

The 64 KiB bound also does not provide a meaningful allocation defense for this adapter: Eggress has already materialized the credential string/Base64 value and Eggfetch materializes authority/request-line strings before the serialized-size check completes. A true request-size hardening policy would therefore need a separately designed input/config bound and is outside this compatibility migration.

### Residual B — the `time` advisory exception is no longer MSRV-blocked

Current `Cargo.lock` resolves:

```text
time 0.3.45
```

and `deny.toml` ignores:

```text
RUSTSEC-2026-0009
```

The exception originally existed because the fixed `time` line required Rust 1.88 while Eggress supported Rust 1.85.

Eggress now has MSRV 1.89. The original reason for the exception is gone. The advisory is fixed in `time >= 0.3.47`, and current `time` releases remain compatible with Rust 1.89.

The lockfile should therefore be updated and the exception removed if the normal dependency graph resolves cleanly.

### Residual C — planning index still advertises the completed parent as active

The parent plan itself is marked `IMPLEMENTED`, and `docs/ROADMAP.md` records the migration as completed.

However, `plans/README.md` still lists:

```text
EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md — READY FOR IMPLEMENTATION
```

under `Active registered handoff`.

That state is stale and contradicts the canonical roadmap.

---

## Governing constraints

1. Do not replace `eggfetch-http-connect` or add `eggfetch-core`.
2. Preserve the implemented Outcome B ownership split.
3. Do not change public Rust, Python, CLI, configuration, URI, protocol, transport, or pproxy compatibility surfaces.
4. Do not change `HttpConnectLimits`.
5. Do not change inbound CONNECT behavior.
6. Do not change H2/H3 CONNECT behavior.
7. Do not change existing credential validation or redaction.
8. Do not add a public request-head-limit configuration knob.
9. Do not introduce a new request-size policy under the guise of this corrective pass.
10. Do not bump `rcgen` or unrelated dependencies unless the lockfile cannot reach a fixed `time` version otherwise.
11. Do not suppress `RUSTSEC-2026-0009` in another configuration location after removing it from `deny.toml`.
12. Do not add new CI jobs, matrices, benchmark gates, or evidence machinery.
13. Keep the parent migration marked implemented; this corrective is a post-implementation closure pass, not a reopening of the migration.

---

## Workstream 1 — Restore request-head compatibility

### Required behavior

For valid inputs accepted by the pre-migration Eggress CONNECT request builder, the shared Eggfetch serializer must not impose a new 64 KiB rejection boundary.

The preferred implementation is to make the upstream serializer's required `max_head_bytes` parameter effectively compatibility-unbounded:

```rust
max_head_bytes: usize::MAX
```

or an equivalently named private compatibility constant.

This preserves Eggfetch's serializer ownership while restoring Eggress' previous absence of an independent request-head size cap.

### Required implementation

1. Remove the private 64 KiB behavioral limit from `connect/client.rs`.
2. Keep `ConnectRequest` + `encode_connect_request()` as the production request serializer.
3. Preserve:
   - local `validate_credentials()`;
   - local Base64 encoding needed for colon-bearing usernames;
   - local target-domain validation;
   - fixed, non-secret `ConnectError -> HttpError` mapping.
4. Update comments/architecture documentation so they no longer claim a 64 KiB compatibility-preserving request-head limit.
5. Do not add a replacement finite limit unless an already-existing Eggress input bound can be demonstrated from the pre-migration public contract.

### Required regression test

Add a focused request-wire regression that would have failed under the 64 KiB cap.

Recommended shape:

1. create a valid username or password large enough that the serialized CONNECT head exceeds 64 KiB;
2. use the existing local canned CONNECT fixture;
3. return a normal `200` response;
4. assert `http_connect()` succeeds;
5. assert the captured request contains the expected `Proxy-Authorization: Basic ...` framing;
6. assert no secret material appears in any error/debug path exercised by adjacent negative tests.

The test should not require hundreds of megabytes; roughly 70–96 KiB of input is sufficient to prove the removed boundary.

Also retain the existing:

- control-character rejection;
- colon-bearing username;
- empty credentials;
- non-ASCII credentials;
- IPv4/IPv6/domain wire tests.

### Explicit non-goal

This corrective does not claim unlimited input is a desirable security policy. It restores the existing contract. If Eggress later wants a bounded credential/request-head policy, that is a separately approved behavior/API hardening change that must define the bound at the input/config layer before allocations occur.

### Acceptance

- [ ] No finite 64 KiB request-head compatibility limit remains in the outbound CONNECT adapter.
- [ ] `eggfetch-http-connect` remains the sole production serializer for the migrated H1 CONNECT path.
- [ ] A >64 KiB valid request regression succeeds.
- [ ] Existing target/auth acceptance and redaction tests remain green.
- [ ] No public limit/config/API is added.

---

## Workstream 2 — Remove the obsolete `time` advisory exception

### Required dependency correction

Update the root `Cargo.lock` so `time` resolves to a non-vulnerable version:

```text
time >= 0.3.47
```

Prefer the smallest compatible lockfile-only update that removes the advisory and avoids unrelated dependency churn.

Suggested first attempt:

```bash
cargo update -p time --precise 0.3.47
```

If Cargo cannot resolve 0.3.47 cleanly, select the newest semver-compatible `time 0.3.x` that:

- is fixed for RUSTSEC-2026-0009;
- supports Rust <= 1.89;
- does not require an unrelated direct dependency bump.

Do not bump `rcgen` merely to make this work unless the existing `rcgen 0.13.2` constraint actually prevents a fixed `time` resolution.

### Standalone lockfiles

Check every committed lockfile, including `fuzz/Cargo.lock`.

- If it contains vulnerable `time <0.3.47`, update it as well.
- If it does not contain `time`, leave it unchanged; do not churn the lockfile merely for symmetry.

### Remove the exception everywhere

After the lockfile is fixed:

1. remove `RUSTSEC-2026-0009` from `deny.toml`;
2. remove it from documented `cargo audit --ignore ...` commands in maintained guidance;
3. repo-wide search for the advisory ID and remove only live suppression/guidance references;
4. historical plan/evidence text may retain the ID where it accurately describes the former state.

Known maintained references currently include:

- `AGENTS.md`;
- `docs/CI_STATUS.md`;
- `deny.toml`.

Review `docs/TESTING.md`, release guidance, and skills for the same command rather than assuming the above list is exhaustive.

### Verification

Run:

```bash
cargo +1.89.0 check --workspace --locked
cargo deny check
cargo audit
```

If other pre-existing advisories require existing documented ignores, retain only those still necessary. The closure condition for this workstream is specifically that `RUSTSEC-2026-0009` no longer needs suppression.

### Stop condition

If updating `time` requires:

- raising Eggress above Rust 1.89;
- bumping an unrelated production dependency family;
- changing production features/API;
- or introducing a new advisory/regression,

stop and document the resolver evidence. Do not trade one compatibility/security issue for another.

### Acceptance

- [ ] Root `Cargo.lock` resolves `time >=0.3.47`.
- [ ] Any other committed lockfile containing vulnerable `time` is corrected.
- [ ] `RUSTSEC-2026-0009` is absent from live audit ignore configuration.
- [ ] Maintained audit commands no longer suppress RUSTSEC-2026-0009.
- [ ] `cargo +1.89.0 check --workspace --locked` passes.
- [ ] `cargo deny check` passes.
- [ ] `cargo audit` passes except for independently documented, still-valid repository exceptions.

---

## Workstream 3 — Reconcile planning state

### Required changes

1. Keep [`EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md`](EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md) marked `IMPLEMENTED`.
2. Update `plans/README.md` so it no longer advertises the parent migration as `READY FOR IMPLEMENTATION`.
3. While this corrective is active, list this corrective plan as the sole active Eggfetch handoff.
4. Register this corrective under `docs/ROADMAP.md` as the current narrow closure item.
5. When this corrective lands:
   - mark this plan `IMPLEMENTED`;
   - remove it from the active subsection in `plans/README.md`;
   - update `docs/ROADMAP.md` to state the Eggfetch line is closed;
   - return `## Next Phase` to no registered handoff unless another plan has separately been approved.
6. Do not create another completion/evidence document.

### Acceptance

- [ ] `plans/README.md`, this plan, the parent plan, and `docs/ROADMAP.md` agree on status.
- [ ] Exactly one active handoff is advertised during implementation: this corrective.
- [ ] No completed plan is labeled ready/active.
- [ ] Closure is recorded in place rather than through a new completion file.

---

## Required verification

Use focused checks first:

```bash
cargo test -p eggress-protocol-http --locked
```

Then dependency/MSRV checks:

```bash
cargo +1.89.0 check --workspace --locked
cargo deny check
cargo audit
```

Before closure:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo check --manifest-path fuzz/Cargo.toml --bins
```

The hosted CI/Python smoke should remain green after push.

### Not required

Do not rerun:

- external pproxy differential solely for changing the private request-size compatibility bound back to prior behavior;
- Shadowsocks interoperability;
- performance benchmarks;
- binary-size measurements;
- soak/load tests;
- release matrices.

Run an external compatibility suite only if implementation changes more than the private request-size correction described here.

---

## Final acceptance criteria

This corrective line is complete only when:

- [ ] valid outbound CONNECT requests are no longer newly rejected at 64 KiB;
- [ ] the shared `eggfetch-http-connect` request serializer remains in place;
- [ ] existing Eggress credential/target/error/redaction behavior remains unchanged;
- [ ] response-parser Outcome B remains unchanged;
- [ ] `time` resolves to a RUSTSEC-2026-0009-fixed version compatible with Rust 1.89;
- [ ] the RUSTSEC-2026-0009 suppression is removed from live policy and documentation;
- [ ] no unrelated dependency/API/capability changes were introduced;
- [ ] focused, exact-MSRV, security, workspace, Clippy, format, and fuzz-compile gates pass;
- [ ] hosted CI remains green;
- [ ] planning/roadmap state is internally consistent and this corrective is closed in place.

## Expected implementation footprint

Expected touched files are narrowly bounded to:

- `crates/eggress-protocol-http/src/connect/client.rs`;
- focused CONNECT tests in the same module/file;
- `Cargo.lock`;
- `deny.toml`;
- maintained audit-command docs/guidance containing the advisory ignore;
- `architecture/protocols-http.md` if the 64 KiB request-limit wording is present;
- `plans/README.md`;
- this corrective plan;
- `docs/ROADMAP.md`.

If implementation begins spreading into runtime, server, routing, TLS, H2/H3, Python, config schema, or parity-manifest code, stop and reassess.
