# Phase 51 Go / No-Go Checklist

**Decision status:** **CONDITIONAL GO for the committed implementation; NO-GO
for publishing until the release-blocking external oracle suite is green or
formally re-baselined.**
**Certification document:** [`FINAL_PPROXY_PARITY_CERTIFICATION_TRACK_BC.md`](FINAL_PPROXY_PARITY_CERTIFICATION_TRACK_BC.md)

The Phase 51 document is retained as a historical snapshot; current counts and
Python packaging claims come from the Track B/C certification and generated
parity report.

## Required Checks

| Check | Result | Evidence |
|---|---|---|
| Manifest validated (strict mode) | ✅ | `python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml` |
| Report regenerated and consistent | ✅ | `python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml` |
| Manifest tests pass (86/86) | ✅ | `cargo test -p eggress-testkit --lib manifest` |
| Workspace unit/lib and doctests pass | ✅ | `cargo test --workspace` |
| Property tests pass (61) | ✅ | `cargo test -p eggress-protocol-socks --test codec_properties`, `cargo test -p eggress-protocol-http --test connect_properties`, `cargo test -p eggress-protocol-trojan --test request_properties`, `cargo test -p eggress-routing --test properties` |
| Format check pass | ✅ | `cargo fmt --all -- --check` |
| Clippy pass | ✅ | `cargo clippy --workspace --all-targets -- -D warnings` |
| CLI binary builds | ✅ | `cargo build --release -p eggress-cli` |
| CLI runs correctly | ✅ | `cargo run --bin eggress -- --help` |
| pproxy compat tests pass (274) | ✅ | `cargo test -p eggress-pproxy-compat` |
| Docs consistency audit complete | ✅ | Manifest claims consistent with codebase |
| Native Python stream tests pass | ✅ | `python/tests/test_proxy_connection.py` |
| Clean `eggress-pproxy-compat` wheel smoke test | ✅ | Matching `eggress` + `pproxy` imports in isolated environment |
| Redacted release evidence bundle generated | ✅ | `python3 scripts/release_evidence.py` |
| Release notes created | ✅ | [`RELEASE_NOTES_PHASE_51.md`](RELEASE_NOTES_PHASE_51.md) |
| Track B/C certification document created | ✅ | [`FINAL_PPROXY_PARITY_CERTIFICATION_TRACK_BC.md`](FINAL_PPROXY_PARITY_CERTIFICATION_TRACK_BC.md) |

## Pending / Environment-Limited Checks

| Check | Result | Notes |
|---|---|---|
| Pinned pproxy differential suite | ❌ Reference-side failure | `pproxy==2.7.9` starts under Python 3.11, but representative SOCKS5 and HTTP echo cases return an empty payload while Eggress returns the payload. Retain as a release blocker; do not claim differential parity. |
| Integration tests pass | ✅ | `cargo test --workspace` completed successfully locally. Hosted CI status is not verified; local verification is the source of truth. |
| Python wheel installable on target platform | ⚠️ Environment limitation | arm64 wheel on x86_64 Python mismatch; not a code defect. Wheels built per-arch in CI. |

## Release Blockers

The pinned external pproxy differential suite remains a publication blocker.
The failure is in the reference-side result, but it still prevents a passing
differential claim until the harness/reference invocation is corrected and
rerun.

## Accepted Limitations

1. Integration tests require port binding; environment-specific, not
   a code defect.
2. Python wheel requires matching architecture; cross-arch install not
   supported.
3. pproxy 2.7.9 differential tests require Python 3.11/3.12 (not 3.14).
4. No hosted CI visibility; local verification is the source of truth.
