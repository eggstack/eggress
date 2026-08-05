# Release Process

## When to use
Use when cutting a new release — bumping versions, verifying the release candidate, publishing Python packages to PyPI, and optionally publishing Rust crates to crates.io.

## Version convention

Default bump is **+0.0.1** (patch). Use +0.1.0 (minor) or +1.0.0 (major) only when explicitly requested.

Current version locations — all must be updated together:

| File | Field | Package |
|------|-------|---------|
| `Cargo.toml` (workspace) | `version` | Workspace (Rust crates) |
| `crates/eggress-python/Cargo.toml` | `version` | Python binding crate |
| `crates/eggress-python/pyproject.toml` | `project.version` | Python wheel (authoritative) |
| `python/pyproject.toml` | `project.version` | Local dev convenience |

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
rg -n 'version\s*=\s*"1\.' Cargo.toml crates/eggress-python/Cargo.toml crates/eggress-python/pyproject.toml python/pyproject.toml
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

### 5. Publish Python package (trusted publisher)

Push a version tag to trigger the `publish-python.yml` workflow:

```bash
git tag -a v<new_version> -m "Release v<new_version>"
git push origin v<new_version>
```

The workflow builds prebuilt wheels for Linux (x86_64, aarch64), macOS (x86_64, arm64), and Windows (x86_64), plus one source distribution, then publishes to PyPI via OIDC trusted publishers. Production publication enforces version coherence and fails on existing versions rather than skipping.

Verify publication:
- Check the workflow run: `gh run list --workflow=publish-python.yml --limit=1`
- Verify on PyPI: `curl -s https://pypi.org/pypi/eggress/<new_version>/json | python -m json.tool | head -5`

### 6. Publish Rust crates to crates.io (manual)

Crates.io publication is manual and currently blocked until crate boundaries are restructured. When ready:

```bash
# Dry run first
cargo publish --dry-run -p eggress-cli

# Publish (dependency order matters)
cargo publish -p eggress-cli
```

Verify:
```bash
cargo install eggress-cli --version <new_version> --locked --root /tmp/eggress-release-check
/tmp/eggress-release-check/bin/eggress --version
```

### 7. Optional: create GitHub Release

```bash
gh release create v<new_version> \
  --title "eggress v<new_version>" \
  --generate-notes
```

## Roll-forward policy

Crates.io versions are immutable. For a defective release:
1. Yank the affected version
2. Fix the defect
3. Increment the version
4. Repeat verification and publish

Do not delete or retag an existing version.

## Troubleshooting

### Python publish workflow didn't trigger
- Ensure the tag matches `v*` pattern (e.g., `v1.0.2`)
- Check that `pypi` environment exists in repo settings
- Verify trusted publisher configuration on PyPI matches: repo `eggstack/eggress`, workflow `publish-python.yml`

### Maturin build fails in CI
- Ensure `rust-toolchain.toml` specifies a valid toolchain
- Check that the workflow uses `dtolnay/rust-toolchain@stable`

### crates.io publish fails
- Internal crates are marked `publish = false` — this is expected until crate boundaries are restructured
- Use `cargo publish --dry-run` to diagnose packaging issues
