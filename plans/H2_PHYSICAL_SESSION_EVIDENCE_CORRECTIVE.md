# H2 Physical Session Evidence Corrective

## Status

READY FOR IMPLEMENTATION — FINAL 1.0.10 EVIDENCE CORRECTIVE — 2026-09-24

## Target repository

eggstack/eggress

Corrective baseline: ea5aa3a088fb0010bce58471f36f11c9573be978 on main, workspace 1.0.10.

Depends on:
- plans/ONE_ZERO_TEN_CLOSURE_EVIDENCE_PASS.md
- plans/H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md
- plans/POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md

## Purpose

Correct one remaining overclaim in the 1.0.10 release evidence without reopening the implemented H2/TLS architecture.

The existing regression h2_pool_does_not_cross_distinct_tls_config_instances intends to prove that two distinct valid caller ClientConfig identities cannot share one pooled physical H2 connection.

Its current server fixture increments the accepted counter after tls_accept succeeds but before h2::server::handshake succeeds. That is insufficient because the client-side pooled path does this:

1. TCP connect.
2. TLS wrap the supplied stream.
3. Enter h2_connect_client_pooled_in_registry.
4. Look for an existing pooled H2 entry.
5. On a hit, return the existing H2 connection and discard the supplied TLS stream.
6. Only on a miss call pool.create_entry(stream), which performs h2::client::handshake(stream).

A broken cross-policy pool can therefore accept a second TLS connection, increment the server counter to two, discard that second stream before H2 handshake, and still satisfy executor B over executor A's pooled H2 connection.

The release evidence must count successful server-side H2 handshakes, not only TLS accepts.

Required invariant:

Two distinct valid TLS policy identities targeting the same H2 proxy must result in two successful physical H2 handshakes while both logical CONNECT operations succeed.

## Workstream 0 — reopen qualification state

Before code changes, append-only correct the current status:

- ONE_ZERO_TEN_CLOSURE_EVIDENCE_PASS.md becomes QUALIFICATION EVIDENCE CORRECTIVE REQUIRED.
- H2 TLS corrective remains implementation-complete but physical-H2 evidence pending.
- docs/ROADMAP.md and plans/README.md register this plan as the sole active 1.0.10 evidence corrective.

Preserve all valid evidence:
- supported 28-crate package dry-run;
- crates.io 1.0.10-missing result;
- custom-CA ALPN preservation;
- bind/insecure/nested pooling corrections;
- prior CI runs.

Only the physical H2 isolation proof and final CI are reopened.

## Workstream 1 — strengthen the local TLS/H2 fixture

Refactor the local test fixture so it separately counts:
- successful TLS accepts;
- successful H2 handshakes.

Preferred test-only state is equivalent to a TlsH2ServerCounters struct with AtomicUsize fields tls_accepts and h2_handshakes.

Required ordering inside the server task:

- call tls_accept;
- increment tls_accepts only after tls_accept succeeds;
- call h2::server::handshake;
- increment h2_handshakes only after h2::server::handshake succeeds.

The H2 counter must not increment on TCP accept, TLS accept, executor success, registry identity checks, or any event before the physical H2 session exists.

Keep the fixture hermetic and local.

## Workstream 2 — strengthen h2_pool_does_not_cross_distinct_tls_config_instances

Required sequence:

1. Build two distinct Arc<ClientConfig> objects.
2. Both configs trust the same local test CA.
3. Both use the same endpoint/SNI/auth target.
4. Executor A establishes an H2 CONNECT and keeps its returned stream alive.
5. Wait boundedly until the server reports one successful H2 handshake.
6. Executor B establishes a second logical H2 CONNECT.
7. Keep both returned logical streams valid long enough for the assertion.
8. Wait boundedly for server counters to settle.
9. Assert tls_accepts is at least two.
10. Assert h2_handshakes equals two.

The decisive assertion is h2_handshakes == 2.

Use a bounded polling helper instead of arbitrary sleeps.

Suggested assertion text: distinct valid TLS policy identities must establish distinct physical H2 sessions.

## Workstream 3 — prove the regression is mutation-sensitive

Do not only show that the strengthened test passes.

Demonstrate that it would fail if both executors deliberately shared the same H2 pool registry/policy scope.

Preferred options:

1. Add a test-only control, with no public API change, showing distinct registries produce two H2 handshakes while a deliberately shared registry produces one; or
2. Temporarily apply the equivalent mutation locally, run the target regression, record that it fails, then revert before commit.

If a test-only control can be expressed cleanly, preferred name:
h2_shared_registry_reuses_one_physical_session_test_control

Do not add a production API solely for the control.

## Workstream 4 — correct evidence wording

Append corrections to:
- ONE_ZERO_TEN_CLOSURE_EVIDENCE_PASS.md
- H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md

Explicitly supersede the statement that two TLS accepts alone prove two physical TLS/H2 connections.

Final evidence must say:
- the server observed two successful TLS accepts;
- the server observed two successful H2 handshakes;
- the H2 counter increments only after h2::server::handshake succeeds;
- both logical H2 CONNECTs succeeded.

Keep the older record for provenance but mark the TLS-accept-only proof insufficient.

## Workstream 5 — focused validation

Run directly:
- cargo test -p eggress-outbound --locked h2_pool_does_not_cross_distinct_tls_config_instances -- --nocapture
- cargo test -p eggress-outbound --locked
- cargo test -p eggress-outbound --locked --features insecure-tls
- cargo test -p eggress-transport-tls --locked

