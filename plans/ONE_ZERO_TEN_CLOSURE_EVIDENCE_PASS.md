# 1.0.10 Closure and Evidence Reconciliation Pass

## Status

**IMPLEMENTED AND QUALIFIED — 1.0.10 PREPARED, UNPUBLISHED — 2026-09-24**

## Target repository

`eggstack/eggress`

Closure baseline:

`99e8cf57860ce5aac689c09053d2090922e376eb` (`main`, lockstep workspace 1.0.10)

Depends on:

- `plans/H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md`
- `plans/POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md`

## Post-closure evidence correction — 2026-09-24

A final review found that the physical H2 isolation regression counts TLS
accepts before server-side H2 handshake establishment. Because the pooled H2
client checks for an existing entry before calling h2::client::handshake on the
supplied TLS stream, a candidate TLS stream can be accepted and then discarded
on a pool hit. Qualification is therefore reopened only for the stronger
physical-H2 proof and final CI.

Closure is delegated to
[H2_PHYSICAL_SESSION_EVIDENCE_CORRECTIVE.md](H2_PHYSICAL_SESSION_EVIDENCE_CORRECTIVE.md).

All other closure evidence remains valid unless the corrective uncovers a new
defect.

## Purpose

Close the remaining evidence and bookkeeping gaps on the prepared 1.0.10 tree
without reopening the implemented transport architecture.

The runtime/TLS correction at the baseline is materially complete:

- caller-owned `rustls::ClientConfig` policy survives ALPN adaptation;
- custom-CA TLS+H2 succeeds through the caller override;
- `tls_override + insecure=true` fails closed;
- H2 pooling remains scoped by TLS policy identity;
- explicit-bind/insecure/nested H2 remains unpooled;
- SSH bind/nested reuse restrictions remain intact;
- Rust CI and Python smoke on the baseline SHA are green.

This pass exists because the current evidence record still contains three
problems:

1. the package dry-run record cites the unsupported command
   `python3 scripts/publish-crates.py --dry-run --allow-dirty`;
2. `h2_pool_does_not_cross_tls_trust_policy` proves the untrusted executor
   fails its TLS handshake, but TLS happens before H2 pool acquisition, so that
   test alone does not prove physical pool separation;
3. the error/docs say callers may “supply an explicit insecure override,” but
   `OutboundExecutorOptions` has no separate insecure-override field.

The canonical roadmap and parent plans also still describe qualification as
pending even though remote CI on the implementation SHA has succeeded.

The closure invariant is:

> Release evidence must claim only what the tests actually prove, and 1.0.10
> may be called qualified only after the supported package dry-run succeeds on
> the committed release-preparation tree.

---

# Baseline evidence to preserve

Record these already-completed remote checks rather than rerunning them merely
to discover their result:

- implementation SHA:
  `99e8cf57860ce5aac689c09053d2090922e376eb`;
- GitHub Actions CI run `36006795053`: **success**;
- GitHub Actions Python smoke run `36006795043`: **success**;
- no `v1.0.10` tag exists at planning time.

The implementation plan already records:

- `cargo test --workspace --locked`: 2,979 passed, 151 ignored;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean;
- `cargo fmt --all -- --check`: clean;
- dependency/audit/fuzz gates: green;
- OpenSSH regression lane: green;
- custom-CA H2 ALPN test: green;
- explicit bind/insecure/nested H2 and SSH regressions: green.

Do not delete those historical records. Correct only claims that are
unsupported or stale.

---

# Workstream 0 — freeze release state

Before editing:

```sh
git rev-parse HEAD
git status --short
git tag --list 'v1.0.10'
scripts/release-preflight.sh --check-versions-only
cargo metadata --locked --format-version 1 >/dev/null
```

Expected:

- HEAD is the current implementation baseline or a descendant containing only
  this closure work;
- workspace remains lockstep 1.0.10;
- 27 exact internal pins remain aligned;
- no `v1.0.10` tag exists.

Also query crates.io/read-only registry state for at least:

- `eggress-core 1.0.10`;
- `eggress-outbound 1.0.10`;
- `eggress-embed 1.0.10`.

If any 1.0.10 artifact is already immutable/published, stop and write a
roll-forward plan rather than qualifying this tree for 1.0.10.

---

# Workstream 1 — add a real physical H2 policy-isolation regression

