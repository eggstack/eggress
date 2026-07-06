# Phase 51: final parity certification and release go/no-go

## Goal

Run the final certification pass for the pproxy parity and Python drop-in release. This phase should not add major new features. It should prove the release claim, freeze the manifest/report, verify install paths, and produce an honest go/no-go result.

## Release claim to certify

The release should state exactly what is true. Suggested wording if prior phases complete without SSH/QUIC/SSR:

> eggress is a Rust-native, embeddable pproxy-compatible proxy framework covering mainline HTTP, SOCKS4/4a, SOCKS5, Shadowsocks, routing, standalone UDP, Python embedding, and pproxy-style CLI translation. Legacy/nonstandard/deferred surfaces such as SSR, SSH upstream, QUIC/H3, SOCKS BIND, and selected protocol-crate-only transports are explicitly classified in the generated parity report.

Do not call the release full strict pproxy parity unless the manifest supports that claim.

## Inputs required before this phase

- Phase 43 report-generator polish complete.
- Trojan decision/implementation complete.
- SOCKS BIND/UDP edge semantics complete or classified.
- WS/raw/H2 promote/demote decisions complete.
- SSH decision complete.
- QUIC/H3 decision complete.
- Packaging/wheel artifacts complete.
- Security/robustness release gate complete.

## Workstream A: freeze parity manifest

- Run manifest validation in strict mode.
- Generate `docs/parity/PPROXY_PARITY_REPORT.md` from the manifest.
- Run report consistency check.
- Confirm every `drop_in` entry has appropriate evidence.
- Confirm every `unsupported` or `intentional_non_parity` entry has rationale and diagnostics.
- Confirm protocol-crate-only/deferred/missing-role caveats are categorized correctly.

Create a release snapshot file:

- `docs/release/FINAL_PPROXY_PARITY_CERTIFICATION.md`

Include:

- pproxy version;
- eggress commit SHA;
- manifest capability counts;
- drop-in list;
- warnings/native equivalents;
- unsupported/intentional non-parity list;
- known platform limitations;
- test commands and results.

## Workstream B: run differential and interop suites

Run all relevant tests against pinned pproxy:

```bash
python3.11 -m pip install "pproxy==2.7.9"
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
EGRESS_RUN_PPROXY_DIFFERENTIAL=1 cargo test -p eggress-cli --test pproxy_differential -- --ignored --test-threads=1
cargo test -p eggress-testkit pproxy_oracle -- --ignored --test-threads=1
```

Record skipped tests and reasons. Do not count skipped or eggress-only smoke tests as pproxy differential evidence.

## Workstream C: package/install verification

For each release artifact:

- install from clean environment;
- run `eggress --version`;
- run `eggress pproxy check --json -- -l socks5://127.0.0.1:0`;
- run a minimal runtime smoke test;
- for Python wheel: `import eggress`, `check_pproxy_args`, `PPProxyService.from_args`, `start_pproxy(local="socks5://127.0.0.1:0")`.

Record platform and architecture results in `docs/release/ARTIFACT_VERIFICATION.md`.

## Workstream D: docs/release consistency

Audit these docs for contradictory claims:

- `README.md`
- `docs/PARITY_MATRIX.md`
- `docs/parity/PPROXY_PARITY_REPORT.md`
- `docs/cli/PPROXY_CLI_INVENTORY.md`
- `docs/PYTHON_BINDINGS.md`
- `docs/release/*`
- `AGENTS.md`
- `.skills/testing/skill.md`

Search for stale phrases:

- `final parity` if not actually final;
- `full parity` if unsupported entries remain;
- `unsupported` for features now native-equivalent;
- `not recognized` for known raw flags;
- `protocol-crate-only` for missing/deferred features.

## Workstream E: release notes

Create release notes with:

- what works;
- what is intentionally different from pproxy;
- what is unsupported;
- Python embedding examples;
- migration examples;
- security defaults and differences;
- install commands;
- artifact checksums;
- known issues.

## Workstream F: go/no-go checklist

Create a checklist file:

- `docs/release/GO_NO_GO_CHECKLIST.md`

Required gates:

- workspace tests pass;
- Python tests pass or documented environment-only failures accepted;
- manifest strict validation passes;
- report consistency passes;
- differential tests pass for claimed drop-in features;
- wheels install and import;
- CLI binaries run;
- security docs exist;
- no stale docs found;
- release artifacts have checksums;
- unsupported features have diagnostics.

## Acceptance criteria

- Final certification document exists and is accurate.
- All release artifacts are install-tested.
- Generated parity report is current and checked.
- Differential evidence exists for all features claimed as pproxy drop-in where feasible.
- Unsupported/intentional non-parity items are clearly documented and diagnosable.
- Go/no-go checklist is complete.
- Release notes do not overclaim full pproxy parity unless true.

## Verification commands

```bash
cargo fmt --all -- --check
cargo test --workspace
python -m pytest python/tests -v
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --write-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
EGRESS_RUN_PPROXY_DIFFERENTIAL=1 cargo test -p eggress-cli --test pproxy_differential -- --ignored --test-threads=1
```

Add packaging-specific commands from Phase 49.

## Non-goals

- Do not add major feature work in this phase.
- Do not promote unsupported features for release optics.
- Do not publish artifacts before explicit approval.
- Do not suppress failing parity tests without updating manifest tiers.
