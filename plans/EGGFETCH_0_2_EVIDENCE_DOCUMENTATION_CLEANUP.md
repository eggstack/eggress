# Eggfetch 0.2 Evidence and Documentation Cleanup

## Status

**READY FOR IMPLEMENTATION — 2026-09-22**

## Baseline

- Repository: `eggstack/eggress`
- Branch: `main`
- Planning baseline: `cd18c19c391e72b93d994acf60298e872fce5f0c`
- Parent migration: [`EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md`](EGGFETCH_0_2_HTTP_CONNECT_CONSOLIDATION.md) (**IMPLEMENTED — response-parser Outcome B**)
- Parent corrective: [`EGGFETCH_0_2_CORRECTIVE_CLOSURE.md`](EGGFETCH_0_2_CORRECTIVE_CLOSURE.md) (**IMPLEMENTED — 2026-09-22**)
- Purpose: reconcile the already-landed corrective evidence state and remove stale live audit guidance without reopening runtime, dependency, API, capability, or parity scope.

## Scope

This is a documentation/evidence-only cleanup with three bounded workstreams:

1. reconcile the parent corrective plan's unchecked acceptance criteria with the implementation and verification that already landed;
2. remove stale live `RUSTSEC-2025-0134` cargo-audit ignores now that `rustls-pemfile` is absent from the lockfile;
3. close this polish handoff in the planning index and canonical roadmap after the evidence state is consistent.

No runtime implementation, dependency update, feature change, API change, compatibility change, parser work, benchmark campaign, or security redesign is authorized.

---

## Current state

Commit `cd18c19c391e72b93d994acf60298e872fce5f0c` already landed the corrective runtime/dependency work:

- outbound HTTP/1 CONNECT request serialization remains delegated to `eggfetch-http-connect 0.2.0`;
- the accidental 64 KiB adapter limit was removed in favor of a compatibility-unbounded `usize::MAX` bound;
- a >64 KiB wire regression is present;
- response-parser Outcome B remains unchanged;
- `time` resolves to `0.3.47`;
- the live `RUSTSEC-2026-0009` suppression was removed;
- hosted Rust CI and Python smoke passed on the corrective commit;
- `plans/README.md` and `docs/ROADMAP.md` record the corrective as implemented.

Two evidence/documentation defects remain.

### Residual A — corrective plan status and acceptance boxes disagree

[`EGGFETCH_0_2_CORRECTIVE_CLOSURE.md`](EGGFETCH_0_2_CORRECTIVE_CLOSURE.md) is marked `IMPLEMENTED`, but its workstream acceptance lists and final acceptance criteria remain unchecked.

That makes the detailed plan read as incomplete even though the code, lockfile, roadmap, plan index, and hosted CI show that the corrective landed.

The cleanup must reconcile those checkboxes from real evidence. It must not simply mass-replace `[ ]` with `[x]`.

### Residual B — maintained audit commands still ignore an absent dependency advisory

`deny.toml` correctly has an empty advisory-ignore list and documents that `rustls-pemfile` was replaced by `rustls-pki-types` PEM APIs. `Cargo.lock` no longer contains `rustls-pemfile`.

However, maintained operator/agent guidance still contains cargo-audit commands that ignore:

`RUSTSEC-2025-0134`

That advisory applies to the unmaintained `rustls-pemfile` crate. Since the crate is absent from the current dependency graph, the live command-line ignore is stale and weakens the signal of the documented audit command.

The separate `RUSTSEC-2023-0071` ignore is not part of this cleanup. The current workspace still contains `rsa 0.10.0-rc.18` through the optional SSH stack, and the advisory has no patched release as of this baseline. Keep that exception unchanged and do not investigate or alter SSH/RSA capability in this plan.

---

## Governing constraints

