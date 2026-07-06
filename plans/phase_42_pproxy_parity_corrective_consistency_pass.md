# Phase 42: pproxy parity corrective consistency pass

## Goal

Correct the consistency and evidence-quality issues introduced while executing Phases 37-41. The repo now has useful parity infrastructure, but the compatibility story is split across implementation, manifest, generated/claimed reports, Python wrappers, CLI inventory, and completion records. This phase should make those surfaces agree before any new feature-parity phase proceeds.

This is a corrective pass, not a protocol-expansion pass. The target is to make the current pproxy/Python drop-in claims precise, mechanically checkable, and not stronger than the actual code and tests.

## Current state

Recent work landed substantial improvements:

- Phase 37 added `docs/parity/pproxy_capability_manifest.toml`, `docs/parity/PPROXY_PARITY_REPORT.md`, `docs/parity/README.md`, and `scripts/validate_pproxy_parity_manifest.py`.
- Phase 38 added native-equivalent handling for several pproxy CLI flags, including `--ssl`, `-b`, `--rulefile`, `--pac`, `--test`, `--sys`, `--log`, `--reuse`, and `--get` diagnostics.
- Phase 39 added `PproxyChain`, `parse_pproxy_chain()`, `__` chain parsing, chain validation, and `pproxy check --json` chain metadata.
- Phase 40 added Python `PPProxyService`, `CompatibilityReport`, `FeatureInfo`, `start_pproxy(...)`, `.pyi` stubs, and Python drop-in tests.
- Phase 41 added reusable differential harness pieces and gated pproxy comparison suites.

The repo is much closer to a serious parity candidate, but several compatibility claims are now inconsistent or over-broad.

## Problems to correct

### C1: Parity report is stale relative to the manifest and code

`docs/parity/PPROXY_PARITY_REPORT.md` still lists some Phase 38-improved capabilities as unsupported or not recognized. Examples:

- `cli.ssl_listener` is listed as unsupported in the report, while the manifest now says it translates `cert[,key]` into listener TLS config.
- `cli.block` is listed as unsupported in the report, while the manifest now says it translates to reject rules.
- `cli.rulefile` is listed as unsupported in the report, while the manifest now says simple reject/block rules are compatible-with-warning.
- `cli.pac` report text says the flag is not recognized, while the parser recognizes `--pac` and translation emits admin PAC TOML.

This undermines the report's claim that it is generated from the manifest.

### C2: Manifest notes contain stale or contradictory wording

Some manifest entries have accurate machine fields but stale human notes. Examples:

- `cli.reuse` says the flag is parsed but emits an unknown-flag warning, even though it is now in the known raw flag set and emits a structured `reuse-connection` warning.
- `cli.get` says it is not recognized, even though `--get` is parsed and emits a structured warning suggesting `curl --proxy`.
- Some entries say `native_equivalent` while evidence is only `unit`, which may be okay but should be explicitly described as translation/config evidence, not runtime equivalence.

### C3: Python `PPProxyService.from_args()` drops non-listen/remote flags

`PPProxyService.from_args(args)` currently manually extracts only `-l/--listen` and `-r/--remote`, then reconstructs the service from those lists. This silently drops pproxy flags such as:

- `--ssl`
- `-b`
- `--rulefile`
- `-ul`
- `-ur`
- `-s`
- `-a`
- `--pac`
- `--test`
- `--sys`
- `--log`
- `--reuse`
- `--get`

By contrast, top-level `start_pproxy(args=...)` uses `EggressService.from_pproxy_args(args)`, which preserves the full argument vector. The two Python entry points should not diverge.

### C4: Python compatibility report tier language is disconnected from the manifest

`CompatibilityReport.tier` currently uses `full`, `partial`, and `unsupported`. The manifest uses `drop_in`, `compatible_with_warning`, `native_equivalent`, `intentional_non_parity`, and `unsupported`. The Python docs claim alignment with `eggress pproxy check --json` and the Phase 37 manifest, but the field values do not align.

The current Python `features` list is also derived from `supported_features()` rather than the manifest or CLI JSON output, so it is a synthetic summary rather than a manifest-backed compatibility report.

### C5: Differential evidence is mixed with eggress-only smoke evidence

The Phase 41 completion record correctly notes that some scenarios are eggress-only because pproxy is broken or unavailable on macOS, especially SOCKS5 UDP ASSOCIATE and TLS listener behavior. These are useful smoke/integration tests but should not be counted as differential pproxy evidence.

The manifest and completion docs should clearly distinguish:

- real side-by-side pproxy differential evidence;
- pproxy oracle/black-box evidence;
- eggress-only smoke tests;
- unit/config translation evidence.

