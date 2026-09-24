# Outbound TCP Socket Metadata Release Qualification

## Status

**HISTORICAL — v1.0.9 TAGGED/RELEASED; SUPERSEDED BY 1.0.10 ROLL-FORWARD — 2026-09-24**

## Target repository

`eggstack/eggress`

Planning baseline:

`e55f8f3f642bd03a2a6477aff24d7ff837378c09` (`main`, workspace 1.0.8)

Depends on:

- `plans/OUTBOUND_TCP_SOCKET_METADATA_RECOVERY.md`

## Post-qualification audit — 2026-09-23

A follow-up review found two reuse-related correctness gaps after the original
1.0.9 qualification:

1. SSH and H2 reusable connection keys contain endpoint/auth/hop-index identity
   but not the preceding chain prefix. At hop index greater than zero, a cache
   hit can discard the newly selected prefix stream and reuse a physical
   connection established through a different prefix. This is tracked by
   [POOLED_TRANSPORT_ROUTE_ISOLATION_CORRECTIVE.md](POOLED_TRANSPORT_ROUTE_ISOLATION_CORRECTIVE.md).
2. At hop 0, legitimate SSH/H2 reuse can discard the freshly opened TCP socket
   whose addresses were captured by the new metadata path. Returning those
   addresses would describe a discarded candidate rather than the physical
   connection carrying the result. This is tracked by
   [OUTBOUND_POOLED_TRANSPORT_METADATA_TRUTHFULNESS_CORRECTIVE.md](OUTBOUND_POOLED_TRANSPORT_METADATA_TRUTHFULNESS_CORRECTIVE.md).

The earlier qualification evidence remains valid for the code that was tested,
but the release disposition is superseded: **1.0.9 is not publishable until
this release qualification is rerun on the final corrective SHA.** Both
corrective plans are implemented at `dfc19a0390e241f5255c8ad78dc2b50e214e537f`.
No 1.0.9 tag or registry publication has occurred.

## Post-release state correction — 2026-09-24

The earlier “unpublished / requalification required” status is no longer the
current release state for all release surfaces.

After the corrective plans were closed, annotated tag `v1.0.9` was created at
`e10dea18300f2618c4a47fa46280f1bf518e7a5f`. GitHub Release
`eggress v1.0.9` was published, and the tag-triggered Python publication and
CLI binary release workflows both completed successfully. Ordinary CI on that
commit also completed successfully.

Native crates.io publication remains a separate manual path in this repository
and must be verified independently; this historical plan must not infer its
state from the Git tag, GitHub Release, PyPI workflow, or package dry-run.

A later audit found remaining hop-zero reusable-transport policy-identity gaps
(H2 executor/TLS scope, H2 local-bind/insecure policy, SSH local-bind policy)
and evidence-record defects. Those cannot be repaired in immutable/tagged
1.0.9. The active roll-forward handoff is
[`POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md`](POOLED_TRANSPORT_POLICY_IDENTITY_AND_1_0_10_ROLLFORWARD.md).

Do not move or recreate `v1.0.9`. Any corrected release must use the next
unused patch version, expected 1.0.10.

## Objective

Prepare the socket-metadata correction for an immutable crates.io patch release
so downstream consumers can adopt it from the registry.

The implementation phase deliberately does not publish or tag. This phase
handles version coherence, packaging/release qualification, and the exact
handoff required for a maintainer-operated release.

If the workspace is still 1.0.8 when this phase begins, the expected next
version is 1.0.9. If another release has landed first, compute the next patch
version from current `main`; do not reuse an already-published version.

No release tag may be pushed and no crates.io upload may be executed by this
plan without an explicit maintainer instruction.

---

## Why a release phase is required

The metadata fix spans at least:

- `eggress-core`, where the actual TCP socket metadata is captured before
  boxing;
- `eggress-outbound`, where that metadata becomes `OutboundInfo`.

