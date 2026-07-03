# Phase 36 Plan: Final Parity Release Audit

## Purpose

Phase 36 is the final release-readiness audit for the pproxy parity roadmap. By this point, the repo should have implemented or explicitly classified the major pproxy compatibility surfaces: protocols, CLI translation, UDP behavior, Shadowsocks, reverse/backward proxying, transparent/Unix/platform behavior, Python bindings, packaging, performance, and security containment.

The goal of this phase is to decide whether Eggress can cut a parity-oriented release, and if so, exactly what claims that release may make.

This phase is evidence-first. It should not add major features. It should close contradictions, rerun validation, generate release artifacts, and produce a final parity report that future users and maintainers can trust.

## Scope

This phase covers:

- Full manifest audit.
- Full docs/evidence consistency audit.
- pproxy differential test rerun.
- Python/PyPI packaging and import verification.
- Platform feature classification.
- Performance/security gate review.
- Release notes and migration guide.
- Final go/no-go decision.

## Non-goals

Do not add new major compatibility features in this phase.

Do not promote unsupported/deferred features to compatible without tests.

Do not publish production artifacts automatically.

Do not hide failed checks; record them as release blockers or accepted known limitations.

## Work items

### 36.1 Freeze the parity target

Confirm and document the target versions.

Required:

- pproxy target version: `2.7.9` unless deliberately changed.
- Rust toolchain version.
- Python supported versions.
- OS/platform support matrix.
- Eggress crate/package version for release candidate.

Output:

```text
docs/release/PARITY_TARGET_FREEZE.md
```

Acceptance:

- Every parity claim references the frozen target versions.

### 36.2 Manifest completeness audit

Audit `tests/compat/pproxy_manifest.toml`.

Checks:

- every feature has unique id;
- every feature has category/status/evidence/tier;
- compatible entries have compatible/differential/interoperability evidence;
- synthetic entries are not labeled pproxy behavioral parity;
- intentional non-parity entries have divergence rationale;
- supported entries have tests or explicit rationale;
- unsupported entries match docs;
- Python entries map to fixtures;
- platform entries include platform constraints;
- security/performance entries include evidence commands.

Add or extend testkit validation if any check is not mechanical.

Acceptance:

- Manifest validation becomes the release evidence gate.

### 36.3 Docs consistency audit

Audit all user-facing docs for claim consistency.

Files:

