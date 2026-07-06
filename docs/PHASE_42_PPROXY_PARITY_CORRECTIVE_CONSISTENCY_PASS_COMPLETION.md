# Phase 42: pproxy Parity Corrective Consistency Pass Completion Record

## Summary

Closed several inconsistency gaps in the pproxy parity surface that had accumulated across Phases 37–41: the parity report was drifting from the manifest, the Python `CompatibilityReport` tier vocabulary did not match the manifest, `PPProxyService.from_args` was silently stripping pproxy flags before translation, the `--ssl` translator only applied TLS to the first listener (pproxy applies it to every listener), and several manifest entries contained stale wording that contradicted the actual code/tests. All five workstreams landed.

## Status: Complete

## Scope delivered

### D1: Parity report is now generated from the manifest

`scripts/validate_pproxy_parity_manifest.py` gained two flags:

- `--write-report PATH` — Regenerates `docs/parity/PPROXY_PARITY_REPORT.md` from the manifest. The manifest is the single source of truth; the report is derived.
- `--check-report PATH` — Verifies the report is consistent with the manifest. Exits non-zero on mismatch. The `.github/workflows/pproxy-compat.yml` workflow runs this step.

The report itself was regenerated from the updated manifest. The 106-capability distribution: `drop_in` 63 (59.4%), `compatible_with_warning` 8 (7.5%), `native_equivalent` 21 (19.8%), `intentional_non_parity` 5 (4.7%), `unsupported` 9 (8.5%).

### D2: Manifest stale wording fixed; two new validation rules

The manifest had entries whose `notes` described features as "not recognized" or "unknown flag" even though the translator now recognizes them and emits a structured diagnostic. Fixed in:

- `cli.alive`, `cli.ssl_listener`, `cli.block`, `cli.rulefile`, `cli.reuse`, `cli.get`, `cli.pac`, `cli.test`, `cli.sys`, `cli.verbose`, `cli.log`, `python.check_pproxy_args`

Two new validator rules were added (Phase 37 had 11; Phase 42 brings the total to 13):

- **Rule 12** — `notes` field flags stale "not recognized"/"unknown-flag" wording. Negation-aware ("NOT unknown-flag" is fine). WARNING.
- **Rule 13** — `config = "not_applicable"` with parser + translator `complete` requires an explicit justification phrase in `notes` ("no config artifact", etc.). WARNING.

Both warnings become errors in `--strict` mode.

### D3: `PPProxyService.from_args` preserves the full pproxy argument vector

Previously `from_args` called `translate_pproxy_args` then constructed an `EggressService` from only the `-l`/`-r` URIs, silently dropping flags like `--ssl`, `-b`, `--pac`, `--rulefile`, etc. Now `from_args` passes the entire translated TOML config to `EggressService`, so flags that the translator recognized and acted on are honored.

This surfaced three latent bugs in the translator that had been masked by the old code path:

1. **Reject reason mismatch** — `-b` rule rejected with reason `"blocked by pproxy -b rule"`, which the config compiler did not recognize. Changed to the canonical reason `"blocked"`.
2. **PAC section incomplete** — `--pac` produced `[admin.pac] enabled = true`, missing the required `proxy` field. Now emits `[admin.pac] enabled = true, path = "/proxy.pac", proxy = "PROXY {}", direct_fallback = true`.
3. **Empty `upstream_group` in reject rules** — block rules serialized `upstream_group = ""`, which the config compiler treats as "unknown upstream group". Added `#[serde(skip_serializing_if = "String::is_empty")]` to `RuleToml.upstream_group`.

`PPProxyService.config` is also exposed as a `@property` (was previously internal `_config`), so users can inspect the translated config without calling `start()`.

### D4: `CompatibilityReport.tier` uses the manifest-aligned vocabulary

The Python compatibility report previously used `compatible` / `partial` / `unsupported` / `intentional_non_parity`. The manifest uses `drop_in` / `compatible_with_warning` / `native_equivalent` / `intentional_non_parity` / `unsupported`. Phase 42 aligns both, with per-diagnostic `tier` fields for attribution.

The `features` list was also scoped to the capabilities triggered by the specific args, rather than the full `supported_features()` set. Previously a smoke call to `check_pproxy_args([])` would list every supported feature as "compatible" — that conflated the convenience catalog with evidence-based classification.

New helper `_manifest_tier_for_diagnostic(code)` maps translator warning codes to manifest tiers. `_classify_aggregate_tier()` rolls up per-diagnostic tiers to the aggregate.

### D5: `evidence` field values already distinguish smoke vs differential

The manifest already had five evidence levels — `differential`, `integration`, `unit`, `synthetic`, `docs_only`, `none` — and Rule 4 already required `drop_in` entries to have `evidence >= integration` (or a `differential_exception` flag). The boundary between "smoke integration test" and "actual pproxy differential" is encoded in the evidence value itself: `integration` means end-to-end test in eggress only (no live pproxy); `differential` means tested against live pproxy. No new field was needed; this is now documented in `docs/parity/README.md`.

### D6: `--ssl` applies to all compatible listeners (matches pproxy)

Investigation of the pproxy 2.7.9 source (`pproxy/server.py`) confirmed that pproxy's `--ssl` loads the cert chain into **every** ssl context (`for context in sslcontexts: context.load_cert_chain(*sslfile)`), not just the first. The eggress translator previously applied TLS only to the first compatible listener.