Keep the existing
`h2_pool_does_not_cross_tls_trust_policy` test, but narrow its documented
claim:

- it proves executor B performs/fails its own certificate verification;
- in conjunction with policy-scoped registry construction, it guards the
  fail-closed trust boundary;
- it does **not**, by itself, prove that H2 pool lookup would otherwise have
  reused executor A's connection, because TLS wrapping occurs before the H2
  handler is entered.

Add one real TLS+H2 regression in which both executors can complete TLS, so pool
reuse would be observable.

Preferred exact test name:

`h2_pool_does_not_cross_distinct_tls_config_instances`

Test shape:

1. start the existing local TLS/H2 CONNECT fixture;
2. create two distinct `Arc<ClientConfig>` instances;
3. both configs trust the same local test CA and can complete TLS successfully;
4. keep executor A's first H2 physical connection alive/eligible for reuse;
5. executor B connects to the same endpoint/SNI/auth target using its distinct
   config object;
6. the TLS/H2 server must observe a second physical TLS/H2 connection;
7. both logical H2 CONNECT operations must succeed.

The test must therefore prove:

```text
same endpoint + same SNI + same auth + both TLS policies valid
+ distinct caller ClientConfig identity
=> two physical H2 connections
```

This directly verifies the registry-scope boundary without relying on a TLS
failure before pool lookup.

Use a local accepted-connection or successful-TLS/H2-handshake counter.

Do not satisfy this test with pointer comparisons alone.

If a stronger mTLS version is trivial using existing fixture support, it is
acceptable, but do not expand scope merely to add a new certificate subsystem.

---

# Workstream 2 — correct trust-boundary evidence wording

Update comments and plan evidence around
`h2_pool_does_not_cross_tls_trust_policy`.

Required wording distinction:

- `custom_ca_tls_override_survives_h2_alpn_adaptation` proves caller custom
  trust survives ALPN adaptation;
- `h2_pool_does_not_cross_tls_trust_policy` proves an untrusted executor
  performs/fails its own TLS verification instead of succeeding through a
  foreign policy path;
- `h2_pool_does_not_cross_distinct_tls_config_instances` proves distinct
  policy identities do not share one reusable physical H2 connection.

Do not claim the certificate-failure test alone demonstrates H2 pool lookup
behavior.

Update the 21-item acceptance mapping in the H2 TLS corrective accordingly.

---

# Workstream 3 — correct custom override + insecure guidance

Current runtime behavior is intentionally fail-closed, but wording such as:

```text
supply an explicit insecure override
```

suggests an API that does not exist.

Clarify the supported contract everywhere it appears.

Preferred semantics:

```text
A caller-supplied tls_override cannot be combined with per-hop
insecure=true.

If the caller intentionally owns an insecure ClientConfig, pass that config as
tls_override and do not also request the Eggress per-hop insecure mode.
Otherwise remove tls_override and use Eggress's feature-gated insecure policy.
```

Update:

- runtime error string in `eggress-outbound/src/executor.rs`;
- `OutboundExecutorOptions::with_tls_override` docs;
- outbound README;
- Rust/embed API docs;
- architecture/outbound and architecture/transports-tls;
- relevant skills/maintainer guidance;
- corrective-plan completion wording.

The error must remain secret-free and fail before substituting any different
TLS policy.

Add/update the existing fail-closed assertion so the test verifies the new
message/semantic category without depending on an unnecessarily exact full
string.

---

# Workstream 4 — run the supported publisher dry-run

This is mandatory closure evidence.

The current publisher accepts:

```sh
python3 scripts/publish-crates.py --list
python3 scripts/publish-crates.py --dry-run
```

It does **not** accept `--allow-dirty`.

After the code/documentation correction is committed and the working tree is
clean:

```sh
git status --short
python3 scripts/publish-crates.py --list
CARGO_BUILD_JOBS=2 python3 scripts/publish-crates.py --dry-run
```

Requirements:

1. working tree is clean before qualification;
2. helper reports workspace version 1.0.10;
3. all 28 publishable crates are discovered in dependency order;
4. workspace-wide `cargo package --workspace --exclude eggress-bench --locked`
   verification succeeds;
5. local patch configuration is used only for verification of unpublished
   exact-version internal dependencies;
6. every registry query returns a non-transient result;
7. 1.0.10 is reported missing for every crate intended for this release;
8. final output reports `dry-run OK: no uploads performed`.

Do not use:

- `--allow-dirty`;
- `--skip-package-verify`;
- `cargo publish --no-verify`;
- `scripts/publish-crates.py --execute`.

Record the exact command, return status, crate count, and registry result.

If any registry lookup is transient, rerun later; do not mark package
qualification complete.

---

# Workstream 5 — focused closure verification

Run the newly added physical-isolation regression directly plus the relevant
TLS tests:

```sh
cargo test -p eggress-transport-tls --locked
cargo test -p eggress-outbound --locked
cargo test -p eggress-outbound --locked --features insecure-tls
```

Explicitly confirm these tests exist and pass:

- `custom_ca_tls_override_survives_h2_alpn_adaptation`;
- `h2_pool_does_not_cross_tls_trust_policy`;
- `h2_pool_does_not_cross_distinct_tls_config_instances`;
- `tls_override_plus_insecure_fails_closed`;
- `tls_override_plus_insecure_fails_closed_with_insecure_tls_feature`;
- `h2_hop_zero_same_policy_reuses_physical_connection`;
- `h2_hop_zero_local_bind_is_not_pooled`;
- `h2_hop_zero_insecure_is_not_pooled`;
- `nested_h2_consumes_selected_prefix`;
- `nested_h2_does_not_cross_reuse_prefixes`;
- SSH default-reuse, source-bind, nested selected-prefix, and cross-prefix
  regressions.

If exact names differ, record the exact final names rather than preserving a
planned name.

Because this pass changes runtime error text and adds one regression but should
not alter behavior, a full workspace rerun is still required before closure:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny check
cargo audit --ignore RUSTSEC-2023-0071
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Run required outbound feature slices and the OpenSSH lane using the repository's
current qualification commands.

---

# Workstream 6 — remote CI evidence

Push the closure implementation commit to `main`.

Require green:

- normal Rust CI;
- Python smoke.

Record:

- implementation/closure code SHA;
- CI run ID and conclusion;
- Python smoke run ID and conclusion.

Do not require a production tag to call the tree release-qualified.

A `v1.0.10` tag is a release action because the repository's tag workflows
publish Python/binary artifacts. Qualification must finish **before** any tag.

---

# Workstream 7 — reconcile the H2 TLS corrective plan

Append a final closure record to
`H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md`.

Correct the stale/incorrect entries:

- replace the unsupported `--dry-run --allow-dirty` record with the actual
  supported dry-run result;
- mark remote CI/Python smoke complete using their real run IDs;
- narrow the claim attached to
  `h2_pool_does_not_cross_tls_trust_policy`;
- add the physical two-valid-policy H2 isolation regression;
- correct the insecure-override wording;
- state whether 1.0.10 remained absent from crates.io during final dry-run;
- state that no tag/publication occurred.

Final status only after all gates pass:

```text
IMPLEMENTED AND QUALIFIED — 1.0.10 PREPARED, UNPUBLISHED
```

Retain prior implementation records as historical evidence; append corrections
rather than silently deleting them.

---

# Workstream 8 — reconcile the parent roll-forward plan

Update
`POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md`.

Its current blocked status is stale after the delegated TLS corrective closes.

Final status:

```text
IMPLEMENTED AND QUALIFIED — 1.0.10 PREPARED, UNPUBLISHED
```

Append a closure note referencing:

- the H2 TLS corrective final SHA;
- new physical policy-isolation regression;
- supported package dry-run;
- final CI/Python run IDs;
- no 1.0.10 tag/publication.

Do not rewrite the historical 1.0.9 audit trail.

---

# Workstream 9 — canonical roadmap and plans registry cleanup

After qualification succeeds, remove the active corrective section as a live
blocker and move both 1.0.10 plans into completed/recently completed state.

`docs/ROADMAP.md` must state unambiguously:

- pooled transport policy identity correction: complete;
- H2 TLS override ALPN correction: complete;
- workspace 1.0.10: qualified;
- 1.0.10: prepared but unpublished;
- publication/tagging remains separately maintainer-authorized.

`plans/README.md` must agree exactly.

Remove stale claims such as:

- “release-qualification pending”;
- “CI green is the next gate”;
- “v1.0.10 tag is the next qualification gate”;
- “fresh 1.0.9 requalification remains pending.”

A tag is not a qualification prerequisite.

---

# Workstream 10 — final non-mutating release handoff

At the end, provide maintainers with the exact release-ready checks only:

```sh
python3 scripts/publish-crates.py --list
scripts/release-preflight.sh --tag v1.0.10
```

The tag preflight is read-only with respect to release publication.

Do **not**:

- run `python3 scripts/publish-crates.py --execute`;
- create/push `v1.0.10`;
- publish PyPI packages;
- create a GitHub Release.

The repository should finish this pass in:

```text
1.0.10 source tree: qualified
crates.io 1.0.10: unpublished
v1.0.10: absent
PyPI/GitHub binary release: not triggered
```

unless a separate maintainer instruction explicitly authorizes release.

---

## Expected files touched

Likely:

```text
crates/eggress-outbound/src/executor.rs
crates/eggress-outbound/README.md
architecture/outbound.md
architecture/transports-tls.md
docs/RUST_API.md
docs/EMBED_API.md
.skills/embed-outbound/skill.md
.skills/rust-proxy-dev/skill.md
plans/H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md
plans/POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md
plans/ONE_ZERO_TEN_CLOSURE_EVIDENCE_PASS.md
docs/ROADMAP.md
plans/README.md
```

No version bump is expected.

---

## Non-goals

- no H2 pool architecture redesign;
- no new TLS configuration API;
- no separate insecure-override field in 1.0.10;
- no nested H2/SSH pooling restoration;
- no dependency upgrade campaign;
- no new compatibility/parity claim;
- no 1.0.11 bump unless 1.0.10 becomes immutable before closure;
- no release publication or tag creation.

---

## Stop conditions

Stop and document the blocker if:

- the two-valid-policy TLS/H2 regression observes only one physical
  connection;
- package verification fails for any crate;
- any 1.0.10 registry version is already present unexpectedly;
- the publisher returns transient registry state after bounded retries;
- final Rust CI or Python smoke fails;
- the working tree cannot be made clean for the supported dry-run;
- correcting the insecure guidance reveals a real API behavior mismatch.

Do not paper over a failed package or pool-isolation gate with documentation.

---

## Acceptance criteria

This closure pass is complete only when:

1. a real TLS+H2 regression proves two distinct valid `ClientConfig`
   identities do not share one physical H2 connection;
2. the certificate-failure trust-boundary test's claim is narrowed to what it
   actually proves;
3. custom-CA ALPN preservation remains green;
4. custom override + per-hop insecure still fails closed;
5. runtime/docs no longer imply a separate insecure-override API exists;
6. all existing pooled H2/SSH route/bind regressions remain green;
7. full fmt/Clippy/workspace/dependency/audit/fuzz/feature/OpenSSH gates pass;
8. the supported `python3 scripts/publish-crates.py --dry-run` succeeds on a
   committed clean 1.0.10 tree;
9. all 28 crates pass package verification;
10. registry state for 1.0.10 is non-transient and absent for the release set;
11. final implementation SHA has green Rust CI;
12. final implementation SHA has green Python smoke;
13. no `v1.0.10` tag exists;
14. no crates.io/PyPI/GitHub release mutation occurs;
15. the H2 TLS corrective is marked implemented and qualified;
16. the parent pooled-transport roll-forward is marked implemented and
    qualified;
17. `docs/ROADMAP.md` and `plans/README.md` agree that 1.0.10 is prepared,
    qualified, and unpublished;
18. stale “qualification pending” / “tag is next qualification gate” wording
    is removed;
19. final evidence records use exact commands/test names/run IDs that actually
    exist;
20. release publication remains a separate maintainer-authorized action.

## Completion record

Executed 2026-09-24. Closure commit
`7b532b9037c41838bc0a96970ba5967aea67e1c5` (code + trust/insecure
wording) followed by the evidence-reconciliation commit on `main`.

1. ✅ `h2_pool_does_not_cross_distinct_tls_config_instances` proves two
   distinct valid `ClientConfig` identities do not share one physical
   H2 connection (server observes exactly 2 accepted handshakes; both
   logical H2 CONNECTs succeed).
2. ✅ `h2_pool_does_not_cross_tls_trust_policy` narrowed to its
   fail-closed trust-boundary claim (code comment + plan records).
3. ✅ `custom_ca_tls_override_survives_h2_alpn_adaptation` green.
4. ✅ `tls_override_plus_insecure_fails_closed` (+ gated
   `..._with_insecure_tls_feature`) green; new fail-closed message
   names the supported contract without implying a separate
   insecure-override field.