- README.md;
- docs/PARITY_MATRIX.md;
- docs/COMPATIBILITY_EVIDENCE.md;
- docs/REAL_PPROXY_PARITY_ROADMAP.md;
- docs/PPROXY_PARITY_SPEC.md;
- docs/PYTHON_BINDINGS.md;
- docs/python/*;
- docs/system_proxy/*;
- docs/performance/*;
- docs/security/*;
- docs/SECURITY_REVIEW.md;
- docs/OPERATIONS.md.

Rules:

- `Compatible` means pproxy behavior is tested for the stated scenario.
- `Supported` means Eggress supports it but pproxy equivalence is not claimed.
- `Partial` means useful subset only.
- `Intentional non-parity` means deliberately different with rationale.
- `Unsupported` means not implemented.
- `Experimental` means code exists but no stability/compat promise.

Acceptance:

- No docs contradict manifest tiers.
- No old completion docs are treated as current source of truth if later passes changed status.

### 36.4 Generate final parity report

Create a release report from manifest, tests, and manual notes.

Output:

```text
docs/release/FINAL_PPROXY_PARITY_REPORT.md
target/compat/final-pproxy-parity-report.json
```

Report sections:

- compatible features;
- supported but not parity-tested features;
- partial features;
- intentional non-parity;
- unsupported/deferred;
- platform-specific support;
- Python API support;
- security posture;
- performance baseline;
- known release blockers;
- accepted limitations.

Acceptance:

- Report is generated or at least mechanically traceable to manifest data.

### 36.5 Full differential/interoperability validation

Run all pproxy and external interop gates that are feasible.

Commands:

```bash
python -m pip install "pproxy==2.7.9"
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1
EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test -p eggress-runtime --test reverse_interop -- --ignored --test-threads=1
EGRESS_REQUIRE_PPROXY_PYTHON_API=1 python -m pytest python/tests/test_pproxy_oracle.py -q
```

Record:

- pass/fail/skip;
- environment;
- exact command;
- reason for any skip;
- whether a failure is release-blocking.

Acceptance:

- Final report distinguishes run tests from skipped/unavailable tests.

### 36.6 Full workspace validation

Run baseline project checks.

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p eggress-testkit manifest
cargo test -p eggress-testkit corpus
```

Python:

```bash
maturin develop
python -m pytest python/tests -q
python -m compileall python/eggress
scripts/test_wheel.sh
```

Supply chain:

```bash
cargo deny check
cargo audit
```

Record all outcomes.

Acceptance:

- Release audit includes exact validation status.

### 36.7 Performance and soak gate review

Run or review Phase 34 gates.

Minimum:

- local performance smoke;
- resource leak checks;
- reverse soak if feasible;
- Python binding overhead smoke.

Record:

- baseline file used;
- changes from baseline;
- known regressions;
- accepted deviations.

Acceptance:

- Release does not ship with unreviewed performance/resource regressions.

### 36.8 Security gate review

Run or review Phase 35 gates.

Minimum:

- threat model present;
- open-proxy warning tests;
- redaction tests;
- admin exposure tests;
- reverse containment tests;
- dependency checks;
- security review docs current.

Acceptance:

- Security posture is explicitly included in release decision.

### 36.9 Platform support matrix

Create final matrix.

Output:

```text
docs/release/PLATFORM_SUPPORT_MATRIX.md
```

Rows:

- Linux x86_64;
- Linux aarch64;
- macOS arm64;
- macOS x86_64;
- Windows x86_64;
- FreeBSD/other if relevant.

Columns:

- TCP proxy modes;
- UDP modes;
- Unix sockets;
- transparent/redir;
- macOS PF;
- system proxy inspect/apply;
- Python wheel availability;
- tests run;
- known limitations.

Acceptance:

- Platform-specific features do not have global claims.

### 36.10 Python/PyPI release readiness

Review Python package release criteria.

Checks:

- import strategy docs current;
- no top-level `pproxy` shadowing;
- wheel smoke passes;
- source distribution behavior documented;
- `py.typed` included;
- package metadata accurate;
- TestPyPI dry run if intended;
- production PyPI publish deferred or approved.

Acceptance:

- Python release readiness is separated from Rust crate release readiness if needed.

### 36.11 Migration guide and release notes

Create final user-facing notes.

Outputs:

```text
docs/release/MIGRATION_FROM_PPROXY_FINAL.md
docs/release/RELEASE_NOTES_PARITY_RC.md
```

Include:

- what can be migrated directly;
- what requires Eggress-native config;
- what is intentionally unsupported;
- CLI translation examples;
- Python examples;
- security warnings;
- platform caveats;
- performance expectations;
- known limitations.

Acceptance:

- Users can decide whether Eggress fits their pproxy workflow without reading source code.

### 36.12 Go/no-go checklist

Create final decision checklist.

Output:

```text
docs/release/PARITY_RELEASE_GO_NO_GO.md
```

Sections:

- required checks passed;
- release blockers;
- accepted limitations;
- deferred features;
- version/tag proposal;
- artifact list;
- rollback plan;
- owner sign-off fields.

Decision outcomes:

- **Go**: release may be tagged with stated claims.
- **Conditional go**: release may proceed only after listed blockers are fixed.
- **No-go**: release should not be tagged.

Acceptance:

- Release decision is explicit and evidence-backed.

## Validation commands

Full local release audit:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p eggress-testkit manifest
cargo test -p eggress-testkit corpus
maturin develop
python -m pytest python/tests -q
python -m compileall python/eggress
scripts/test_wheel.sh
cargo deny check
cargo audit
```

Gated parity/interoperability:

```bash
python -m pip install "pproxy==2.7.9"
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --test-threads=1
EGRESS_REQUIRE_REVERSE_INTEROP=1 cargo test -p eggress-runtime --test reverse_interop -- --ignored --test-threads=1
EGRESS_REQUIRE_PPROXY_PYTHON_API=1 python -m pytest python/tests/test_pproxy_oracle.py -q
```

Performance/security gates:

```bash
scripts/perf/run_local_baseline.sh
EGRESS_REQUIRE_SOAK=1 scripts/perf/run_soak.sh
cargo fuzz run pproxy_uri -- -max_total_time=60
```

## Acceptance criteria

Phase 36 is complete when:

- Target versions are frozen.
- Manifest and docs are internally consistent.
- Final parity report exists.
- Full workspace validation is recorded.
- Gated interop status is recorded with skips/failures explained.
- Platform support matrix exists.
- Python/PyPI readiness is documented.
- Migration guide and release notes exist.
- Go/no-go checklist records final release decision.

## Handoff notes

This is the audit phase. Do not add convenience features here. If a feature is not ready, classify it honestly and let the release proceed with a narrower claim set or mark the release no-go.
