# Phase 24 Plan: Evidence Consistency Cleanup

## Purpose

Phase 23 materially improved the repo's evidence discipline: manifest validation exists, overclaimed features were downgraded, compatibility evidence has a canonical document, pproxy and Shadowsocks CI workflows exist, and standalone UDP has direct runtime tests.

A few narrow inconsistencies remain. Phase 24 is a targeted cleanup pass to remove the last visible contradictions before continuing with new pproxy parity feature work.

The goal is not to add new protocol capability. The goal is to make the current compatibility story internally consistent, mechanically validated, and easy to audit.

## Current observed issues

### 1. Manifest date semantics are wrong or confusing

`tests/compat/pproxy_manifest.toml` currently has:

```toml
last_updated = "2025-06-30"
```

That value is stale relative to the current project date. The validator also has confusing semantics: `StaleLastUpdated` is described as a date that appears current/recent, and `is_recent_date()` warns for years `>= 2025`. This is backwards for a normal `last_updated` field.

### 2. `-ul` / `-ur` manifest entries contradict parity docs

The manifest still classifies `udp_listen_flag` and `udp_remote_flag` as `intentional_non_parity` and says Eggress uses SOCKS5 UDP ASSOCIATE instead.

But current parity docs and implementation say `-ul` and `-ur` translate to standalone pproxy UDP mode and UDP upstream config.

This needs a single truth.

### 3. Standalone UDP evidence references are muddy

`docs/PARITY_MATRIX.md` marks standalone UDP relay compatible but points at `differential_socks5_udp_associate`, which sounds like SOCKS5 UDP ASSOCIATE rather than standalone `-ul` semantics.

New standalone runtime tests exist, and the completion doc references 7 gated differential tests, but the matrix and manifest should name standalone-specific tests or classify the evidence precisely.

### 4. CI exists but visible green status is not yet established

The workflows are present and better configured, but release-grade claims should not rely only on committed statements. The repo should have a short CI status note that distinguishes:

- configured workflow;
- locally run validation;
- hosted CI observed green;
- manually gated external interop.

### 5. Compatibility evidence document is hand-maintained

`docs/COMPATIBILITY_EVIDENCE.md` says it is generated from the manifest, but the current implementation appears hand-maintained. That is acceptable temporarily, but the text should either become truly generated or say it is manually synchronized from the manifest.

## Non-goals

Do not implement new pproxy features in this phase.

Do not implement UDP multi-hop, Trojan server, SSH, transparent proxying, HTTP/2, HTTP/3, QUIC, WebSocket, raw tunnel, system proxy integration, or Python drop-in API parity.

Do not relax the evidence rules added in Phase 23.

Do not mark standalone UDP as fully pproxy-compatible unless standalone-specific pproxy differential evidence exists and is named.

## Work items

### 24.1 Fix or remove manifest `last_updated`

Choose one of two approaches.

Preferred approach: remove `last_updated` from `tests/compat/pproxy_manifest.toml` entirely.

Rationale:

- A hand-maintained timestamp is easy to stale.
- Generated reports can carry run timestamps.
- The manifest is versioned by Git history.

Required changes for preferred approach:

- Remove `last_updated` from the manifest.
- Remove `last_updated` from `ManifestMeta`, unless backward compatibility is needed.
- Delete `StaleLastUpdated` and `is_recent_date()` from `crates/eggress-testkit/src/manifest.rs`.
- Remove tests that depend on a warning for `last_updated`.
- Update docs to say report timestamps live in `target/compat/pproxy-parity-report.json`.

Alternative approach: keep `last_updated` and make it sane.

Required changes for alternative approach:

- Set `last_updated = "2026-06-30"`.
- Rename `StaleLastUpdated` to something accurate, or invert the logic to warn only for old dates.
- Define accepted format as `YYYY-MM-DD`.
- Add a validator test for stale dates and current dates.

Acceptance:

- No obviously stale date remains.
- Validator semantics match the field name.
- The manifest validator no longer emits misleading warnings.

### 24.2 Reconcile `-ul` and `-ur` manifest entries

