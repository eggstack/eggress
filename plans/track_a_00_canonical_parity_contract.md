# Track A.00: Canonical Parity Contract

## Objective

Create a single authoritative pproxy parity contract and make every compatibility claim in the repository derive from it or validate against it. This is the first Track A task because implementation status is currently spread across README sections, `docs/parity/pproxy_capability_manifest.toml`, `tests/compat/pproxy_manifest.toml`, compatibility evidence docs, CLI diagnostics, and Python feature introspection. Those surfaces must stop drifting.

## Problem Statement

The current compatibility story is too optimistic in some places and too stale in others. A release reader can see claims of final parity certification while older evidence files still mark important areas partial, synthetic-only, unsupported, or intentional non-parity. The result is not just documentation debt; it directly affects the CLI check path and Python compatibility surface.

This plan establishes a manifest-first workflow. All public claims must be generated from or checked against one manifest. The manifest should be treated as an executable compatibility contract.

## Canonical File

Use `docs/parity/pproxy_capability_manifest.toml` as the canonical source unless there is a compelling reason to move it. The older `tests/compat/pproxy_manifest.toml` should either be deleted after migration or generated from the canonical manifest into a test-friendly projection.

Do not maintain two independently edited manifests.

## Required Manifest Schema

Each capability entry should include at least:

- `id`: stable machine-readable capability ID.
- `category`: one of `cli`, `uri`, `protocol`, `routing`, `udp`, `tls`, `python`, `packaging`, `system_proxy`, `security`, `docs`.
- `pproxy_surface`: exact pproxy flag, URI scheme, API symbol, or behavior.
- `pproxy_behavior`: concise statement of observed/expected pproxy behavior.
- `egress_behavior`: concise statement of egress behavior.
- `tier`: compatibility tier.
- `parser`: parser status.
- `translator`: translator status.
- `config`: config/compiler status.
- `runtime`: runtime status.
- `cli`: CLI status.
- `python`: Python status.
- `docs`: documentation status.
- `evidence`: strongest evidence class.
- `tests`: supporting test IDs.
- `notes`: known divergence.
- `rationale`: required for intentional non-parity.
- `caveat_class`: optional grouping for release notes.

## Tier Vocabulary

Normalize all code and docs to the following vocabulary:

- `drop_in`: same pproxy invocation/API is expected to work with materially equivalent behavior.
- `compatible_with_warning`: same invocation/API is accepted but egress reports a behavior difference.
- `native_equivalent`: egress has an equivalent capability but not the same pproxy shape.
- `feature_gated_legacy`: implemented only behind explicit compatibility/legacy features.
- `intentional_non_parity`: deliberately not replicated; rationale required.
- `unsupported`: not implemented.

Avoid using older labels such as `supported`, `partial`, or `compatible` in generated public docs unless they are clearly mapped from the canonical tiers.

## Evidence Vocabulary

Normalize evidence values to:

- `differential`: tested against real pproxy.
- `interop`: tested against a non-pproxy standard implementation.
- `integration`: tested through egress runtime/CLI but not against pproxy.
- `unit`: local unit-level evidence.
- `synthetic`: synthetic protocol harness only.
- `docs_only`: documented but not executable.
- `none`: no evidence yet.

A capability cannot be `drop_in` unless evidence is `differential`, or the manifest includes a `differential_exemption` field explaining why differential evidence is impossible.

## Implementation Tasks

1. Add a manifest schema model in a test/helper module or xtask module.
2. Parse `docs/parity/pproxy_capability_manifest.toml` in tests.
3. Validate every capability has required fields and valid vocabulary.
4. Add checks that `intentional_non_parity` entries include rationale and caveat class.
5. Add checks that `drop_in` entries have adequate evidence.
6. Replace or regenerate `tests/compat/pproxy_manifest.toml` from the canonical manifest.
7. Update `docs/COMPATIBILITY_EVIDENCE.md` so it is generated or explicitly marked as generated from the canonical manifest.
8. Add a generated `docs/parity/PARITY_REPORT.md` path, even if initial generation is manual through a new command.
9. Update README status language to reference the generated report rather than duplicating detailed matrix claims.
10. Update Python `supported_features()` to read from generated/static manifest data, or rename it to distinguish parseable features from runtime-supported features.
11. Update `eggress pproxy check --json` so every status and diagnostic tier is manifest-backed.

## Consistency Checks

Add tests that fail when:

- README claims a feature is complete while manifest says runtime/config/CLI is refused or unsupported.
- Python feature introspection lists a protocol as supported when manifest runtime is refused.
- `pproxy check` emits a tier not in the canonical vocabulary.
- a capability has contradictory per-layer statuses.
- docs refer to `tests/compat/pproxy_manifest.toml` as source of truth after migration.

## Known Conflicts To Resolve Immediately

- Trojan server: README and manifest/evidence need to agree whether it is runtime-supported.
- WebSocket/raw/H2: protocol-crate-only features must not appear as runtime-supported features.
- System proxy: distinguish inspect, dry-run, apply, rollback, and pproxy `--sys` mutation semantics.
- `--ssl`, `--block`, `--rulefile`, `--log`: older manifest statuses must be reconciled with newer translator behavior.
- Python system proxy support: if unimplemented, do not list as supported.

## Acceptance Criteria

- One manifest is explicitly documented as authoritative.
- Old manifests are deleted, generated, or clearly marked non-authoritative.
- `cargo test parity_manifest_consistency` or equivalent exists.
- README, compatibility evidence docs, CLI check output, and Python feature reporting agree with the manifest.
- The repository no longer makes unqualified "full parity" claims without generated certification evidence.

## Non-goals

This task does not implement missing protocols. It only establishes the contract and prevents inaccurate claims from entering docs or tools.

## Handoff Notes

Start with the manifest/parser/test work before editing README copy. Once tests can identify contradictions, use them to drive doc and code corrections. Avoid broad manual rewrites that will drift again.
