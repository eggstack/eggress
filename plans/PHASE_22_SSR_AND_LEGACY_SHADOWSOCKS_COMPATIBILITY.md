# Phase 22 Plan: ShadowsocksR and Legacy Shadowsocks Compatibility

## Purpose

Phase 22 addresses the long-tail Shadowsocks compatibility surface that pproxy exposes beyond modern Shadowsocks AEAD. This includes ShadowsocksR, legacy stream ciphers, OTA-related behavior, protocol plugins, and obfuscation modes if they are present in the target pproxy version.

This phase must be handled separately from Phase 21 because the compatibility/security tradeoff is different. Modern Shadowsocks AEAD can be part of normal safe operation. SSR and legacy stream ciphers should be treated as compatibility modes with explicit containment.

## Dependencies

Phase 22 should follow Phase 21. Do not implement SSR on top of the old non-standard Shadowsocks TCP framing. The modern Shadowsocks codec, URI parser, inbound server model, UDP model, and interop discipline must exist first.

Phase 22 also depends on Phase 18 for pproxy oracle tests and manifest discipline.

## Non-goals

Do not make SSR or legacy stream ciphers enabled by default.

Do not prioritize performance before correctness and containment.

Do not add a broad plugin framework unless pproxy compatibility requires it. Implement narrowly scoped compatibility adapters first.

Do not claim final pproxy parity for SSR unless real pproxy differential tests pass.

## Work items

### 22.1 Behavior capture and feature inventory

Use real pproxy to capture the legacy/SSR behavior surface for the pinned target version.

Inventory:

- accepted SSR URI schemes and aliases;
- pproxy command-line forms for SSR;
- URI grammar for method, password, protocol, protocol parameters, obfs, obfs parameters, host, and port;
- supported legacy Shadowsocks stream ciphers;
- supported SSR ciphers;
- supported SSR protocol modes;
- supported SSR obfs modes;
- OTA behavior;
- TCP behavior;
- UDP behavior;
- authentication failure behavior;
- malformed URI diagnostics;
- unsupported method diagnostics;
- interaction with chains and routing;
- Python API exposure if any.

Persist findings in:

```text
docs/protocols/SHADOWSOCKS_LEGACY.md
docs/protocols/SHADOWSOCKSR.md
docs/PPROXY_PARITY_SPEC.md
tests/compat/fixtures/pproxy_ssr_behavior.md
```

The inventory must distinguish confirmed pproxy behavior from assumptions based on third-party SSR documentation.

### 22.2 Compatibility decision record

Before implementation, add an ADR deciding how legacy modes are exposed.

Suggested path:

```text
docs/adr/ADR_legacy_shadowsocks_ssr_compatibility.md
```

Decision points:

- whether legacy stream ciphers are implemented;
- whether OTA is implemented;
- whether SSR is implemented;
- whether support is compile-time feature gated;
- whether support is runtime gated;
- whether Python compatibility API exposes these modes by default;
- warning text and docs requirements;
- support/deprecation stance.

Recommended policy:

- compile-time feature gate for SSR/legacy crypto;
- runtime `insecure_legacy_compat = true` or equivalent for use;
- visible warning unless explicitly silenced;
- metrics label for legacy mode use;
- no accidental activation through URI parsing alone.

### 22.3 URI grammar and parser support

Implement parser support before runtime support.

Requirements:

- parse pproxy-compatible SSR URIs;
- parse legacy Shadowsocks methods;
- parse method/password/userinfo forms exactly as pproxy does;
- preserve percent-encoding behavior;
- preserve base64 or URL-safe base64 behavior if pproxy uses it;
- emit pproxy-shaped diagnostics for malformed values;
- redact passwords and sensitive plugin parameters;
- classify parsed URI as disabled unless the relevant feature gates are enabled.

Tests:

- golden URI parse tests from pproxy examples;
- malformed URI tests;
- redaction tests;
- disabled-feature diagnostics tests.

### 22.4 Legacy stream cipher support

If the ADR permits implementation, add legacy stream cipher support in an isolated module.

Potential methods to inventory and implement only if pproxy supports them:

- `aes-128-ctr`;
- `aes-192-ctr`;
- `aes-256-ctr`;
- `aes-128-cfb`;
- `aes-192-cfb`;
- `aes-256-cfb`;
- `rc4-md5`;
- any other pproxy-supported methods captured by the oracle.

Requirements:

- strict feature gate;
- no use in default builds unless explicitly enabled;
- clear insecure-mode warning;
- separate codec module from AEAD implementation;
- no silent fallback from AEAD to stream ciphers;
- test vectors or pproxy differential evidence;
- interop test against pproxy for every implemented method.

### 22.5 OTA compatibility

If pproxy's target version supports OTA and the ADR permits implementation, add OTA support.

Requirements:

- capture pproxy OTA packet/stream behavior;
- implement encode/decode in isolated module;
- support only confirmed combinations;
- reject unsupported combinations early;
- add differential tests;
- document security limitations.

If not implemented, add pproxy-compatible diagnostics and classify final status in the manifest.

### 22.6 SSR protocol modes

Implement only confirmed pproxy-supported SSR protocol modes.

Candidate modes from the roadmap inventory:

- `origin`;
- `verify_simple`;
- `verify_deflate`;
- any additional pproxy-confirmed modes.

