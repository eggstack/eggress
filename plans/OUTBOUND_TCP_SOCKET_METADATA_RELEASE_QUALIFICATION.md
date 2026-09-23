# Outbound TCP Socket Metadata Release Qualification

## Status

**READY AFTER OUTBOUND_TCP_SOCKET_METADATA_RECOVERY — 2026-09-22**

## Target repository

`eggstack/eggress`

Planning baseline:

`e55f8f3f642bd03a2a6477aff24d7ff837378c09` (`main`, workspace 1.0.8)

Depends on:

- `plans/OUTBOUND_TCP_SOCKET_METADATA_RECOVERY.md`

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
with no `1.0.9` registry release indicated. The package helper's dry-run is
the final release graph/collision qualification; no publication is performed.

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

Not yet executed.
