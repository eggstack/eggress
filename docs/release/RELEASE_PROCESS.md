# Release Process (Phase 49)

This document defines the end-to-end release process for eggress, covering
Rust binaries, Python wheels, container images, and all associated artifacts.

## Prerequisites

- Rust stable toolchain (MSRV 1.75)
- Python >= 3.9 with `maturin` installed
- `gh` CLI authenticated (for GitHub Release creation)
- `cargo-deny` and `cargo-audit` installed
- Write access to the repository

## Pre-release checklist

### 1. Code quality gates

All of these must pass on `main` before tagging:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

### 2. Python package verification

```bash
cd crates/eggress-python
maturin build --release --out ../../dist
cd ../..
./scripts/test_wheel.sh
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
```

### 3. Manifest validation

```bash
cargo test -p eggress-testkit --lib manifest
cargo test -p eggress-testkit --lib corpus
```

### 4. Version alignment

Verify version is consistent across:

- `Cargo.toml` workspace version
- `crates/eggress-python/pyproject.toml`
- `crates/eggress-python/Cargo.toml`
- `python/eggress/__init__.py` (`__version__`)

### 5. Documentation review

- [ ] Release notes updated (`docs/release/RELEASE_NOTES_PARITY_RC.md`)
- [ ] Platform support matrix current (`docs/release/PLATFORM_SUPPORT_MATRIX.md`)
- [ ] Migration guide current (`docs/release/MIGRATION_FROM_PPROXY_FINAL.md`)
- [ ] README install section accurate

## Release steps

### Step 1: Create release branch (optional)

```bash
git checkout -b release/v0.1.0
```

### Step 2: Tag the release

```bash
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

### Step 3: Wait for CI

The `v*` tag triggers:

- **`python-wheels.yml`**: Builds wheels for 5 platforms, uploads as artifacts
- **`release.yml`**: Builds CLI binaries, generates checksums/SBOM, creates GitHub Release

### Step 4: Create GitHub Release

If not auto-created by CI:

```bash
gh release create v0.1.0 \
  --title "eggress v0.1.0" \
  --notes-file docs/release/RELEASE_NOTES_PARITY_RC.md \
  --draft
```

### Step 5: Upload artifacts to GitHub Release

```bash
gh release upload v0.1.0 \
  dist/eggress-*.tar.gz \
  dist/eggress-*.zip \
  dist/SHA256SUMS \
  dist/sbom.json
```

### Step 6: Publish Python package to PyPI

Trigger the `publish-pypi.yml` workflow manually, or:

```bash
cd crates/eggress-python
maturin upload --repository pypi ../../dist/*
```

### Step 7: Build and push container image

```bash
docker buildx build --platform linux/amd64,linux/arm64 \
  -t ghcr.io/{owner}/eggress:v0.1.0 \
  -t ghcr.io/{owner}/eggress:latest \
  --push .
```

### Step 8: Finalize GitHub Release

```bash
gh release edit v0.1.0 --draft=false
```

## Post-release

### Verify installation methods

```bash
# Binary
./eggress --version

# Python
pip install eggress==0.1.0
python -c "import eggress; print(eggress.__version__)"

# Container
docker pull ghcr.io/{owner}/eggress:v0.1.0
docker run --rm ghcr.io/{owner}/eggress:v0.1.0 --version
```

### Update documentation

- [ ] Tag the release in README badge/links
- [ ] Update ROADMAP.md with completed phase
- [ ] Create `docs/release/RELEASE_v0.1.0.md` if needed

## Rollback

If a critical regression is found:

1. **Yank Python wheels**: `twine yank eggress==0.1.0`
2. **Delete GitHub Release** (binaries are not yankable)
3. **Push patch**: `v0.1.1` with the fix
4. **Rebuild and republish** all artifacts

## Versioning

- Use semantic versioning: `MAJOR.MINOR.PATCH`
- Tags use the format `vX.Y.Z`
- Python package version must match the git tag version
- Workspace `Cargo.toml` version is the single source of truth

## CI/CD flow

```
git tag v0.1.0
  │
  ├─→ python-wheels.yml (v* trigger)
  │     ├─ Build wheels (5 platforms)
  │     ├─ Build sdist
  │     └─ Upload artifacts
  │
  └─→ release.yml (v* trigger)
        ├─ Build CLI binaries (5 targets)
        ├─ Generate SHA256SUMS
        ├─ Generate SBOM
        ├─ Build container image
        └─ Create GitHub Release with all artifacts
```