Audit implementation and tests for pproxy UDP flag translation.

Expected outcomes:

If `-ul` and `-ur` are implemented and tested:

- Change `udp_listen_flag` from `intentional_non_parity` to the correct status.
- Change `udp_remote_flag` from `intentional_non_parity` to the correct status.
- Update divergence notes to describe standalone pproxy UDP mode, not SOCKS5 UDP ASSOCIATE substitution.
- Add test names that actually exercise the translation.
- Add or update pproxy CLI tests if current tests only check generic translation.

If they are only partially implemented:

- Mark them `partial` or `supported`, not `intentional_non_parity`.
- State the missing pieces precisely.
- Keep parity matrix and README aligned.

If they are not implemented despite docs saying so:

- Downgrade parity docs and README immediately.
- Add TODO plan items for implementation.

Acceptance:

- Manifest, `docs/PARITY_MATRIX.md`, `docs/COMPATIBILITY_EVIDENCE.md`, README, and migration docs all agree on `-ul` / `-ur`.
- No document says Eggress uses SOCKS5 UDP ASSOCIATE instead of standalone pproxy UDP if standalone mode exists.

### 24.3 Add standalone UDP-specific manifest feature IDs

Add explicit feature IDs for standalone UDP behavior instead of reusing SOCKS5 UDP ASSOCIATE language.

Suggested feature IDs:

```toml
id = "standalone_udp_relay"
id = "standalone_udp_direct_echo"
id = "standalone_udp_domain_target"
id = "standalone_udp_multi_client"
id = "standalone_udp_multi_target"
id = "standalone_udp_malformed_datagram"
id = "standalone_udp_nonzero_frag"
id = "standalone_udp_route_reject"
id = "standalone_udp_flow_limits"
```

Do not over-fragment if the manifest is intended to stay high-level. At minimum, add `standalone_udp_relay` and `standalone_udp_error_handling`.

Evidence rules:

- If evidence is only Eggress runtime tests, use `supported` + `implemented_synthetic`.
- If evidence includes real pproxy differential tests for standalone `-ul`, use `compatible` + `compatible`.
- If evidence is behavioral but not byte-for-byte pproxy, use `supported` or `partial`.

Acceptance:

- Standalone UDP is represented separately from SOCKS5 UDP ASSOCIATE.
- Evidence commands point to standalone test names.

### 24.4 Rename or add standalone UDP differential tests

Audit `crates/eggress-cli/tests/differential_pproxy.rs` and identify whether standalone pproxy UDP tests exist.

If they exist but are poorly named:

- Rename tests so they contain `standalone_udp` or `pproxy_ul`.
- Update manifest and docs to reference those names.

If they do not exist:

- Add gated pproxy differential tests that launch real pproxy with `-ul` and compare Eggress standalone UDP behavior.
- Keep these tests narrow and deterministic.

Minimum differential cases:

- direct UDP echo through pproxy `-ul` versus Eggress standalone UDP;
- malformed short datagram behavior;
- nonzero FRAG behavior;
- two clients using the same listener;
- two targets from one client.

Acceptance:

- `docs/PARITY_MATRIX.md` standalone UDP row references standalone-specific differential tests or explicitly says runtime-only evidence.
- `docs/COMPATIBILITY_EVIDENCE.md` uses the same names.
- Manifest test-name validation passes.

### 24.5 Fix parity matrix contradictions

Update `docs/PARITY_MATRIX.md` after the manifest is corrected.

Specific expected changes:

- `Standalone UDP relay` should not cite `differential_socks5_udp_associate` unless that test is renamed or truly exercises standalone UDP.
- `-ul` and `-ur` CLI rows must agree with the manifest.
- `SOCKS5 UDP ASSOCIATE` should remain separate from `Standalone UDP relay`.
- `Retry within group` should not be marked `Compatible` without evidence unless manifest also says compatible; likely it should be `Supported` or `Intentional non-parity` depending on pproxy behavior.
- Any row with `Compatible` must map to a manifest row with `evidence_level = "compatible"`.

Acceptance:

