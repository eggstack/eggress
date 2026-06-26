# CI Status

## Hosted CI State

As of 2026-06-26, hosted GitHub Actions CI is **non-functional** due to a
billing/payment issue on the repository account. All workflow runs fail
immediately (within 1–2 seconds) with:

> The job was not started because recent account payments have failed or your
> spending limit needs to be increased. Please check the 'Billing & plans'
> section in your settings

This affects both `ci.yml` and `security.yml` workflows on every push to
`main` and every pull request. No jobs actually execute — the failures are
not code-related.

## Required Local Verification

Before merging or marking a phase complete, run **all** of the following:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

These correspond to the six CI jobs: Format, Check, Test, Clippy, Deny,
and Audit. If all pass locally, the changes are correct regardless of
hosted CI status.

## Workflows

### ci.yml (primary)

Triggers on pushes to `main` and pull requests to `main`. Contains these
separate visible jobs:

| Job | Runner | What it checks |
|-----|--------|----------------|
| Format | ubuntu-latest | `cargo fmt --all -- --check` |
| Check | ubuntu/macos/windows | `cargo check --workspace --all-targets` |
| Test | ubuntu/macos/windows | `cargo test --workspace` |
| Clippy | ubuntu-latest | `cargo clippy --workspace --all-targets -- -D warnings` |
| Deny | ubuntu-latest | `cargo deny check` (license + advisory) |
| Audit | ubuntu-latest | `cargo audit` (security advisories) |
| Interoperability | ubuntu-latest | Cross-implementation tests (pproxy, curl) |

### security.yml (legacy — removed)

Previously ran duplicate `cargo-deny` and `cargo-audit` jobs using older
action versions. Superseded by the Deny and Audit jobs in `ci.yml`. Removed
to avoid confusion and wasted billing minutes.

## How to Interpret Completion Docs

When hosted CI is unavailable, completion documents (e.g.
`PHASE_*_COMPLETION.md`) should record:

1. **Local verification commands run** — which of the six commands were
   executed and their pass/fail status.
2. **Local test output** — relevant test names and counts, not full trace.
3. **Absence of hosted CI** — explicitly note that hosted CI was not
   observable, and that local verification is the source of truth.

Do not claim "CI passed" when only local verification was performed. Say
"Local verification passed" instead.

## Known Blockers

- **Billing**: GitHub Actions minutes are unavailable until the repository
  owner resolves the payment issue in Settings → Billing & plans.
- **No secrets required**: None of the CI jobs depend on custom repository
  secrets. The only secret used is `GITHUB_TOKEN` (automatic).
- **No paid-only actions**: All actions used (`actions/checkout`,
  `dtolnay/rust-toolchain`, `Swatinem/rust-cache`,
  `EmbarkStudios/cargo-deny-action`) are free for public repositories.
