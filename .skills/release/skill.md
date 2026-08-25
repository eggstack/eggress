# Release Process

## When to use
Use when cutting a new release — bumping versions, verifying the release candidate, publishing Python packages to PyPI, and optionally publishing Rust crates to crates.io.

## Version convention

Default bump is **+0.0.1** (patch). Use +0.1.0 (minor) or +1.0.0 (major) only when explicitly requested.

Current version locations — all must be moved in lockstep:

| File | Field | Notes |
|------|-------|-------|
| `Cargo.toml` (workspace) | `[workspace.package]` `version` | Authoritative workspace version |
| `Cargo.toml` (workspace) | every internal `=x.y.z` pin under `[workspace.dependencies]` (~24 entries) | Required for crates.io resolution |
| `crates/eggress-python/pyproject.toml` | `project.version` | Python wheel |
| `python-pproxy-compat/pyproject.toml` | `project.version` (+ its `eggress==x.y.z` dependency pin) | Opt-in compat distribution |

Notes:
- `crates/eggress-python/Cargo.toml` uses `version.workspace = true`; it has no independent version.
- `python/pyproject.toml` is a local dev convenience and inherits the workspace version.
- The publish workflow hard-fails if a pushed tag does not match the workspace version.

## Release steps

### 1. Determine the new version

```bash
# Read current version from workspace Cargo.toml
grep '^version' Cargo.toml | head -1
```

Increment by +0.0.1 unless the user specifies otherwise.

### 2. Update all version files

Use `sed` or manual edits to update every version location listed above. Verify all files match:

```bash
rg -n '1\.<old>' Cargo.toml crates/eggress-python/pyproject.toml python-pproxy-compat/pyproject.toml
```

### 3. Verify the release candidate

Run the broad local gate on the release commit:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Run dependency checks (release inputs are changing):

```bash
cargo deny check
cargo audit --ignore RUSTSEC-2025-0134
```

For Python-facing releases, also verify the Python build:

```bash
python3 -m venv .venv-release
.venv-release/bin/python -m pip install --upgrade pip
.venv-release/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv-release/bin/maturin develop)
.venv-release/bin/python -m pytest python/tests tests/compat -q
```

### 4. Commit the version bump

```bash
git add -A
git commit -m "release: bump version to v<new_version>"
git push origin main
```

### 5. Verify the release-only Python workflow

The authoritative workflow is `.github/workflows/publish-python.yml`. It is
release-only and bounded to five `cp39-abi3` wheels (Linux x86_64/aarch64,
macOS x86_64/arm64, Windows x86_64) plus one sdist. Linux wheels use the
manylinux2014/glibc 2.17 floor. The collector parses wheel filenames and fails
on missing targets, duplicate targets, non-abi3 wheels, debug artifacts, or an
unexpected sdist. Its installed-artifact smoke is
`scripts/release_artifact_smoke.py`, which imports both `eggress` and top-level
`pproxy`, starts a port-0 service, checks readiness/bound addresses, shuts it
down, and checks readiness is false.

Manually dispatch the workflow with `publish_target=testpypi` using a version
that is safe for TestPyPI. Confirm every build, collection, wheel smoke,
compatibility-range smoke, sdist smoke, and publish job succeeds. Install one
published artifact from TestPyPI in a clean environment and run the same smoke
script. Record the run URL and artifact filenames in the corrective plan before
production use. Do not push a production tag solely to test the workflow.

### 6. Publish Python package (trusted publisher)

Push a version tag to trigger the `publish-python.yml` workflow:

```bash
git tag -a v<new_version> -m "Release v<new_version>"
git push origin v<new_version>
```

The workflow builds prebuilt wheels for Linux (x86_64, aarch64), macOS (x86_64, arm64), and Windows (x86_64), plus one source distribution, then publishes to PyPI via OIDC trusted publishers. Production publication enforces version coherence and fails on existing versions rather than skipping.

Verify publication:
- Check the workflow run: `gh run list --workflow=publish-python.yml --limit=1`
- Verify on PyPI: `curl -s https://pypi.org/pypi/eggress/<new_version>/json | python -m json.tool | head -5`

### 7. Publish Rust crates to crates.io (manual)

Crates.io publication is operator-driven; no workflow publishes crates. All
internal crates carry crates.io metadata and are publishable. Because crates.io
rate-limits new crate publications to roughly one per 10 minutes, use the
tiered helper which publishes in dependency order and waits out the cooldown:

```bash
# Dry run first
./scripts/publish-remaining.sh --dry-run

# Publish remaining crates in dependency order (~4h wall time)
./scripts/publish-remaining.sh
```

For a single-crate release, publish manually in dependency order (top-level
facades such as `eggress-cli` last):

```bash
cargo publish --dry-run -p eggress-cli
cargo publish -p eggress-cli
```

Verify:
```bash
cargo install eggress-cli --version <new_version> --locked --root /tmp/eggress-release-check
/tmp/eggress-release-check/bin/eggress --version
/tmp/eggress-release-check/bin/pproxy --help
```

## Roll-forward policy

Crates.io versions are immutable. For a defective release:
1. Yank the affected version when appropriate.
2. Fix the defect.
3. Increment the version.
4. Repeat verification and publish.

Do not delete or retag an existing version to simulate replacement.

## Troubleshooting

### Python publish workflow didn't trigger
- Ensure the tag matches `v*` pattern (e.g., `v1.0.2`)
- The workflow hard-fails if the tag does not equal the workspace version
- Check that the protected `pypi` environment exists in repo settings
- Verify trusted publisher configuration on PyPI matches: repo `eggstack/eggress`, workflow `publish-python.yml`

### Maturin build fails in CI
- Ensure `rust-toolchain.toml` specifies a valid toolchain
- Check that the workflow uses `dtolnay/rust-toolchain@stable`

### crates.io publish fails
- Respect the ~10-minute crates.io cooldown for new crate names (`scripts/publish-remaining.sh` handles this; override with `EGGRESS_PUBLISH_DELAY_SECONDS`)
- Wait for index propagation between dependent publishes; re-run the dependent dry run if resolution is uncertain
- Use `cargo publish --dry-run` to diagnose packaging issues