Eggress uses exact internal version pins and a lockstep workspace release
policy. Published crate versions are immutable. A consumer pinned to
`eggress-outbound = "=1.0.8"` cannot receive the correction from source
changes on `main`.

Therefore downstream closure requires a new published patch version after the
implementation is qualified.

---

## Fixed constraints

1. Keep the repository's lockstep workspace version policy.
2. Do not selectively republish a modified 1.0.8 crate.
3. Do not introduce Git/path dependencies for downstream consumers.
4. Do not use `--no-verify` or `--allow-dirty`.
5. Do not push a `v*` tag as part of ordinary implementation handoff.
6. Do not run `scripts/publish-crates.py --execute` without explicit
   maintainer approval.
7. PyPI and binary release workflows fire on production tags; avoid accidental
   publication.
8. Preserve Rust 1.89.
9. The release carries the metadata behavior correction only; do not bundle
   unrelated API/capability work merely because a patch release is being
   prepared.

---

# Workstream 0 — require implementation closure first

Before any version change, verify
`OUTBOUND_TCP_SOCKET_METADATA_RECOVERY.md` is marked implemented and records:

- implementation SHA;
- green focused core/outbound/embed tests;
- green outbound feature slices;
- workspace Clippy/tests;
- actual direct and first-hop `local_addr` / `peer_addr` evidence;
- no second metadata-only DNS lookup;
- remaining `None` semantics for legitimate non-TCP transports.

If the implementation plan is not closed, stop here.

Capture the pre-release SHA and current workspace version.

---

# Workstream 1 — select the next immutable patch version

Query current repository/version state and crates.io availability.

If current workspace version is 1.0.8 and 1.0.9 is unused, select 1.0.9.

If version state has moved, select the next unused patch version instead.
Never overwrite or attempt to replace a published version.

### Selection record

Selected `1.0.9`: `cargo search eggress --limit 10` reports `eggress-core`,
`eggress-outbound`, and the workspace facade crates at latest version `1.0.8`,
with no `1.0.9` registry release indicated. The package helper lists all 28
publishable crates. Its first package verification at 1.0.8 confirmed why the
new patch is needed: `eggress-outbound` compiled against immutable
`eggress-core 1.0.8`, which does not contain the additive metadata methods.
The publisher dry-run verifier is being corrected to patch internal
dependencies to the current workspace sources during verification only; no
upload command or registry mutation is used.

Record the selected version in this plan's completion record before editing
version files.

---

# Workstream 2 — apply the standard lockstep version bump

Use the repository's existing release policy.

Update the lockstep version locations described in `AGENTS.md`:

1. root `[workspace.package].version`;
2. every internal exact `=x.y.z` pin under `[workspace.dependencies]`;
3. `crates/eggress-python/pyproject.toml`;
4. `python-pproxy-compat/pyproject.toml`, including its exact
   `eggress==x.y.z` dependency;
5. `python/pyproject.toml` local-development version for alignment.

Regenerate/update `Cargo.lock` normally.

Do not hand-edit generated dependency resolutions beyond what Cargo requires.

Run immediately:

```sh
scripts/release-preflight.sh --check-versions-only
cargo metadata --locked --format-version 1 >/dev/null
```

The preflight must report all exact internal pins aligned.

---

# Workstream 3 — package and registry-graph qualification

Use the existing graph-derived publishing helper in non-mutating modes:

```sh
python3 scripts/publish-crates.py --list
python3 scripts/publish-crates.py --dry-run
```

Requirements:

- topological order resolves successfully;
- every publishable workspace crate reports the selected lockstep version;
- package verification succeeds without `--no-verify`;
- no path-only/unpinned internal registry edge appears;
- the helper identifies the release as unpublished rather than colliding with
  an existing immutable version.

Do not execute:

```sh
python3 scripts/publish-crates.py --execute
```

unless the maintainer separately asks for publication.

---

# Workstream 4 — release-facing behavioral qualification

Re-run the metadata-specific contract on the version-bumped tree:

```sh
cargo test -p eggress-core --locked
cargo test -p eggress-outbound --locked
cargo test -p eggress-embed --locked --test outbound_detailed
cargo test -p eggress-embed --locked --test public_api
```

Required outbound feature slices:

```sh
cargo check -p eggress-outbound --locked --no-default-features
cargo check -p eggress-outbound --locked --no-default-features --features toml
cargo check -p eggress-outbound --locked --no-default-features --features pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features ssh
cargo check -p eggress-outbound --locked --no-default-features --features ssh,pproxy-compat
cargo check -p eggress-outbound --locked --no-default-features --features udp
```

If the implementation strengthened Python metadata tests:

```sh
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pytest python/tests/test_outbound_stream_verification.py -q
```

Repository release qualification:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo deny check
cargo audit --ignore RUSTSEC-2023-0071
```

The existing RSA advisory exception remains governed by current repository
policy; do not broaden advisory ignores for this release.

Run fuzz compile if current release policy still requires it:

```sh
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Do not run pproxy oracle/differential suites unless the implementation changed
a compatibility claim. Socket metadata population is not itself a pproxy
parity change.

---

# Workstream 5 — release notes and downstream handoff

Update current release-facing documentation only where the project normally
records patch behavior. Do not create a new compatibility claim.

The release note/handoff should state:

- listener-free TCP `OutboundInfo.local_addr` now reports the actual local
  socket address for direct and TCP-backed proxy-chain routes;
- `peer_addr` is now derived from the actual connected socket rather than a
  separate metadata-only resolution;
- chain metadata describes the first-hop transport;
- non-TCP first hops may continue to return `None`;
- public method signatures and `OutboundInfo` field types are unchanged.

Record the exact published-consumer requirement:

```text
Downstream consumers must require the new patch version of eggress-outbound;
1.0.8 cannot contain this correction because crates.io releases are immutable.
```

For Eggsec specifically, the downstream corrective can remove its unknown
`local_addr` sentinel only after the selected patch version is actually
visible on crates.io and has been separately qualified in Eggsec. Do not modify
Eggsec from this repository.

---

# Workstream 6 — maintainer-operated publication boundary

At the end of this plan, the repository should be release-ready but
unpublished unless explicit maintainer instruction says otherwise.

Record the exact manual commands a maintainer would use later:

```sh
python3 scripts/publish-crates.py --list
python3 scripts/publish-crates.py --execute
```

If a production tag/release is desired, the maintainer must separately follow
the tag workflow and run the tag preflight against the exact commit:

```sh
scripts/release-preflight.sh --tag vX.Y.Z
```

Remember: pushing a `v*` tag triggers PyPI and binary release workflows.
Never push it casually or as an incidental closeout action.

After an authorized crates.io publish, verify at minimum that the selected
versions of `eggress-core` and `eggress-outbound` are registry-visible
before telling downstream consumers to adopt them. Because the workspace
publisher is lockstep/graph-derived, record the complete publication outcome.

---

## Expected files touched

Likely:

```text
Cargo.toml
Cargo.lock
crates/eggress-python/pyproject.toml
python-pproxy-compat/pyproject.toml
python/pyproject.toml
README.md or existing release-facing documentation if appropriate
docs/ROADMAP.md
plans/README.md
plans/OUTBOUND_TCP_SOCKET_METADATA_RELEASE_QUALIFICATION.md
```

Do not churn unrelated crate manifests individually; workspace inheritance and
the root exact pins own the version policy.

---

## Non-goals

- no runtime redesign;
- no new metadata API beyond the implementation plan;
- no resolver injection;
- no dependency upgrade unrelated to packaging the patch;
- no parity-manifest status change;
- no automated crates.io publication;
- no tag push;
- no PyPI/TestPyPI publication;
- no GitHub Release creation;
- no Eggsec change in this repository.

---

## Stop conditions

Stop and record the blocker if:

- the implementation phase is not fully green;
- the selected patch version is already present on crates.io;
- lockstep version preflight fails;
- package dry-run cannot resolve the internal graph;
- metadata behavior differs between source qualification and packaged crates;
- a release requires weakening cargo verification or using dirty/unverified
  flags.

Roll forward to a new patch version rather than trying to overwrite an
immutable published version.

---

## Acceptance criteria

This phase is complete when:

1. the metadata implementation phase is closed;
2. a new unused patch version is selected;
3. all lockstep version locations are aligned;
4. `release-preflight.sh --check-versions-only` passes;
5. Cargo metadata resolves with the locked graph;
6. `publish-crates.py --list` succeeds;
7. `publish-crates.py --dry-run` succeeds;
8. metadata-specific core/outbound/embed tests are green on the version-bumped
   tree;
9. required outbound feature slices compile;
10. Python metadata tests are green if touched;
11. fmt, workspace Clippy, workspace tests, dependency policy, audit policy,
    and required fuzz compile are green;
12. release-facing docs describe the behavior without changing parity claims;
13. no tag has been pushed and no registry mutation has occurred unless
    separately authorized;
14. the completion record contains the final SHA, selected patch version, all
    qualification evidence, and the exact manual publication handoff.

## Completion record

Selected release: `1.0.9` (lockstep workspace version).

Phase 1 implementation commit: `253370450dc76c16aa1a3987591089010183d3b3`.
Phase 1 closure commit: `874de4b6d0c2a7ec639de7b9c94bdee9c309df9b`.
Release preparation commit: `fc47c2dba39c2a8a7b1ef31a6daa46b67fb6ad37`.

Qualification evidence on the 1.0.9 tree:

- `scripts/release-preflight.sh --check-versions-only`: passed; workspace,
  Python bindings, both Python packages, and all 27 exact internal pins align.
- `cargo metadata --locked --format-version 1`: passed.
- `python3 scripts/publish-crates.py --list`: passed; 28 publishable crates
  in dependency order.
- `CARGO_BUILD_JOBS=2 python3 scripts/publish-crates.py --dry-run`: passed;
  all packed crates verified and every 1.0.9 registry query returned
  `missing`. No upload was performed. The helper applies temporary Cargo
  config patches to use matching workspace sources when verifying the
  unpublished internal graph; it does not alter packed manifests or publish
  behavior.
- `tests/scripts/test_publish_crates.py`: 17 passed.
- Focused tests: core 119 passed, outbound 15 passed, embed detailed 23
  passed, and embed public API 5 passed.
- Outbound no-default feature slices (`base`, `toml`, `pproxy-compat`, `ssh`,
  `ssh,pproxy-compat`, `udp`): all passed.
- Python extension rebuilt for 1.0.9; outbound metadata suite: 43 passed,
  11 optional skips.
- `cargo fmt --all -- --check`, workspace Clippy with `-D warnings`, and
  `cargo test --workspace --locked`: passed.
- `cargo deny check`: passed. `cargo audit --ignore RUSTSEC-2023-0071`:
  passed with the existing allowed yanked warnings for `der 0.8.0` and
  `wnaf 0.14.0`.
- `cargo check --manifest-path fuzz/Cargo.toml --bins`: passed; fuzz lockfile
  was updated to workspace version 1.0.9.

No compatibility tier or parity claim changed. Direct TCP metadata describes
the established socket; chain metadata describes its TCP-backed first hop;
non-TCP first hops may still report `None`. Existing public method signatures
and `OutboundInfo` field types are unchanged.

The release is prepared but unpublished. No tag, crates.io upload, PyPI
publication, or GitHub Release was created. Downstream consumers must require
`eggress-outbound` 1.0.9 or newer once it is registry-visible; 1.0.8 is
immutable and cannot contain this correction.

Maintainer handoff after separate publication authorization:

```sh
python3 scripts/publish-crates.py --list
python3 scripts/publish-crates.py --execute
```

Any production tag must separately pass `scripts/release-preflight.sh
--tag v1.0.9`; pushing it triggers the PyPI and binary release workflows.
