# Phase 44: Trojan server, fallback, and interoperability completion

## Goal

Complete the Trojan parity surface enough that eggress can honestly classify Trojan support as more than client/upstream-only. The current state is useful for outgoing Trojan upstream connections, but pproxy parity requires a server/listener role, fallback semantics, authentication behavior, TLS behavior, and interop evidence.

This phase should promote Trojan-related manifest entries only when runtime behavior, config/compiler support, tests, and docs all agree.

## Current baseline

Known current state from prior audits:

- Trojan client/upstream support exists.
- Trojan URI parsing and password-only userinfo handling exist.
- Trojan listener/server is currently unsupported.
- Trojan fallback routing is incomplete.
- Real interoperability evidence is incomplete.
- Python exposure follows translator/service surfaces but should not overclaim server parity.

## Scope

### In scope

- Trojan inbound listener/server mode.
- TLS requirements and validation for Trojan listener mode.
- Password authentication semantics.
- Target address parsing and TCP relay behavior.
- Optional fallback routing behavior where pproxy supports it.
- TOML/config compiler representation for Trojan listeners.
- CLI pproxy translator support for `trojan://` local/listener URIs if implemented.
- Python diagnostics and compatibility report updates.
- Interoperability and differential tests against pproxy or a known Trojan implementation.
- Manifest/report/doc updates.

### Out of scope

- Trojan UDP unless the existing runtime already has safe support.
- QUIC/H3/Trojan-over-QUIC variants.
- Obfs/nonstandard extensions.
- SSR.
- SSH.

## Design questions to resolve first

1. Does pproxy support Trojan as both local listener and remote upstream, or only upstream in the relevant version?
2. What exact TLS defaults does pproxy apply for Trojan local/server mode?
3. What fallback behavior does pproxy implement for invalid password or non-Trojan traffic?
4. Does fallback route to a fixed endpoint, direct, another proxy, or close?
5. Are password-only URI forms required for both listener and upstream?
6. Does pproxy support multiple Trojan passwords or only one credential per URI?

Record answers in a short design note before implementation.

## Implementation tasks

### T1: Protocol server state machine

Inspect `crates/eggress-protocol-trojan` and implement/complete inbound handling:

- parse Trojan request line and CRLF framing;
- validate password hash/credential according to Trojan spec and pproxy behavior;
- parse command and target address;
- reject unsupported commands deterministically;
- expose a stream adapter compatible with the runtime relay path;
- bound parser sizes and timeouts;
- produce structured errors without leaking secrets.

### T2: Config model and compiler

Add or complete config support for Trojan listeners:

- listener protocol list should accept `trojan` only when TLS is configured or when explicit insecure mode is set for tests;
- auth/password source should be explicit and redacted in displays;
- compile-time validation should reject unsafe or incomplete Trojan listener configs;
- metrics labels should identify Trojan inbound without including credentials.

### T3: Runtime supervisor wiring

Wire Trojan inbound into listener dispatch:

- mixed inbound detection behavior must be defined; Trojan is usually TLS-wrapped and may not be autodetected from plaintext;
- ensure TLS termination happens at the correct layer;
- ensure half-close and shutdown semantics match other TCP proxies;
- enforce handshake timeout and connection limits.

### T4: pproxy translator updates

If pproxy supports local Trojan listener URIs, update `eggress-pproxy-compat`:

- stop rejecting supported `trojan://` local URIs;
- translate password-only userinfo correctly;
- generate listener TLS and Trojan auth config;
- add diagnostics for missing password, missing TLS, or unsupported fallback;
- update `check_pproxy_args` output.

If pproxy does not support local Trojan in the pinned version, keep rejection but fix manifest/docs accordingly.

### T5: Fallback behavior

Implement fallback only if pproxy semantics are clear and safe.

Potential outcomes:

- exact fallback parity if pproxy behavior is implementable;
- `compatible_with_warning` if eggress rejects invalid Trojan traffic rather than proxying fallback traffic;
- `intentional_non_parity` if fallback creates security ambiguity.

Do not silently accept fallback traffic without explicit config.

### T6: Tests

Add unit and integration tests for:

- valid Trojan request to TCP echo target;
- invalid password rejection;
- malformed request rejection;
- missing TLS config validation;
- password-only URI translation;
- credential redaction;
- listener startup with generated config;
- half-close behavior;
- fallback behavior if implemented.

Add interop/differential tests if feasible:

- eggress client -> pproxy Trojan server;
- pproxy client -> eggress Trojan server;
- eggress Trojan upstream against known Trojan server;
- pproxy CLI translation behavior for Trojan listener and upstream URIs.

### T7: Python exposure

Update Python tests and docs:

- `check_pproxy_args` should classify Trojan listener/upstream accurately;
- `PPProxyService.from_args` should preserve Trojan args and diagnostics;
- unsupported server/fallback combinations should raise `UnsupportedFeatureError` unless `allow_partial=True`.

### T8: Manifest/report/docs

Update:

- `docs/parity/pproxy_capability_manifest.toml`
- generated `docs/parity/PPROXY_PARITY_REPORT.md`
- `docs/PARITY_MATRIX.md`
- `docs/cli/PPROXY_CLI_INVENTORY.md`
- Trojan-specific docs if present

Do not promote `protocol.trojan_server` beyond actual evidence.

## Acceptance criteria

- Trojan client/upstream remains working.
- Trojan server/listener either works end-to-end or remains explicitly unsupported with corrected rationale.
- pproxy translator behavior matches pinned pproxy support for Trojan local/upstream URIs.
- Invalid credentials are rejected safely and tested.
- No credential appears in logs, reprs, reports, or generated diagnostics.
- Manifest tiers reflect actual runtime and evidence.
- Differential/interoperability evidence is clearly labeled.

## Verification commands

Run at minimum:

```bash
cargo fmt --all -- --check
cargo test -p eggress-protocol-trojan
cargo test -p eggress-pproxy-compat trojan
cargo test -p eggress-cli --test pproxy_cli trojan
cargo test --workspace
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
```

Run gated interop when available:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test -p eggress-cli --test differential_pproxy trojan -- --ignored --test-threads=1
```

## Non-goals

- Do not implement Trojan UDP in this phase unless already trivial and testable.
- Do not add nonstandard obfuscation.
- Do not weaken TLS verification defaults.
- Do not promote capabilities based on parser-only support.