### C6: `--ssl` listener scope needs pproxy comparison

Current translation applies `--ssl` TLS config only to the first generated listener. If pproxy applies `--ssl` to all listeners, then eggress is not drop-in for multi-listener SSL commands. This needs a focused oracle check and either an implementation fix or an explicit compatibility-with-warning classification.

### C7: Hosted CI status is unclear

Completion records say verification passed locally, but repository workflow run lookup did not show runs for the latest commits. This may be a connector limitation, but the repo should still make verification status clear in docs and CI configuration.

## Deliverables

### D1: Repair parity report generation or regeneration

Preferred path: make `docs/parity/PPROXY_PARITY_REPORT.md` generated from `docs/parity/pproxy_capability_manifest.toml` by extending `scripts/validate_pproxy_parity_manifest.py` with a report generation mode.

Acceptable path: manually update the report and add a validator check that report counts and key capability IDs match the manifest.

Required outcomes:

- Report tier counts match the manifest exactly.
- Report status for `cli.ssl_listener`, `cli.block`, `cli.rulefile`, `cli.pac`, `cli.get`, `cli.reuse`, `cli.test`, and `cli.sys` matches the manifest.
- Report no longer says `--pac` is not recognized.
- Report no longer lists Phase 38-improved capabilities as unsupported unless the manifest also does.
- Report explicitly labels protocol-crate-only features as not runtime drop-in.

Suggested command shape:

```bash
python3 scripts/validate_pproxy_parity_manifest.py docs/parity/pproxy_capability_manifest.toml --write-report docs/parity/PPROXY_PARITY_REPORT.md
```

If adding write mode is too large, add:

```bash
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/pproxy_capability_manifest.toml docs/parity/PPROXY_PARITY_REPORT.md
```

### D2: Fix stale manifest notes and evidence language

Review the full manifest for contradictions between tier/layer fields and notes. At minimum fix:

- `cli.reuse` note: remove unknown-flag wording; state parsed as known flag and emits intentional-non-parity diagnostic.
- `cli.get` behavior: remove not-recognized wording; state parsed as known flag and emits diagnostic recommending `curl --proxy`.
- `cli.pac`: ensure parser/translator/CLI layer fields reflect that the flag is recognized and `admin.pac` TOML is emitted.
- `cli.test`: clarify whether it is native-equivalent or compatible, depending on actual pproxy run behavior.
- `cli.sys`: clarify that inspection is performed and mutation/apply semantics differ, if applicable.
- `cli.ssl_listener`: clarify evidence as translation/config/runtime smoke, not pproxy differential unless a true differential test exists.
- `cli.block` and `cli.rulefile`: clarify rule grammar limits and evidence scope.

Add validator warnings for entries where `notes` or `eggress_behavior` contain stale phrases such as:

- `not recognized` when `parser = "complete"` and `cli = "complete"`;
- `unknown-flag` for a known raw flag;
- `Generated by` report header if report is not actually generated.

### D3: Fix `PPProxyService.from_args()` argument preservation

Change `PPProxyService.from_args(args, allow_partial=False)` so it preserves the full pproxy argument vector.

Preferred implementation:

```python
@classmethod
def from_args(cls, args: Sequence[str], allow_partial: bool = False) -> PPProxyService:
    from eggress.config import EggressConfig
    result = translate_pproxy_args(list(args))
    if not allow_partial and not result.ok:
        ... raise UnsupportedFeatureError ...
    return cls(config=result.config())
```

Do not manually parse only `-l` and `-r` inside Python. The Rust compat parser is the source of truth for pproxy CLI syntax.

Add Python tests proving flags survive through this path:

- `PPProxyService.from_args(["-l", "socks5://127.0.0.1:0", "--ssl", "cert.pem,key.pem"])` generates/uses TLS config or at least preserves it in translated TOML before startup.
- `PPProxyService.from_args(["-l", "socks5://127.0.0.1:0", "-b", ".*example.*"])` includes reject rule.
- `PPProxyService.from_args(["-l", "socks5://127.0.0.1:0", "--pac"])` includes admin PAC config.
- `PPProxyService.from_args(["-l", "socks5://127.0.0.1:0", "-s", "rr"])` preserves scheduler behavior when a remote exists.
- `PPProxyService.from_args(["-ul", ":0"])` does not fail merely because no `-l` was manually extracted.

Also check the `Server` class: it is URI-list based, so it may reasonably only accept listen/remote. Do not force full CLI flag support into `Server` unless documented.

### D4: Align Python compatibility report tier semantics

Choose one of two paths.

#### Option A: Align Python tiers with manifest tiers

Change `CompatibilityReport.tier` to emit one of:

- `drop_in`
- `compatible_with_warning`
- `native_equivalent`
- `intentional_non_parity`
- `unsupported`

For aggregate reports, use deterministic severity order:

1. any unsupported hard failure -> `unsupported`
2. any intentional non-parity -> `intentional_non_parity`
3. any native-equivalent warning -> `native_equivalent`
4. any warning -> `compatible_with_warning`
5. no diagnostics -> `drop_in`

This is closest to the manifest language.

#### Option B: Rename the Python field and document it as Python-local

Keep `full` / `partial` / `unsupported`, but rename to `status` or add `status` and deprecate `tier`. Then remove claims that it aligns exactly with the manifest.

Preferred path: Option A.

Additional requirements:

- `FeatureInfo.tier` should not mark every feature from `supported_features()` as `compatible` by default.
- If manifest-backed feature info is not available in Python, mark `features` as a convenience capability list rather than manifest evidence.
- If feasible, expose a generated compact manifest JSON or static table to Python so reports can classify known unsupported feature IDs correctly.

Tests:

- no-warning simple config -> `drop_in` or documented equivalent;
- warning-only config -> `compatible_with_warning` or `native_equivalent` as appropriate;
- SSH/SSR -> `unsupported` or `intentional_non_parity` as appropriate;
- `--reuse` -> `intentional_non_parity`, not generic unsupported;
- `--get` -> documented status.

### D5: Separate differential evidence from smoke evidence

Update these files as needed:

- `docs/PHASE_41_DIFFERENTIAL_PARITY_HARNESS_COMPLETION.md`
- `docs/DIFFERENTIAL_TESTING.md`
- `docs/parity/pproxy_capability_manifest.toml`
- `docs/parity/PPROXY_PARITY_REPORT.md`
- `tests/compat/pproxy_manifest.toml`
- `docs/PARITY_MATRIX.md`

Required classification:

- True side-by-side pproxy tests may use `evidence = "differential"`.
- Eggress-only TLS listener smoke tests should use `integration` or `smoke`, not `differential`. If the manifest schema lacks `smoke`, use `integration` and explain in notes.
- Eggress-only UDP tests due to pproxy macOS limitations must not be described as pproxy differential evidence.
- pproxy oracle behavior probes should be separately labeled from differential tests when they do not compare eggress side-by-side.

Add a small section to the Phase 41 completion doc titled `Evidence classification` with a table:

| Scenario | Evidence kind | Reason |
|----------|---------------|--------|
| HTTP CONNECT | differential | side-by-side pproxy/eggress |
| SOCKS5 UDP ASSOCIATE on macOS | integration | pproxy UDP unavailable/broken |
| TLS listener | integration unless pproxy side-by-side added | eggress-only smoke |

### D6: Determine and correct `--ssl` multi-listener semantics

Add a small pproxy oracle/differential test for multi-listener `--ssl` behavior.

Questions to answer:

- Does pproxy apply `--ssl` to all local listeners or only the first?
- Does it require one cert/key pair globally or per listener?
- Does it affect HTTP, SOCKS, and Shadowsocks listeners uniformly?

If pproxy applies TLS globally to all listeners, change translation to apply `ssl_config.clone()` to every generated compatible TCP listener rather than only the first. If pproxy applies it only to the first, document this and add a regression test proving eggress matches.

Implementation note:

Current code applies TLS here:

```rust
if let Some(tls) = ssl_config {
    if let Some(listener) = listeners.first_mut() {
        listener.tls = Some(tls);
    }
}
```

If applying to all listeners, use:

```rust
if let Some(tls) = ssl_config {
    for listener in &mut listeners {
        listener.tls = Some(tls.clone());
    }
}
```

Only do this after validating pproxy semantics.

Tests:

- two listeners plus `--ssl` translation includes expected TLS config on each or only first according to pproxy behavior;
- generated TOML compiles;
- at least one TLS runtime smoke test remains;
- manifest/report tier reflects evidence.

### D7: Improve CI/status verification hygiene

Inspect `.github/workflows/pproxy-compat.yml` and related workflows.

Ensure at minimum:

- manifest validator runs in normal CI;
- report consistency check runs in normal CI;
- Python drop-in tests run in at least one CI path if Python packaging is available;
- gated pproxy differential tests remain opt-in or scheduled, but docs say exactly when they are expected to run;
- completion records do not imply hosted CI passed when only local verification was performed.

If hosted CI is intentionally unavailable for some checks, add explicit wording:

> Local verification passed; hosted CI status unavailable/not configured for this gated external-dependency suite.

## Implementation order