Confirm these remain green:
- custom_ca_tls_override_survives_h2_alpn_adaptation
- h2_pool_does_not_cross_tls_trust_policy
- h2_pool_does_not_cross_distinct_tls_config_instances
- h2_hop_zero_same_policy_reuses_physical_connection
- h2_hop_zero_local_bind_is_not_pooled
- h2_hop_zero_insecure_is_not_pooled
- nested_h2_does_not_cross_reuse_prefixes
- tls_override_plus_insecure_fails_closed
- feature-gated insecure variant
- SSH default reuse, local-bind, nested selected-prefix, and cross-prefix regressions

Run the shared-registry control explicitly if one is added.

## Workstream 6 — full requalification

Run:
- cargo fmt --all -- --check
- cargo clippy --workspace --all-targets --locked -- -D warnings
- cargo test --workspace --locked
- cargo deny check
- cargo audit --ignore RUSTSEC-2023-0071
- cargo check --manifest-path fuzz/Cargo.toml --bins

Run the established outbound feature slices and required OpenSSH lane.

Before restoring release-qualified state, rerun:
- scripts/release-preflight.sh --check-versions-only
- cargo metadata --locked --format-version 1
- python3 scripts/publish-crates.py --list
- CARGO_BUILD_JOBS=2 python3 scripts/publish-crates.py --dry-run

The same package rules apply:
- no --allow-dirty;
- no --skip-package-verify;
- no --execute;
- all registry results non-transient;
- all 28 version 1.0.10 crates still missing.

If any 1.0.10 crate has become published, stop and roll forward rather than restoring 1.0.10 release-ready status.

## Workstream 7 — final remote CI

Commit and push the strengthened regression/evidence correction.

Require green Rust CI on the new code SHA.

If Python smoke does not trigger because no Python paths changed, record that fact and retain the prior green Python result for the unchanged Python surface.

Record:
- code SHA;
- Rust CI run ID and result;
- Python run ID/result or path-scoped non-trigger rationale.

## Workstream 8 — restore qualified state only after proof passes

After all gates pass:
- H2_PHYSICAL_SESSION_EVIDENCE_CORRECTIVE.md becomes IMPLEMENTED AND QUALIFIED.
- ONE_ZERO_TEN_CLOSURE_EVIDENCE_PASS.md returns to IMPLEMENTED AND QUALIFIED — 1.0.10 PREPARED, UNPUBLISHED.
- H2 TLS corrective returns to IMPLEMENTED AND QUALIFIED — 1.0.10 PREPARED, UNPUBLISHED.
- parent pooled-transport corrective remains qualified.
- docs/ROADMAP.md and plans/README.md remove the active evidence blocker and list this corrective under recently completed.

Final evidence must explicitly say physical H2 isolation is proven by successful H2 handshake counts, not TLS accept counts.

## Release boundary

Do not create or publish:
- v1.0.10;
- crates.io packages;
- PyPI packages;
- GitHub Release or binaries.

Tagging remains a separate maintainer-authorized release action.

Expected successful end state:
- workspace 1.0.10;
- strengthened physical H2 policy-isolation regression green;
- supported package dry-run green;
- Rust CI green;
- v1.0.10 absent;
- crates.io 1.0.10 absent;
- release state qualified, prepared, unpublished.

## Expected files touched

Likely:
- crates/eggress-outbound/src/executor.rs
- plans/H2_PHYSICAL_SESSION_EVIDENCE_CORRECTIVE.md
- plans/ONE_ZERO_TEN_CLOSURE_EVIDENCE_PASS.md
- plans/H2_TLS_OVERRIDE_ALPN_PRESERVATION_AND_1_0_10_QUALIFICATION_CORRECTIVE.md
- docs/ROADMAP.md
- plans/README.md

No version bump or production API change is expected.

## Non-goals

- no H2 pool implementation redesign;
- no TLS policy redesign;
- no new public executor/pool API;
- no nested pooling restoration;
- no dependency upgrade;
- no release publication;
- no 1.0.11 unless 1.0.10 becomes immutable before closure.

## Stop conditions

Stop and document a blocker if:
- two distinct valid TLS policy scopes do not produce two successful H2 handshakes;
- the fixture cannot distinguish pool reuse from candidate-stream discard;
- strengthening the proof requires a production API change;
- package verification fails;
- any 1.0.10 registry version is already present;
- final Rust CI fails.

Do not restore qualified status by relying on TLS accept counts alone.

## Acceptance criteria

This corrective is complete only when:

1. The local TLS/H2 fixture separately counts TLS accepts and successful H2 handshakes.
2. The H2 counter increments only after h2::server::handshake succeeds.
3. Executor A's H2 connection remains alive during executor B's connect.
4. Executor B also completes its logical H2 CONNECT successfully.
5. The server observes at least two TLS accepts.
6. The server observes exactly two successful H2 handshakes.
7. The test is shown mutation-sensitive using a test control or recorded local mutation proof.
8. Prior custom-CA/trust/insecure/bind/nested regressions remain green.
9. Full fmt/Clippy/workspace/dependency/audit/fuzz/feature/OpenSSH gates pass.
10. The supported 28-crate package dry-run passes again.
11. All 1.0.10 crates remain absent from crates.io at final qualification.
12. The new code SHA has green Rust CI.
13. Python smoke is green or explicitly unchanged/path-scoped.
14. Evidence records explicitly supersede TLS-accept-only proof.
15. Roadmap and plan registry identify no active 1.0.10 blocker after closure.
16. 1.0.10 remains untagged and unpublished.
17. Release publication remains separately authorized.

## Completion record

Not yet executed.
