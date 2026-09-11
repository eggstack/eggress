# CI and Verification Policy

This document is the source of truth for repository verification. It supersedes older phase-completion documents and workflow descriptions that treated every available check as a mandatory gate.

## Policy

Egress uses deliberately small hosted CI. GitHub Actions is a smoke signal for ordinary development, not a release engine, compatibility evidence archive, or substitute for focused local testing.

The repository has four hosted workflows:

- `.github/workflows/ci.yml`: one Ubuntu Rust job running format, Clippy, the default workspace test suite, one bounded optional-compat compile check, and fuzz-target compilation.
- `.github/workflows/python-test.yml`: one path-scoped Ubuntu/Python 3.12 smoke job for the Python binding and compatibility packages.
- `.github/workflows/publish-python.yml`: a real release path, not a smoke job. It fires on every `v*` tag push, validates the tag against the workspace version, builds five-platform abi3 wheels plus an sdist, smoke-tests them, and publishes to PyPI through the protected `pypi` GitHub environment (TestPyPI only via manual dispatch).
- `.github/workflows/release-binaries.yml`: a real release path, not a smoke job. It fires on every `v*` tag push (or manual dispatch against an existing tag), validates the tag with `scripts/release-preflight.sh`, builds the five canonical `eggress-cli` target archives with default features, smoke-tests both executables natively, and creates/updates the GitHub Release with archives, SHA-256 sidecars, and installers. Ordinary CI never builds this matrix.

There are no crates-publishing workflows, cross-platform ordinary-CI matrices, or mandatory compatibility-evidence uploads. Pushing a version tag is a release action; ordinary pushes never publish anything.

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

The Ubuntu Rust job additionally runs one bounded compile-only gate for
product-relevant optional compatibility features that the default `full`
group intentionally leaves off. It is compile verification, not a second
full test suite, and it deliberately excludes the insecure/test-only
`insecure-quic` combination:

```bash
cargo check -p eggress-cli --locked --no-default-features \
  --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon \
  --bins
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
| `cargo audit --ignore RUSTSEC-2025-0134 --ignore RUSTSEC-2023-0071 --ignore RUSTSEC-2026-0009` | Dependency changes; release preparation |
| optional-compat compile gate above | SSH, QUIC/H3, SSR/`pproxy-legacy`, legacy-crypto, or daemon code paths |
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

Rust crates.io publication is entirely manual: the release operator runs local checks and `cargo publish` in dependency order. See `docs/release/RELEASE_PROCESS.md`.

The exceptions to "no publishing automation" are the two tag-triggered release paths: pushing a `v*` tag triggers `publish-python.yml` (PyPI wheel/sdist via the protected `pypi` environment) and `release-binaries.yml` (prebuilt CLI archives + GitHub Release). Both hard-fail on a tag/version mismatch, so a tag push must always be a deliberate release act. GitHub Actions still does not publish crates, push container images, or duplicate the ordinary suite inside release jobs. See `docs/release/RELEASE_PROCESS.md`.

## Design rationale

The previous apparatus duplicated compilation and linting across multiple workflows, ran operating-system and Python-version matrices for routine changes, installed external implementations on every push, generated evidence artifacts continuously, and repeated the same gates inside release automation. That increased latency and maintenance without proportionate correctness benefit.

The lean policy preserves the highest-value invariant checks while moving expensive or environment-sensitive verification to the point where it is technically relevant.