- Manual scan of `PARITY_MATRIX.md` finds no compatibility claim that lacks manifest support.
- A future doc/manifest consistency test can be built from this structure.

### 24.6 Fix compatibility evidence doc generation claim

Currently `docs/COMPATIBILITY_EVIDENCE.md` says it is generated from the manifest. Make that true or reword it.

Preferred approach:

- Add a small generator in `eggress-testkit` or `scripts/` that emits `docs/COMPATIBILITY_EVIDENCE.md` from `tests/compat/pproxy_manifest.toml`.
- Add a CI/test check that generated output is up to date.

Alternative approach:

- Change the wording to: "Manually synchronized evidence summary derived from the manifest."
- Add a TODO to generate it later.

Acceptance:

- The doc does not falsely claim to be generated if it is not generated.
- If generated, CI fails when the checked-in output is stale.

### 24.7 Add CI status note

Create a short CI status document.

Suggested path:

```text
docs/CI_STATUS.md
```

Contents:

- workflow names;
- what each workflow validates;
- which workflows require external tools;
- whether they are expected to run on push/PR;
- known repository-hosting limitations, if any;
- how to run the equivalent checks locally;
- how to interpret skipped gated tests.

Do not claim hosted CI is green unless confirmed by GitHub status checks or workflow run pages.

Acceptance:

- A reviewer can distinguish configured CI from observed CI success.
- The completion doc links to this status note.

### 24.8 Tighten manifest validation for external dependency claims

Add validation around `external_dependency` if practical.

Rules:

- Any `compatible` entry with a test name containing `differential_` should have `external_dependency = "pproxy==2.7.9"`.
- Entries with `implemented_interop` should either name an external dependency or have a divergence note explaining the interop suite, such as Shadowsocks standard tooling.
- Entries with no `external_dependency` should be allowed only when synthetic or intentional non-parity.

Acceptance:

- Manifest entries better distinguish real pproxy differential evidence from standard interop evidence.
- The validator catches missing pproxy dependencies for pproxy differential claims.

### 24.9 Re-run validation and update completion doc

After changes, run:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test -p eggress-testkit manifest
cargo test -p eggress-runtime standalone
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

If external tools are available:

```bash
python3 -m pip install "pproxy==2.7.9"
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
cargo install shadowsocks-rust --features "local,server"
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1
```

Then add:

```text
docs/PHASE_24_EVIDENCE_CONSISTENCY_CLEANUP_COMPLETION.md
```

The completion doc should explicitly list:

- date-field resolution;
- `-ul` / `-ur` final classification;
- standalone UDP evidence classification;
- CI status note added;
- validator rules added;
- tests run;
- remaining gaps.

## Acceptance criteria

Phase 24 is complete when:

- `last_updated` is either removed or semantically correct.
- `udp_listen_flag` and `udp_remote_flag` no longer contradict parity docs.
- Standalone UDP has standalone-specific manifest entries and evidence references.
- `PARITY_MATRIX.md`, `COMPATIBILITY_EVIDENCE.md`, README, and the manifest agree on UDP status.
- `COMPATIBILITY_EVIDENCE.md` no longer falsely claims generation unless generation exists.
- `docs/CI_STATUS.md` exists and distinguishes configured CI from observed green runs.
- Manifest validation catches missing external dependency declarations for compatible differential claims.
- Workspace tests and manifest validation pass.

## Remaining gaps expected after this phase

This phase should leave the same real product gaps as before:

- UDP multi-hop chains.
- Trojan server/listener.
- SSH upstream transport.
- Transparent proxy/redir/PF and Unix sockets.
- HTTP/2, HTTP/3, QUIC, WebSocket, raw tunnel, reverse/backward proxying.
- System proxy configuration.
- True pproxy-shaped Python API drop-in replacement.
- Legacy Shadowsocks/SSR intentional non-parity unless the ADR changes.

## Handoff notes

This is a small but important paper-cut pass. The project should not proceed to the next feature-heavy pproxy parity phase until these contradictions are removed. The implementation has improved faster than the evidence metadata; this pass brings the metadata back into alignment.
