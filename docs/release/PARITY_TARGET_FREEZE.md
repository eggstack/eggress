# Parity Target Freeze (Phase 36)

This document freezes the versions and platforms that the eggress parity release
candidate targets. All compatibility claims elsewhere in this repository are
interpreted relative to these targets.

## Status: FROZEN

This freeze is in effect as of Phase 36. Any change to the frozen targets
requires a new release audit.

## Pinned pproxy version

| Target | Value | Source of truth |
|---|---|---|
| pproxy | `2.7.9` | `tests/compat/pproxy_manifest.toml::meta.pproxy_version` |
| pproxy Python module | `pproxy==2.7.9` | Used by gated differential tests |

The pproxy version is enforced at build time by `eggress-testkit`'s manifest
validator (`PINNED_PPROXY_VERSION`).

## Eggress crate / package versions

| Artifact | Version | Notes |
|---|---|---|
| Workspace crate version | `0.1.0` | `Cargo.toml` `[workspace.package]` |
| `eggress` Python package | `0.1.0` | `crates/eggress-python/Cargo.toml`, exposed via `eggress.__version__` |
| Release tag (proposed) | `v0.1.0` | First parity release candidate |

## Rust toolchain

| Component | Version | Notes |
|---|---|---|
| MSRV | `1.75` | `crates/*/Cargo.toml` `rust-version` |
| Stable tested | `1.96.0` | Local verification on 2026-07-03 (Phase 36) |
| Edition | `2021` | All workspace crates |
| Unsafe policy | `forbid` | `unsafe_code = "forbid"` everywhere |

## Python support matrix

| Python | Status | Notes |
|---|---|---|
| `3.8` | Not built | End-of-life; no wheel |
| `3.9` | Built | Oldest supported line per Phase 15 wheels |
| `3.10` | Built | Standard tier |
| `3.11` | Built | Primary CI target; required for differential tests against `pproxy==2.7.9` |
| `3.12` | Built | Standard tier |
| `3.13` | Built | Standard tier |
| `3.14` | Built but **incompatible with `pproxy==2.7.9` differential tests** | The eggress Python package itself runs; pproxy 2.7.9 uses `asyncio.get_event_loop()` which raises on 3.14. Differential tests must run under Python 3.11. |

The Python support matrix is built per-platform via maturin. See
`docs/PYPI_RELEASE.md` for wheel CI matrix.

## OS / platform support matrix

| Platform | Tier | Notes |
|---|---|---|
| Linux x86_64 | Full | All features; primary CI |
| Linux aarch64 | Full | All features; CI builds wheels |
| macOS arm64 (Apple Silicon) | Full | Primary development host |
| macOS x86_64 (Intel) | Full | CI builds wheels |
| Windows x86_64 | Mostly full | All features **except** Unix domain sockets and Linux-specific transparent proxy |
| FreeBSD | Not built | No wheel, no CI; not a release target |
| Other Unix | Not built | Out of scope |

Platform-specific features are documented per-row in
`docs/release/PLATFORM_SUPPORT_MATRIX.md`.

## Categories in scope

The parity manifest (`tests/compat/pproxy_manifest.toml`) tracks these
categories. The validator enforces this list:

```
protocol, udp, routing, security, cli, uri, transport, platform,
system_proxy, python, python-api, packaging, performance,
inbound_tcp, upstream_tcp
```

## Status vocabulary

The manifest uses six statuses. All other docs that talk about parity should
use these definitions verbatim:

| Status | Meaning | Evidence requirement |
|---|---|---|
| `compatible` | pproxy behavior is tested for the stated scenario. | `evidence_level = "compatible"` with at least one differential test, an `external_dependency` reference, and a non-empty divergence rationale. |
| `supported` | Eggress supports it; pproxy equivalence is **not claimed**. | `evidence_level = "implemented_synthetic"` or `implemented_interop` or `implemented_differential`. |
| `partial` | Useful subset only. | `evidence_level` of any kind, with divergence documenting the subset. |
| `intentional_non_parity` | Deliberately different from pproxy. | `evidence_level` must be `intentional_non_parity` or `implemented_synthetic`. Requires non-empty divergence. |
| `unsupported` | Not implemented. | `evidence_level = "unimplemented"`. Requires non-empty divergence explaining why. |
| `experimental` | Code exists but no stability / compat promise. | `evidence_level` of any kind. Requires non-empty divergence describing the experiment. |

## How to reference this freeze

When you write or update a parity claim in any user-facing doc, link to this
file with the specific section. The `eggress-testkit` manifest validator
enforces structural consistency; semantic consistency is enforced by the
audit tests in `manifest.rs`.

## Changelog

| Date | Author | Change |
|---|---|---|
| 2026-07-03 | Phase 36 audit | Initial freeze. |