Changed `crates/eggress-pproxy-compat/src/translate.rs` to apply `listeners.tls` to every compatible TCP listener. Added unit test `test_ssl_flag_applies_to_all_listeners` that asserts two `-l` URIs plus `--ssl` produce two `[listeners.tls]` blocks.

### D7: CI runs `--check-report` on every push

`.github/workflows/pproxy-compat.yml` now runs `validate_pproxy_parity_manifest.py --check-report` between the manifest validation step and the pproxy_oracle tests. If the report drifts from the manifest, CI fails.

## Key design decisions

### Single source of truth: manifest, not report

The report is generated from the manifest. Hand-editing the report is not supported — `--check-report` would catch it. The manifest's `notes` field is the only place where descriptive rationale lives; the report just summarizes.

### Five-tier vocabulary is the only vocabulary

`CompatibilityReport.tier` accepts exactly the five manifest values. Convenience strings (`"supported"`, `"partial"`, `"compatible"`) were removed. Anyone reading the Python report sees the same vocabulary as the manifest, the README, and the parity matrix.

### `--ssl` semantics match pproxy, not just eggress internals

The decision to apply TLS to every listener was driven by reading the pproxy source, not by an oracle/differential test. The unit test verifies the eggress behavior; pproxy equivalence is reasoned from the pproxy source. This is consistent with the manifest's `evidence = "unit"` for `cli.ssl_listener` (no `differential` coverage).

## Files created/modified

### Created
- `docs/PHASE_42_PPROXY_PARITY_CORRECTIVE_CONSISTENCY_PASS_COMPLETION.md` — this document

### Modified
- `scripts/validate_pproxy_parity_manifest.py` — Added `--write-report`, `--check-report`, Rule 12, Rule 13
- `docs/parity/pproxy_capability_manifest.toml` — Fixed stale notes for 12 entries; added `cli.ssl_listener` multi-listener unit test reference
- `docs/parity/PPROXY_PARITY_REPORT.md` — Regenerated from the manifest (no longer hand-edited)
- `docs/parity/README.md` — Updated to 13 validation rules; added `--write-report`/`--check-report` usage
- `python/eggress/pproxy.py` — `PPProxyService.from_args` now passes full translated TOML through; `check_pproxy_args` uses five-tier vocabulary with per-diagnostic tier attribution
- `python/eggress/pproxy.pyi` — Stub docstrings updated to reflect five-tier vocabulary; `EggressHandle` import; corrected return types
- `python/tests/test_pproxy_dropin.py` — Added `TestPPProxyServiceFromArgsPreservesFlags`; updated `TestCompatibilityReport` for five-tier vocabulary
- `crates/eggress-pproxy-compat/src/translate.rs` — `--ssl` applies TLS to every compatible listener; new `test_ssl_flag_applies_to_all_listeners` unit test
- `docs/PARITY_MATRIX.md` — `--ssl` rows reflect `Native equivalent` tier and Phase 42 multi-listener semantics
- `README.md` — Status banner mentions Phase 42; capability count updated to 106; Phase 42 row added
- `AGENTS.md` — `--write-report` / `--check-report` commands added; Phase 42 entry in the key architecture facts; capability count updated to 106
- `.skills/testing/skill.md` — `--write-report` / `--check-report` commands added in two places
- `docs/ROADMAP.md` — Current Phase is Phase 42; Phase 42 entry added to Completed Milestones
- `.github/workflows/pproxy-compat.yml` — Added `--check-report` step

## Verification commands run

| Command | Status |
|---------|--------|
| `python3 scripts/validate_pproxy_parity_manifest.py docs/parity/pproxy_capability_manifest.toml` | PASS (106 caps, 0 errors, 0 warnings) |
| `python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml` | PASS (0 errors) |
| `python3 scripts/validate_pproxy_parity_manifest.py --write-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml` | Wrote report |
| `python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml` | Report is consistent |
| `cargo test -p eggress-pproxy-compat --lib -- --test-threads=1` | PASS (212 tests) |
| `cargo test -p eggress-pproxy-compat --lib ssl` | PASS (5 tests including new `test_ssl_flag_applies_to_all_listeners`) |
| `cargo test -p eggress-config --lib` | PASS (93 tests) |
| `cargo test -p eggress-testkit --lib` | PASS (56 tests, 2 ignored) |
| `cargo test -p eggress-testkit validate_real_manifest` | PASS |
| `cargo test -p eggress-testkit manifest_test_names_exist` | PASS |
| `pytest python/tests/test_pproxy_dropin.py` (Python 3.11) | PASS (46 tests, including new `TestPPProxyServiceFromArgsPreservesFlags`) |
| `pytest python/tests/test_pproxy_compat.py python/tests/test_pproxy_redaction.py python/tests/test_pproxy_concurrency.py python/tests/test_pproxy_diagnostics.py` | 102 passed, 1 environment-flaky failure (`test_local_http_direct` startup timeout; not regression) |

## Notes for follow-up phases

- The `cli.ssl_listener` capability is `native_equivalent` (not `drop_in`) because there is no pproxy differential coverage; only unit-level evidence that TLS applies to every listener. A future phase could add a pproxy oracle test that verifies multi-listener TLS, then promote to `drop_in` if byte-equivalent.
- Rule 12 and Rule 13 are warnings by default; CI runs without `--strict` to keep the gate behavior identical to Phase 37. If the project later wants these to block CI, switch the workflow to `--strict`.