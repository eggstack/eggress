# Phase 33 Plan: System Proxy and Desktop Integration

## Purpose

Phase 33 covers the platform-facing system proxy functionality in the pproxy parity roadmap. Python `pproxy` exposes convenience behavior for setting or using system proxy settings; Eggress should decide which parts are safe and useful to implement, which are better left to OS tooling, and how to expose any supported behavior without surprising users.

This phase is intentionally conservative. System proxy mutation is operationally risky: it can break network connectivity, leak traffic through the wrong proxy, require privileges, and behave differently across Linux, macOS, Windows, desktop environments, browsers, and package managers. The goal is to provide reliable inspection and optional, explicit configuration helpers, not hidden global side effects.

## Scope

This phase covers:

- pproxy system-proxy behavior capture.
- Platform capability inventory for system proxy settings.
- Read-only system proxy inspection helpers.
- Optional explicit apply/revert helpers where safe.
- CLI diagnostics and dry-run output.
- Python helper exposure if implemented.
- Docs, manifest entries, tests, and release criteria.

## Non-goals

Do not silently change global system proxy settings during normal `eggress run` or `eggress pproxy run`.

Do not implement privileged OS integration without explicit opt-in and dry-run diagnostics.

Do not claim system proxy parity unless behavior is verified per platform against pproxy or a documented OS command oracle.

Do not attempt to manage every browser-specific proxy setting.

Do not make desktop integration a required dependency for headless/server deployments.

## Work items

### 33.1 Capture pproxy system proxy behavior

Document how `pproxy==2.7.9` handles system proxy operations.

Tasks:

- Identify pproxy CLI flags and APIs related to system proxy settings, especially `--sys` or equivalent behavior.
- Capture behavior on supported platforms if practical:
  - Linux desktop with environment variables / GNOME / KDE if available;
  - macOS network services;
  - Windows WinHTTP / Internet Settings if available.
- Record whether pproxy mutates global state, sets process-local environment variables, or delegates to OS tools.
- Capture failure behavior without privileges.
- Capture cleanup/revert behavior, if any.

Output:

```text
docs/system_proxy/PPROXY_SYSTEM_PROXY_BEHAVIOR.md
```

Acceptance:

- pproxy behavior is documented before Eggress implements or rejects any global proxy mutation.

### 33.2 Add system proxy capability model

Extend or add platform capability detection for system proxy support.

Potential type:

```rust
pub enum SystemProxyCapability {
    InspectEnvironment,
    InspectMacosNetworksetup,
    ApplyMacosNetworksetup,
    InspectWindowsInternetSettings,
    ApplyWindowsInternetSettings,
    InspectGnomeSettings,
    ApplyGnomeSettings,
    InspectKdeSettings,
    ApplyKdeSettings,
}
```

Tasks:

- Keep capability checks read-only unless explicitly invoked.
- Avoid shelling out during normal proxy startup.
- Distinguish:
  - platform unsupported;
  - desktop environment missing;
  - tool missing;
  - privilege missing;
  - dry-run available;
  - apply available.
- Add tests using injected command runners/mocks.

Acceptance:

- Capability checks do not mutate global system state.
- Docs distinguish inspection support from apply/revert support.

### 33.3 Implement read-only inspection first

Add read-only system proxy inspection.

Possible CLI:

```bash
eggress system-proxy inspect --json
eggress pproxy system-proxy inspect --json
```

Possible Python:

```python
eggress.system_proxy.inspect()
eggress.pproxy.system_proxy_inspect()
```

Inspection should report:

- detected platform;
- supported backends;
- current HTTP proxy if safely readable;
- current HTTPS proxy if safely readable;
- current SOCKS proxy if safely readable;
- no-proxy/bypass list if safely readable;
- whether apply/revert is supported;
- commands that would be used for apply if dry-run is requested.

Acceptance:

- Read-only inspection works without elevated privileges on supported platforms where possible.
- Unsupported platforms return structured diagnostics, not panics.

### 33.4 Design explicit apply/revert model

If system proxy mutation is implemented, make it transactional and explicit.

Required semantics:

- No mutation without `--apply` or equivalent explicit call.
- Always support `--dry-run`.
- Capture previous settings before applying.
- Write rollback state to an explicit file if requested.
- Provide `revert` command requiring rollback state or explicit values.
- Redact credentials from logs and rollback files where possible.
- Warn when proxy binds non-loopback addresses.

Potential CLI:

```bash
eggress system-proxy apply --http http://127.0.0.1:8080 --https http://127.0.0.1:8080 --dry-run
eggress system-proxy apply --from-listener main --write-rollback ~/.cache/eggress/proxy.rollback.json
eggress system-proxy revert ~/.cache/eggress/proxy.rollback.json
```

Acceptance:

- Apply/revert design is documented before broad implementation.
- Mutation is impossible by accident.

### 33.5 Implement platform backends incrementally

Implement only backends with testable behavior.

Recommended order:

1. Environment-only inspection and shell export generation.
2. macOS `networksetup` dry-run/inspection.
3. Windows read-only inspection.
4. Linux desktop environment support if stable.

For each backend:

- Add a trait-backed command runner for tests.
- Parse command output robustly.
- Add redaction for credentials.
- Add negative tests for missing tools and permission failures.
- Do not mark apply support unless revert is implemented.

Acceptance:

- Every backend has a safe failure path and tests.

### 33.6 pproxy compatibility classification

Add manifest entries such as:

```text
system_proxy_inspect
system_proxy_apply_macos
system_proxy_revert_macos
system_proxy_env_exports
pproxy_sys_flag
python_system_proxy_inspect
```

Classification rules:

- `Compatible` only if pproxy behavior is reproduced and tested on that platform.
- `Supported` for Eggress-native safer behavior.
- `Intentional non-parity` for hidden/global mutation behavior that Eggress rejects.
- `Unsupported` for platforms/backends not implemented.

Acceptance:

- Manifest does not overclaim platform-specific support.

### 33.7 CLI and Python integration

Expose system proxy features carefully.

CLI requirements:

- structured JSON output;
- human-readable diagnostics;
- dry-run default for mutation commands, or require explicit `--apply`;
- stable exit codes;
- credential redaction.

Python requirements, if implemented:

- helpers are read-only by default;
- mutation helpers require explicit function names such as `apply_system_proxy`, not ambiguous `set_proxy`;
- return dataclasses/dicts with diagnostics;
- no mutation at import time.

Acceptance:

- CLI and Python behavior share backend logic.

### 33.8 Tests

Add unit tests for:

- capability detection;
- command-runner parsing;
- credential redaction;
- dry-run command generation;
- rollback file schema;
- unsupported platform diagnostics;
- CLI JSON output;
- Python helper import/inspection if implemented.

Add gated/manual tests only for real platform mutation:

```bash
EGRESS_REQUIRE_SYSTEM_PROXY_APPLY=1 cargo test -p eggress-cli --test system_proxy -- --ignored --test-threads=1
```

Acceptance:

- Ungated tests do not mutate system settings.
- Gated tests are explicitly opt-in and documented.

### 33.9 Documentation

Add/update:

```text
docs/system_proxy/README.md
docs/system_proxy/PPROXY_SYSTEM_PROXY_BEHAVIOR.md
docs/system_proxy/MACOS_NETWORKSETUP.md
docs/system_proxy/WINDOWS_PROXY_SETTINGS.md
docs/system_proxy/LINUX_DESKTOP_PROXY.md
docs/SECURITY_REVIEW.md
docs/PARITY_MATRIX.md
docs/COMPATIBILITY_EVIDENCE.md
```

Docs must state:

- system proxy mutation is explicit only;
- inspection is safe/read-only;
- rollback limitations;
- platform caveats;
- how to recover manually if settings break;
- why Eggress may intentionally diverge from pproxy global mutation behavior.

## Validation commands

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p eggress-system-proxy
cargo test -p eggress-cli system_proxy
cargo test -p eggress-testkit manifest
```

If Python helpers exist:

```bash
maturin develop
python -m pytest python/tests/test_system_proxy.py -q
```

Gated manual mutation:

```bash
EGRESS_REQUIRE_SYSTEM_PROXY_APPLY=1 cargo test -p eggress-cli --test system_proxy -- --ignored --test-threads=1
```

## Acceptance criteria

Phase 33 is complete when:

- pproxy system proxy behavior has been captured.
- Eggress has a documented system proxy capability model.
- Read-only inspection exists or is explicitly deferred.
- Mutation support, if any, requires explicit apply and supports dry-run/revert.
- Platform-specific support is tested and conservatively classified.
- CLI/Python docs do not imply hidden global mutation.
- Manifest/evidence docs accurately describe support and non-parity.

## Handoff notes

System proxy integration is a trust boundary. Prefer read-only inspection and explicit generated commands over automatic OS mutation. If behavior is not reversible, do not implement it as an automated command.