Requirements:

- clean trait/interface for SSR protocol transforms;
- deterministic framing/verification errors;
- per-mode tests;
- pproxy differential tests;
- explicit unsupported diagnostics for unimplemented modes.

### 22.7 SSR obfs modes

Implement only confirmed pproxy-supported obfs modes.

Candidate modes from the roadmap inventory:

- `plain`;
- `http_simple`;
- `tls1.2_ticket_auth`;
- any additional pproxy-confirmed modes.

Requirements:

- clean transform layer separate from ciphers;
- pproxy-compatible parameter parsing;
- deterministic randomization or test hooks where obfs introduces randomness;
- pproxy differential tests;
- safe failure on invalid parameters.

### 22.8 SSR TCP client/upstream support

Add upstream/client support once parser, cipher, protocol, and obfs layers are ready.

Requirements:

- connect to SSR server;
- apply selected cipher/protocol/obfs transforms;
- encode target address;
- relay bidirectional TCP;
- support domain, IPv4, and IPv6 targets according to pproxy behavior;
- integrate with TCP chain executor;
- expose precise capability validation.

Tests:

- Eggress SSR upstream to pproxy SSR listener;
- pproxy SSR client to Eggress SSR listener after server support;
- direct TCP echo through SSR;
- wrong password;
- wrong protocol params;
- unsupported obfs.

### 22.9 SSR TCP inbound server support

Add inbound listener/server support if pproxy parity requires it.

Requirements:

- accept SSR client connections;
- decode obfs/protocol/cipher layers;
- extract target address;
- route through Eggress routing engine;
- relay TCP;
- enforce timeouts and bounds;
- report auth/decode failures safely;
- integrate with listener config and pproxy URI syntax.

### 22.10 SSR UDP support

Implement SSR UDP only after TCP behavior is stable and pproxy UDP behavior is captured.

Requirements:

- capture pproxy SSR UDP format;
- support standalone pproxy UDP mode from Phase 20 if applicable;
- support inbound and upstream datagram transforms;
- add anti-amplification controls;
- add per-client and global flow limits;
- add pproxy differential tests.

### 22.11 Security containment

Legacy/SSR support must not weaken default Eggress behavior.

Required controls:

- compile-time feature gate or clearly named optional dependency feature;
- runtime opt-in;
- structured warning on startup;
- metrics counter for legacy mode activation;
- docs stating security status;
- tests proving disabled mode rejects legacy URIs;
- tests proving enabled mode is explicit;
- no downgrade from AEAD to legacy based only on remote failure.

### 22.12 Python compatibility implications

Record which legacy/SSR features are visible through pproxy's Python API.

Requirements:

- update Python compatibility inventory if Phase 29 has already started;
- ensure disabled legacy features produce pproxy-shaped exceptions in compatibility mode;
- ensure Eggress-native Python API remains explicit about insecure legacy enablement;
- add redaction tests for SSR/legacy URI strings.

### 22.13 Documentation updates

Update:

- `docs/protocols/SHADOWSOCKS_LEGACY.md`;
- `docs/protocols/SHADOWSOCKSR.md`;
- `docs/SECURITY_REVIEW.md`;
- `docs/PARITY_MATRIX.md`;
- `docs/PPROXY_PARITY_SPEC.md`;
- `docs/PPROXY_MIGRATION.md`;
- `docs/CONFIG_REFERENCE.md`;
- README capability table;
- compatibility manifest.

Docs must clearly state whether SSR/legacy support is enabled by default, feature-gated, or intentionally unsupported.

## Validation commands

At minimum:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p eggress-protocol-shadowsocks legacy
cargo test -p eggress-protocol-shadowsocks ssr
cargo test --test differential_pproxy -- ssr --nocapture
cargo test --test differential_pproxy -- legacy_shadowsocks --nocapture
```

If SSR is feature-gated:

```bash
cargo test --workspace --features legacy-shadowsocks
cargo test --workspace --features ssr-compat
```

Use the exact feature names chosen by implementation.

## Acceptance criteria

Phase 22 is complete when:

- pproxy's legacy Shadowsocks and SSR behavior has been captured;
- an ADR records the compatibility/security decision;
- parser support exists for confirmed pproxy URI forms;
- disabled legacy/SSR modes fail with clear pproxy-compatible diagnostics;
- any implemented legacy cipher, OTA, protocol, or obfs mode has pproxy differential evidence;
- default Eggress remains modern-AEAD-only unless the user explicitly opts into legacy compatibility;
- docs and manifest classify every legacy/SSR item accurately.

## Risks

SSR and legacy Shadowsocks are maintenance-heavy and security-sensitive. The largest risk is accidentally creating a broad crypto compatibility layer that becomes default attack surface. Gate aggressively.

Some pproxy legacy behavior may rely on old dependencies or ambiguous protocol conventions. Capture observed behavior before implementing, and prefer explicit final non-parity over speculative compatibility.

Randomized obfs behavior can make differential testing flaky. Add deterministic test hooks where possible while preserving production randomness.

## Handoff notes

This phase is where project positioning must be especially disciplined. Supporting SSR for pproxy parity does not mean recommending SSR. Treat it as a legacy compatibility surface, not as part of the modern Eggress default path.