5. ✅ runtime/docs/skills no longer imply a separate insecure-override
   API (`executor.rs`, outbound README, `architecture/outbound.md`,
   `docs/RUST_API.md`, `docs/EMBED_API.md`, embed-outbound skill).
6. ✅ pooled H2/SSH route/bind regressions green
   (`cargo test --workspace --locked`: 2980 passed, 151 ignored).
7. ✅ fmt clean; `cargo clippy --workspace --all-targets --locked
   -- -D warnings` clean; `cargo deny check` ok;
   `cargo audit --ignore RUSTSEC-2023-0071` clean (yanked warnings
   only); fuzz bins compile; outbound no-default base/`toml`/
   `pproxy-compat`/`ssh`/`ssh,pproxy-compat`/`udp` slices compile;
   embed `ssh`/`pproxy-compat`/`ssh,pproxy-compat` slices compile;
   OpenSSH lane 6 passed (`EGRESS_REQUIRE_OPENSSH_TESTS=1`).
8. ✅ `CARGO_BUILD_JOBS=2 python3 scripts/publish-crates.py --dry-run`
   exit 0 on the committed clean `1.0.10` tree (no `--allow-dirty`,
   no `--skip-package-verify`, no `publish --no-verify`, no `--execute`).
9. ✅ all 28 crates pass package verification via the helper.
10. ✅ registry state non-transient: all 28 crates `1.0.10`-missing on
    crates.io during final dry-run.
11. ✅ Rust CI `36015875160` success on `7b532b9`.
12. ✅ Python smoke `36006795043` success on `99e8cf5` (path-scoped
    workflow; closure touches no Python-smoke paths, so no new run was
    triggered and the prior green stands).
13. ✅ no `v1.0.10` tag exists (`git tag --list 'v1.0.10'` empty).
14. ✅ no crates.io/PyPI/GitHub release mutation.
15. ✅ H2 TLS corrective marked implemented and qualified (closure
    record appended).
16. ✅ parent roll-forward marked implemented and qualified (closure
    note appended).
17. ✅ `docs/ROADMAP.md` and `plans/README.md` agree: 1.0.10 prepared,
    qualified, unpublished; no live blocker section remains.
18. ✅ stale "qualification pending" / "tag is next qualification gate" /
    "fresh 1.0.9 requalification remains pending" wording removed.
19. ✅ evidence records use exact commands/test names/run IDs.
20. ✅ release publication remains separately maintainer-authorized;
    handoff checks only: `python3 scripts/publish-crates.py --list`,
    `scripts/release-preflight.sh --tag v1.0.10` (read-only; not run
    here beyond `--check-versions-only`, which is OK).

Final state: 1.0.10 source tree qualified; crates.io 1.0.10
unpublished; `v1.0.10` absent; PyPI/GitHub binary release not
triggered.

## Physical-H2 evidence correction — 2026-09-24

The completion record above (item 1) states the server observes
"exactly 2 accepted handshakes". That wording is superseded: the
fixture at that time incremented its counter after `tls_accept`
succeeded but before `h2::server::handshake` succeeded, so two TLS
accepts alone did not prove two physical H2 sessions — a pooled hit
discards the candidate TLS stream before H2 handshake.

Corrected proof (see
`H2_PHYSICAL_SESSION_EVIDENCE_CORRECTIVE.md`):

- the server observed two successful TLS accepts;
- the server observed two successful H2 handshakes;
- the H2 counter increments only after `h2::server::handshake`
  succeeds;
- both logical H2 CONNECTs succeeded;
- `h2_shared_registry_reuses_one_physical_session_test_control`
  proves mutation sensitivity (shared registry → 1 H2 handshake;
  distinct registries → 2).

The older TLS-accept-only record is retained above for provenance but
is marked insufficient as a physical-session proof.

## Qualification restored — 2026-09-24

`H2_PHYSICAL_SESSION_EVIDENCE_CORRECTIVE.md` is now
`IMPLEMENTED AND QUALIFIED` (implementation `c5826b3`; Rust CI
`36032622797` success; supported 28-crate dry-run exit 0, all
`1.0.10`-missing; no `v1.0.10` tag/publication). This plan returns to
`IMPLEMENTED AND QUALIFIED — 1.0.10 PREPARED, UNPUBLISHED`. No active
1.0.10 evidence blocker remains.