1. Do not change Rust source, Python source, tests, configuration schemas, manifests, protocol behavior, or public APIs.
2. Do not change `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, features, or dependency versions.
3. Do not change the implemented Eggfetch ownership split.
4. Do not change response-parser Outcome B.
5. Do not change pproxy compatibility claims, the capability manifest, or the practical compatibility matrix.
6. Do not reopen the 64 KiB CONNECT correction.
7. Do not remove or alter the live `RUSTSEC-2023-0071` exception as part of this pass.
8. Do not add a replacement ignore for `RUSTSEC-2025-0134` anywhere else.
9. Do not rewrite historical records merely because they mention a formerly relevant advisory; change only live guidance/suppression text or inaccurate current-state claims.
10. Do not create a new completion/evidence bundle after this plan. Close this file in place.

---

## Workstream 1 — Reconcile corrective acceptance evidence

### Required review

Use the current repository state and the corrective commit as the primary evidence source.

For every unchecked acceptance item in
[`EGGFETCH_0_2_CORRECTIVE_CLOSURE.md`](EGGFETCH_0_2_CORRECTIVE_CLOSURE.md):

1. identify the concrete code, lockfile, documentation, CI, or command evidence that satisfies it;
2. mark the item `[x]` only when that evidence exists;
3. leave the item unchecked and stop closure if the evidence cannot be established without new runtime implementation.

Known evidence already present at the baseline includes:

- `COMPAT_UNBOUNDED_REQUEST_HEAD = usize::MAX`;
- production use of `ConnectRequest` + `encode_connect_request()`;
- `test_wire_large_credentials_above_64kib_preserved`;
- existing target/auth/redaction regressions;
- `time 0.3.47` in the root lockfile;
- no live `RUSTSEC-2026-0009` ignore;
- hosted Rust CI success on `cd18c19c...`, covering format, Clippy, workspace tests, optional compile slices, OpenSSH regression, and fuzz-target compile;
- hosted Python smoke success on the same commit;
- consistent implemented state in the parent plan, `plans/README.md`, and `docs/ROADMAP.md`.

### Verification needed for evidence not supplied by hosted CI

Hosted CI does not prove the exact MSRV floor or the local dependency-policy/audit commands. Re-run only the narrow missing evidence:

```bash
cargo +1.89.0 check --workspace --locked
cargo deny check
cargo audit --ignore RUSTSEC-2023-0071
```

Run the audit command after Workstream 2 removes the stale `RUSTSEC-2025-0134` ignore from maintained guidance.

Do not rerun benchmarks, binary-size measurements, external pproxy differential suites, Shadowsocks interoperability, soak/load tests, or release matrices. No runtime or compatibility claim is changing.

### Evidence note

Add a compact implementation/evidence note to the corrective plan if needed so a future reader can tell why the boxes are checked. Prefer a short dated block naming:

- corrective implementation commit;
- hosted CI/Python status;
- exact-MSRV check result;
- `cargo deny check` result;
- `cargo audit --ignore RUSTSEC-2023-0071` result.

Do not paste command transcripts or create a separate evidence file.

### Acceptance

- [ ] Every checked corrective criterion is backed by identifiable evidence.
- [ ] No criterion is checked merely because the plan status says `IMPLEMENTED`.
- [ ] Exact-MSRV and dependency-policy/audit evidence is recorded compactly.
- [ ] The corrective plan's status, acceptance boxes, and evidence note agree.

---

## Workstream 2 — Remove stale RUSTSEC-2025-0134 audit guidance

### Required inventory

Search maintained repository guidance for both the advisory ID and audit command forms, for example:

```bash
rg -n 'RUSTSEC-2025-0134|cargo audit --ignore' \
  AGENTS.md docs .skills .agents .opencode plans
