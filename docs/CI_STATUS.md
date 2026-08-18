# CI and Verification Policy

This document is the source of truth for repository verification. It supersedes older phase-completion documents and workflow descriptions that treated every available check as a mandatory gate.

## Policy

Egress uses deliberately small hosted CI. GitHub Actions is a smoke signal for ordinary development, not a release engine, compatibility evidence archive, or substitute for focused local testing.

The repository has two automatic workflows:

- `.github/workflows/ci.yml`: one Ubuntu Rust job running format, Clippy, and the workspace test suite.
- `.github/workflows/python-test.yml`: one path-scoped Ubuntu/Python 3.12 smoke job for the Python binding and compatibility packages.

There are no tag-triggered release workflows, artifact assembly workflows, publishing workflows, cross-platform release matrices, or mandatory compatibility-evidence uploads.

## Routine development

Use the narrowest local command that exercises the code being changed. Examples:

```bash
cargo test -p eggress-routing
cargo test -p eggress-runtime retry_fallback
cargo test -p eggress-cli --test cli_exit_codes
```

Formatting should normally be applied locally before commit:

```bash
cargo fmt --all
```

A routine change does not require security audits, cross-platform matrices, ignored interoperability suites, benchmark runs, parity-report regeneration, or completion-evidence documents unless the change directly affects those areas.

## Before merge

For a normal Rust change, the expected broad local check is:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

For a Python-facing change, also build the extension and run the relevant Python tests:

```bash
python3 -m venv .venv
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pip install --no-deps ./python-pproxy-compat
.venv/bin/python -m pytest python/tests tests/compat -q
```

A focused test may be sufficient during iteration. The broad workspace check is expected before merging a substantial change, not after every edit.

## Specialized checks

Run these only when their trigger condition applies:

| Check | Trigger |
|---|---|
| `cargo deny check` | Dependency, feature, or license-policy changes; release preparation |
| `cargo audit --ignore RUSTSEC-2025-0134` | Dependency changes; release preparation |
| pproxy differential/oracle suites | Compatibility behavior, manifests, URI translation, or pproxy namespace changes |
| Shadowsocks external interoperability | Shadowsocks wire-format, cipher, or relay changes |
| strict closure audit | Explicit compatibility-certification work |
| benchmarks, load, soak, or fuzzing | Performance, concurrency, parser, or hardening work |
| cross-platform local/hosted checks | Platform-specific code or release preparation |

The commands remain documented in `docs/TESTING.md`, `docs/DIFFERENTIAL_TESTING.md`, and `AGENTS.md`. Their existence does not make them routine merge gates.

## Evidence and completion records

Ordinary implementation work requires a clear commit message and passing relevant tests. It does not require generated parity reports, uploaded workflow artifacts, large completion documents, screenshots, or copied command transcripts.

Compatibility claims must still be backed by the applicable oracle or interoperability suite. Evidence should be generated when a claim changes or a release is being evaluated, not on every push.

## Release boundary

Release cadence is entirely manual. GitHub Actions must not publish crates, create GitHub Releases, push container images, build release bundles, or react to version tags.

The release operator performs release checks and `cargo publish` locally. See `docs/release/RELEASE_PROCESS.md`.

## Design rationale

The previous apparatus duplicated compilation and linting across multiple workflows, ran operating-system and Python-version matrices for routine changes, installed external implementations on every push, generated evidence artifacts continuously, and repeated the same gates inside release automation. That increased latency and maintenance without proportionate correctness benefit.

The lean policy preserves the highest-value invariant checks while moving expensive or environment-sensitive verification to the point where it is technically relevant.
