# Platform Support Matrix (Phase 36)

This matrix records per-platform feature availability for the parity release
candidate. Every claim here must be backed by either a passing differential
test, a unit test, or an explicit `intentional_non_parity` rationale in the
manifest.

**Source of truth:** `tests/compat/pproxy_manifest.toml`.
**Frozen targets:** `docs/release/PARITY_TARGET_FREEZE.md`.

## Legend

- ✅ Supported and tested
- 🟡 Supported with caveats (see notes)
- 🟦 Eggress-native extension (no pproxy equivalent)
- ⛔ Intentional non-parity
- ❌ Not implemented / not applicable

## Matrix

| Feature | Linux x86_64 | Linux aarch64 | macOS arm64 | macOS x86_64 | Windows x86_64 | Unix (other) |
|---|---|---|---|---|---|---|
| TCP proxy modes (HTTP CONNECT, SOCKS4/4a/5) | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| HTTP forward proxy (RFC 7230) | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| Shadowsocks AEAD TCP (SIP003) | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| Trojan TLS upstream (client) | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | ❌ |
| UDP direct forwarding | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| UDP SOCKS5 ASSOCIATE relay | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | ❌ |
| UDP standalone relay (`mode = standalone_pproxy_udp`) | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| UDP Shadowsocks AEAD | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| Unix domain socket listener | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| Transparent TCP proxy (`SO_ORIGINAL_DST`, Linux) | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| macOS PF original-destination recovery | ⛔ | ⛔ | ⛔ | ⛔ | ⛔ | ⛔ |
| Reverse / backward proxy (control channel) | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| System proxy inspection (read-only) | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| System proxy apply (dry-run only) | ❌ | ❌ | 🟡 | 🟡 | ❌ | ❌ |
| System proxy revert (macOS only) | ❌ | ❌ | 🟡 | 🟡 | ❌ | ❌ |
| Hot-reload of routing + upstreams | 🟦 | 🟦 | 🟦 | 🟦 | 🟦 | ❌ |
| HTTP/2 CONNECT tunnel | 🟦 | 🟦 | 🟦 | 🟦 | 🟦 | ❌ |
| WebSocket tunnel (server + upstream) | 🟦 | 🟦 | 🟦 | 🟦 | 🟦 | ❌ |
| Raw fixed-target tunnel | 🟦 | 🟦 | 🟦 | 🟦 | 🟦 | ❌ |
| Python embed API (`eggress-embed`) | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| Python package on PyPI | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| Prometheus metrics endpoint | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| `eggress pproxy translate` (pproxy CLI parity) | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| `eggress pproxy check --json` | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| `eggress pproxy run` | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| `eggress system-proxy inspect` | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| Cargo deny + cargo audit in CI | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| Property tests (parsers, codecs, routes) | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| Resource-leak smoke tests | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |

## Platform-specific notes

### Linux x86_64 / aarch64

- Transparent TCP proxy uses `SO_ORIGINAL_DST` (IPv4) and `IP6T_SO_ORIGINAL_DST`
  (IPv6). Both are tested with a Linux capability probe.
- The kernel feature for transparent bind (`IP_TRANSPARENT`) is required for
  the listener; eggress reports `MissingPrivilege` or `KernelUnsupported`
  through `eggress-runtime::platform::check_capability()`.

### macOS arm64 / x86_64

- The `SO_ORIGINAL_DST` equivalent (`pf` + `rdr` rule) is **not** implemented.
  This is `intentional_non_parity` per manifest entry `macos_pf_transparent_proxy`.
- `eggress-runtime::platform` reports macOS PF capability as `KernelUnsupported`
  on all macOS versions tested (audit H3 in Phase 25-28 hardening).
- System proxy inspection uses `networksetup -getwebproxy` /
  `-getsecurewebproxy` / `-getsocksfirewallproxy`. Apply and revert use
  `networksetup -setwebproxy` etc. All operations are explicit (dry-run by
  default).

### Windows x86_64

- Unix domain sockets are not implemented (`intentional_non_parity` for that
  row).
- Transparent TCP proxy is not applicable (`intentional_non_parity`).
- System proxy inspection reads the registry:
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`.

### Unix (other)

- Out of release scope. No wheels, no CI, no compatibility claim.

## Python wheel availability

The CI matrix builds wheels for the following target triples:

- `cp39-manylinux_x86_64`, `cp310-manylinux_x86_64`, `cp311-manylinux_x86_64`,
  `cp312-manylinux_x86_64`, `cp313-manylinux_x86_64`, `cp314-manylinux_x86_64`
- `cp39-manylinux_aarch64`, `cp310-manylinux_aarch64`, `cp311-manylinux_aarch64`,
  `cp312-manylinux_aarch64`, `cp313-manylinux_aarch64`, `cp314-manylinux_aarch64`
- `cp39-macosx_x86_64`, …, `cp314-macosx_x86_64`
- `cp39-macosx_arm64`, …, `cp314-macosx_arm64`
- `cp39-win_amd64`, …, `cp314-win_amd64`

Source distribution (`sdist`) is built for any platform but requires a Rust
toolchain at install time.

## How to update this matrix

When adding a platform-specific feature:

1. Add or update the manifest entry with `category = "platform"` and a
   divergence that names the supported platforms (Linux, macOS, Windows, Unix).
2. Update the matching row in this matrix.
3. Run `cargo test -p eggress-testkit --lib manifest` to confirm the validator
   passes.
4. Reference this file from `PARITY_MATRIX.md` if the feature is user-facing.