1. Fix `PPProxyService.from_args()` first because it is a real user-facing bug.
2. Add/adjust Python tests for full arg preservation and tier/status semantics.
3. Fix manifest stale entries and add validator warnings for stale phrases.
4. Regenerate or repair `PPROXY_PARITY_REPORT.md` from the corrected manifest.
5. Correct evidence classifications in Phase 41 docs and manifests.
6. Investigate `--ssl` multi-listener semantics and patch translator/tests if needed.
7. Update CI/docs verification wording.
8. Run full verification.

## Files likely to change

- `python/eggress/pproxy.py`
- `python/eggress/pproxy.pyi`
- `python/eggress/__init__.pyi`
- `python/tests/test_pproxy_dropin.py`
- possibly `python/tests/test_pproxy_compat.py`
- `crates/eggress-pproxy-compat/src/translate.rs`
- `crates/eggress-pproxy-compat/src/tests.rs`
- `crates/eggress-cli/tests/pproxy_cli.rs`
- `crates/eggress-cli/tests/pproxy_differential.rs`
- `scripts/validate_pproxy_parity_manifest.py`
- `docs/parity/pproxy_capability_manifest.toml`
- `docs/parity/PPROXY_PARITY_REPORT.md`
- `docs/parity/README.md`
- `docs/PHASE_41_DIFFERENTIAL_PARITY_HARNESS_COMPLETION.md`
- `docs/PHASE_40_PYTHON_PPROXY_DROPIN_API_COMPLETION.md`
- `docs/DIFFERENTIAL_TESTING.md`
- `docs/PARITY_MATRIX.md`
- `docs/cli/PPROXY_CLI_INVENTORY.md`
- `.github/workflows/pproxy-compat.yml`
- `AGENTS.md`

## Acceptance criteria

### Python API correctness

- `PPProxyService.from_args()` preserves the complete pproxy argument vector.
- `PPProxyService.from_args()` and `start_pproxy(args=...)` produce equivalent configs for the same args.
- Python tests cover `--ssl`, `-b`, `--pac`, scheduler, and `-ul`/`-ur` or standalone UDP where feasible.
- Python compatibility status/tier semantics are documented and tested.

### Manifest/report consistency

- `docs/parity/PPROXY_PARITY_REPORT.md` agrees with the manifest counts and key capability tiers.
- No report entry says a known Phase 38 flag is unsupported/not recognized unless the manifest also says so.
- Validator catches stale report counts or stale report entries.
- Validator catches or warns on stale manifest phrases such as `not recognized` for known parsed flags.

### Evidence hygiene

- True pproxy differential tests are clearly separated from eggress-only smoke/integration tests.
- Manifest evidence levels for UDP/TLS scenarios do not overclaim side-by-side pproxy evidence where only eggress ran.
- Completion records state local vs hosted verification accurately.

### `--ssl` behavior

- Multi-listener `--ssl` behavior is verified against pproxy or explicitly documented as unknown with conservative classification.
- Translator behavior matches verified pproxy semantics or is downgraded to compatible-with-warning/native-equivalent with a clear diagnostic.

### CI/docs

- Standard CI validates the parity manifest and report consistency.
- Gated external pproxy differential tests are documented with exact env vars and install prerequisites.
- README, PARITY_MATRIX, CLI inventory, and parity report no longer contradict one another on Phase 38-41 capabilities.

## Verification commands

Run at minimum:

```bash
cargo fmt --all -- --check
cargo test -p eggress-pproxy-compat
cargo test -p eggress-cli --test pproxy_cli
cargo test -p eggress-cli --test pproxy_differential -- --ignored
cargo test --workspace
python3 scripts/validate_pproxy_parity_manifest.py docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python -m pytest python/tests/test_pproxy_dropin.py -v
python -m pytest python/tests -v
```

For gated pproxy side-by-side coverage, run when pproxy 2.7.9 is installed:

```bash
python3.11 -m pip install "pproxy==2.7.9"
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
EGRESS_RUN_PPROXY_DIFFERENTIAL=1 cargo test -p eggress-cli --test pproxy_differential -- --ignored --test-threads=1
```

If some gated tests are intentionally skipped on macOS, document the exact skip reason in test output and docs.

## Non-goals

- Do not implement SSH upstream support in this phase.
- Do not implement Trojan server/fallback in this phase.
- Do not promote WebSocket/raw/H2 protocol-crate-only features into runtime support in this phase.
- Do not add SSR.
- Do not add QUIC/H3.
- Do not broaden Python API semantics beyond correcting current drop-in claims.

## Handoff notes

Be conservative with compatibility labels. The repo is now feature-rich enough that the main risk is overclaiming, not lack of functionality. Prefer `native_equivalent` or `compatible_with_warning` over `drop_in` unless there is actual runtime and differential evidence.