```

Classify each hit as either:

- **live guidance/current-state text** — update it; or
- **historical provenance** — retain it if it accurately describes the former state.

Known live locations include the maintained audit commands in `AGENTS.md`, `docs/CI_STATUS.md`, `docs/TESTING.md`, and release guidance. Treat the repo-wide search as authoritative rather than assuming this list is exhaustive.

### Required live command

Where the current repository documents the dependency/release audit command, it should become:

```bash
cargo audit --ignore RUSTSEC-2023-0071
```

The RSA advisory remains intentionally explicit rather than being hidden in a broad suppression policy.

### deny.toml

`deny.toml` currently has:

```toml
[advisories]
ignore = []
```

and an accurate explanatory comment that `rustls-pemfile` was replaced.

No `deny.toml` change is required unless implementation discovers stale wording that incorrectly describes the current dependency graph. Do not add either advisory to `deny.toml`.

### Historical plans

Historical plan/evidence references to `RUSTSEC-2025-0134` may remain when they accurately document a previous state. Do not rewrite historical records solely to make a repository-wide string search empty.

The closure condition is that no **live** audit command or current suppression policy ignores an advisory for a crate absent from the lockfile.

### Acceptance

- [ ] No maintained/live cargo-audit command ignores `RUSTSEC-2025-0134`.
- [ ] `RUSTSEC-2023-0071` remains the only documented cargo-audit ignore required by this cleanup.
- [ ] `deny.toml` remains free of advisory ignores.
- [ ] Historical provenance is not rewritten unnecessarily.
- [ ] `cargo audit --ignore RUSTSEC-2023-0071` succeeds for the current lockfile.

---

## Workstream 3 — Close planning state

### During implementation

This plan is the sole active registered handoff for this cleanup.

Keep both parent Eggfetch plans marked implemented.

### At closure

When Workstreams 1 and 2 pass:

1. change this plan status to `IMPLEMENTED`;
2. check this plan's acceptance criteria from actual evidence;
3. move this plan from the active subsection of `plans/README.md` to recently completed;
4. change the roadmap entry from active evidence/documentation cleanup to completed maintenance;
5. restore `## Next Phase` to no registered handoff unless another independently approved plan exists;
6. keep the runtime Eggfetch line described as closed.

### Acceptance

- [ ] This plan, both parent plans, `plans/README.md`, and `docs/ROADMAP.md` agree on status.
- [ ] No completed plan remains advertised as active.
- [ ] No new completion document is created.
- [ ] Runtime/API/capability scope remains untouched.

---

## Required verification

This pass is intentionally documentation/evidence scoped.

Required:

```bash
cargo +1.89.0 check --workspace --locked
cargo deny check
cargo audit --ignore RUSTSEC-2023-0071
```

Also inspect the final diff and confirm it contains only documentation/planning/guidance changes.

Hosted CI need not be rerun manually; the normal push-triggered workflows should remain green. If a documentation-only path does not trigger a path-scoped workflow, do not manufacture an unrelated code change to force it.

### Explicitly not required

- full external pproxy differential;
- Shadowsocks external interoperability;
- performance benchmarks;
- binary-size measurements;
- load/soak testing;
- release artifact matrix;
- dependency updates;
- RSA/russh redesign.

---

## Final acceptance criteria

This cleanup is complete only when:

- [ ] the implemented corrective plan no longer contains unexplained unchecked acceptance criteria;
- [ ] each reconciled checkbox is supported by concrete repository/CI/command evidence;
- [ ] live audit guidance no longer suppresses `RUSTSEC-2025-0134`;
- [ ] the current audit command suppresses only the separately documented `RUSTSEC-2023-0071` exception;
- [ ] `cargo +1.89.0 check --workspace --locked` passes;
- [ ] `cargo deny check` passes;
- [ ] `cargo audit --ignore RUSTSEC-2023-0071` passes;
- [ ] no dependency, runtime, API, feature, capability, parity, or compatibility claim changes;
- [ ] planning state is internally consistent and this handoff is closed in place.

## Expected implementation footprint

Expected files are limited to documentation/planning/guidance such as:

- `plans/EGGFETCH_0_2_CORRECTIVE_CLOSURE.md`;
- this plan;
- `plans/README.md`;
- `docs/ROADMAP.md`;
- `AGENTS.md`;
- `docs/CI_STATUS.md`;
- `docs/TESTING.md`;
- release/security/skill guidance containing the live audit command.

If implementation touches `Cargo.toml`, `Cargo.lock`, Rust/Python source, tests, compatibility manifests, or runtime configuration, stop and reassess.
