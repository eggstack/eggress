# OutboundConnector pproxy Chain Corrective Pass

## Status

**READY FOR IMPLEMENTATION**

## Parent context

- `plans/PPROXY_PRACTICAL_PARITY_ROADMAP.md`
- `plans/PPROXY_PARITY_NARROW_CORRECTIVE_PASS.md`
- `docs/EMBED_API.md`

This is a narrow follow-on correction for the native Rust embedding surface. It does **not** reopen the completed practical-parity roadmap or authorize broader pproxy compatibility work.

## Baseline

Written against repository head:

```text
38636f8df3cb26a1a0d13975347639431fb4f2b4
```

## Motivation

`eggress-embed::OutboundConnector` is the intended in-process egress primitive for Rust applications that need Eggress proxy semantics without starting a listener service. A downstream consumer such as EggPool can therefore retain its existing pproxy-style proxy configuration while replacing Python `pproxy.Connection(...).tcp_connect(...)` with a native Rust connector.

The current constructor is correct for a single hop but is incorrectly wired for canonical pproxy multi-hop expressions.

Today `OutboundConnector::from_pproxy_uri()` does this conceptually:

```rust
let parsed = parse_pproxy_uri(uri)?;
let chain = PproxyChain {
    raw: uri.to_string(),
    hops: vec![parsed],
};
translate_from_uris(..., &[chain])
```

`parse_pproxy_uri()` intentionally parses only the first hop when the source contains pproxy's `__` chain separator. The compatibility crate already has the correct full-expression parser, `parse_pproxy_chain()`, and the translator already accepts `PproxyChain` values. As a result, the embed constructor currently discards every hop after the first rather than passing the complete chain into the existing runtime.

Example affected input:

```text
socks5://proxy-a:1080__http://proxy-b:8080
```

Expected compiled runtime chain:

```text
hop 1: socks5://proxy-a:1080
hop 2: http://proxy-b:8080
```

Current embed-constructor behavior can collapse this to only hop 1.

This is an adapter-boundary defect, not a missing proxy-engine capability.

## Objective

Make `OutboundConnector::from_pproxy_uri()` consume one complete pproxy remote expression, including canonical `__` multi-hop chains, while preserving:

- existing single-hop behavior;
- existing `direct://` behavior;
- pproxy-compatible URI parsing and translation semantics;
- fail-closed handling of unsupported chain members;
- credential redaction in embed-API errors;
- current feature boundaries;
- the listener-free native outbound execution model.

The corrected path should be:

```text
pproxy remote expression
  -> parse_pproxy_chain()
  -> compatibility validation/translation
  -> compiled Eggress chain
  -> OutboundConnector
  -> ChainExecutor::execute()
```

No loopback listener, subprocess, compatibility daemon, or second proxy implementation should be introduced.

---

# Scope guardrails

## In scope

- `OutboundConnector::from_pproxy_uri()` full-chain parsing;
- preservation of the single-hop direct fast path;
- fail-closed validation of chain forms that the native outbound path cannot execute;
- safe handling of compatibility translation results;
- credential-safe diagnostics for malformed chained expressions;
- focused embed-level regression tests;
- concise embed documentation for multi-hop URI expressions.

## Out of scope

Do not use this correction to add or redesign:

- new proxy protocols;
- SSH support when the `ssh` feature is disabled;
- QUIC/H3 support when its features are disabled;
- ShadowsocksR or legacy cipher/plugin expansion;
- general multi-hop UDP;
- listener/runtime service behavior;
- Python compatibility APIs;
- routing schedulers or account/provider routing;
- a new URI grammar;
- a second chain parser;
- a new public connector type;
- a loopback proxy used merely to adapt another HTTP client;
- EggPool-specific HTTP, retry, or provider behavior;
- broad CI matrices, external-service tests, or a new compatibility harness.

The existing compatibility parser, translator, config compiler, and chain executor remain the source of truth.

## Engineering rule

Fix the narrowest complete seam. If the implementation starts duplicating protocol parsing or chain execution inside `eggress-embed`, stop and route the data through the existing compatibility/runtime layers instead.

---

# Confirmed defect

## Current constructor loses pproxy `__` hops

Primary file:

- `crates/eggress-embed/src/outbound.rs`

Current behavior:

1. `from_pproxy_uri(uri)` calls `eggress_pproxy_compat::uri::parse_pproxy_uri(uri)`.
2. The single-URI parser deliberately selects only the first hop when `__` occurs because callers needing the complete expression are expected to use `parse_pproxy_chain()`.
3. `from_pproxy_uri()` then wraps that one parsed hop in a new `PproxyChain`.
4. `translate_from_uris()` therefore receives a one-hop chain even when the original input contained two or more hops.
5. `connect_tcp()` executes the compiled chain correctly, but the missing hops are already gone before compilation.

The compatibility translator itself already uses `parse_pproxy_chain()` for normal remote CLI arguments. The embed constructor should consume the same typed representation instead of reconstructing a narrower one.

---

# Workstream 1 — Wire the full pproxy chain into OutboundConnector

## Goal

Make the embed constructor preserve every hop in a canonical pproxy remote expression.

## Primary files

- `crates/eggress-embed/src/outbound.rs`
- `crates/eggress-pproxy-compat/src/uri.rs` only if a tiny reusable helper is genuinely required
- `crates/eggress-pproxy-compat/src/lib.rs` only if an existing helper needs a public re-export

Prefer an `outbound.rs`-only implementation change. Do not change the parser grammar unless a focused regression proves the existing chain parser itself is wrong.

## Required changes

1. Replace the single-hop parse-and-wrap sequence in `OutboundConnector::from_pproxy_uri()` with the existing full-expression parser:

   ```rust
   let chain = eggress_pproxy_compat::uri::parse_pproxy_chain(uri)?;
   ```

2. Do not manually construct a `PproxyChain` from one `PproxyUri` after parsing.

3. Preserve the direct connector fast path only for an expression representing exactly one direct hop.

   Conceptually:

   ```rust
   if chain.hops.len() == 1 && chain.hops[0].scheme == "direct" {
       // existing direct connector path
   }
   ```

4. Do not silently interpret `direct` in a multi-hop expression as the single-hop direct fast path. A multi-hop form containing a chain member unsupported by Eggress chaining must fail according to the existing compatibility rules.

5. Pass the complete `PproxyChain` into `translate_from_uris()` without reconstructing or flattening it.

6. Preserve hop order exactly. No sorting, scheduler conversion, deduplication, or canonical reordering belongs in this constructor.

7. Preserve every existing per-hop field carried by `PproxyUri`, including where applicable:

   - credentials;
   - TLS/SSL modifiers;
   - protocol composition;
   - local bind;
   - rules;
   - plugin metadata;
   - fixed target;
   - fragment authentication.

8. Do not add a parallel parser based on `eggress_uri::parse_proxy_chain()`. `from_pproxy_uri()` is explicitly a pproxy-compatibility entry point and must continue using `eggress-pproxy-compat` as the syntax authority.

## Acceptance criteria

- `socks5://proxy-a:1080__http://proxy-b:8080` produces a compiled outbound chain with exactly two hops.
- Hop order is SOCKS5 then HTTP.
- A three-hop supported expression produces exactly three compiled hops in source order.
- Existing single-hop SOCKS/HTTP construction remains unchanged.
- `direct://` retains the current zero-proxy direct execution path.
- A multi-hop expression containing `direct` is not collapsed into direct mode.
- No new listener or runtime service is started by the constructor.

---

# Workstream 2 — Make unsupported/partial translation fail closed

## Goal

Ensure the embed constructor never reports successful construction after compatibility translation says some requested chain behavior is unsupported.

## Rationale

The CLI translation path performs chain validation before execution and exposes structured unsupported-feature output. `OutboundConnector::from_pproxy_uri()` currently calls `translate_from_uris()` directly and then consumes only the generated TOML. This means the embed seam should explicitly honor compatibility rejection/unsupported state rather than relying on accidental TOML compilation failure.

This matters more once complete multi-hop expressions reach the constructor because each additional hop can carry a protocol or role that is valid pproxy syntax but unavailable in the selected Eggress feature set or chaining role.

## Required changes

1. Run the existing chain-hop validation appropriate to pproxy remotes before compiling the connector.

2. If `validate_chain_hops(&chain)` reports unsupported chain members, return a stable Eggress error instead of deleting, bypassing, or approximating those hops.

3. After `translate_from_uris()`, inspect its `TranslationOutput`.

4. If the output contains an unsupported condition that prevents faithful execution of the requested remote chain, fail connector creation. Do not call `Self::from_toml()` and silently proceed with a partial translation.

5. Compatibility warnings that do not alter executable semantics may remain warnings/internal translation metadata; do not turn every warning into a hard failure.

6. Use the existing `EggressError` categories. Do not create a new public error hierarchy for this pass. Prefer:

   - `EggressError::UnsupportedFeature` for a known unsupported capability; or
   - `EggressError::Config` for malformed/invalid compatibility input.

7. Keep errors deterministic enough that downstream applications can classify connector construction failure without string-matching protocol implementation details.

## Acceptance criteria

- A supported HTTP/SOCKS chain constructs successfully.
- A chain containing a known unsupported chaining role does not construct a partial connector.
- Feature-disabled protocols fail at construction rather than becoming a shorter/different route.
- Ordinary compatibility warnings do not unnecessarily reject otherwise executable supported chains.
- No unsupported hop is silently dropped.

---

# Workstream 3 — Preserve credential-safe diagnostics

## Goal

Fix chain parsing without creating an embed-API credential leak on malformed expressions.

## Confirmed risk

`parse_pproxy_chain()` correctly reports structural errors such as leading/trailing separators and unmatched delimiters, but some compatibility error messages include the source expression for diagnostics. `EggressError` documents that its stored messages have credentials redacted. Mapping a compatibility error directly with `e.to_string()` can violate that contract when the malformed source contains userinfo.

The recent repository hardening also consistently routes URI diagnostics through redaction helpers. The corrected constructor should follow the same rule.

## Primary files

- `crates/eggress-embed/src/outbound.rs`
- `crates/eggress-embed/tests/error_redaction.rs`
- optionally a small existing redaction helper location if reuse is cleaner than a local adapter

## Required changes

1. Do not include the original unredacted `uri` in an `EggressError` returned by `from_pproxy_uri()`.

2. When compatibility parser/validator errors include the source URI, sanitize the diagnostic before storing it in `EggressError`.

3. Reuse an existing URI redaction primitive where it safely covers pproxy chain input. Do not build a generalized secret-scanning subsystem.

4. If a tiny pproxy-expression redaction helper is needed, keep it syntax-local and deterministic. It should redact credentials independently of whether the full expression parses successfully.

5. Ensure malformed chains with credentials in more than one hop do not expose any username or password through `Display` or `Debug` of the returned embed error.

## Required regression input

At minimum exercise a malformed credentialed chain such as:

```text
socks5://user_a:secret_a@127.0.0.1:1080__http://user_b:secret_b@127.0.0.1:8080__
```

The resulting error must contain none of:

```text
user_a
secret_a
user_b
secret_b
```

The exact redacted rendering is not part of the public compatibility contract; absence of secrets is.

## Acceptance criteria

- malformed chained input remains diagnosable;
- no plaintext username/password from any hop appears in the embed error;
- valid URI credentials still reach runtime configuration internally;
- no global logging/redaction subsystem is introduced.

---

# Workstream 4 — Focused regression tests

## Goal

Protect the corrected adapter seam without duplicating Eggress protocol-engine tests.

## Primary files

- unit tests in `crates/eggress-embed/src/outbound.rs`
- `crates/eggress-embed/tests/error_redaction.rs`

Use module-local tests where private compiled state needs inspection. Do not add public introspection APIs merely to test hop count.

## Required tests

### 1. Two-hop constructor preservation

Construct:

```text
socks5://127.0.0.1:1080__http://127.0.0.1:8080
```

Inspect the connector's private compiled runtime configuration from the `outbound.rs` test module and assert:

- one upstream exists;
- the upstream chain has exactly two hops;
- hop 0 is SOCKS5;
- hop 1 is HTTP.

This is the core regression test. It must fail on the pre-fix implementation.

### 2. Three-hop preservation

Use three supported hop forms and assert compiled chain length and source order. This protects against a future `first()`/single-hop regression without requiring network traffic.

### 3. Single-hop regression

Keep or extend the existing single-hop `from_pproxy_uri()` test to prove the change does not break ordinary construction.

### 4. Direct regression

Assert a single direct expression still produces the direct connector path and does not manufacture an upstream chain.

### 5. Malformed chain rejection

At minimum cover:

```text
socks5://127.0.0.1:1080__
__socks5://127.0.0.1:1080
```

Both must fail connector construction.

### 6. Unsupported-chain fail-closed behavior

Use one pproxy-valid chain member that the selected feature set/runtime chaining contract does not support and assert construction fails rather than producing a shorter chain.

Do not make the test depend on an external proxy or Internet service.

### 7. Credential redaction

Add the malformed two-credential chain regression described in Workstream 3.

## Runtime-test policy

Do **not** add a new two-proxy integration harness solely for this correction. The defect is the constructor's typed-chain wiring, and Eggress already separately tests chain parsing, translation, compilation, and execution. A compiled-chain assertion at the embed seam is sufficient to prove this specific regression.

Add a live multi-hop test only if implementation review discovers that the compiled chain is correct but `OutboundConnector::connect_tcp()` treats embed-created chains differently from ordinary compiled chains. There is no current evidence of such a second defect.

---

# Workstream 5 — Document the corrected embed contract

## Goal

Make downstream Rust consumers aware that the native outbound connector accepts pproxy chain expressions directly and does not require a listener.

## Primary files

- `docs/EMBED_API.md`
- `crates/eggress-embed/README.md`

## Required changes

Add a concise `OutboundConnector` example such as:

```rust
let connector = OutboundConnector::from_pproxy_uri(
    "socks5://127.0.0.1:1080__http://127.0.0.1:8080"
)?;

let (stream, info) = connector.connect_tcp("api.example.com", 443).await?;
assert_eq!(info.hop_count, 2);
```

Documentation must state:

- `from_pproxy_uri()` accepts a pproxy remote expression, including `__` chains;
- the connector executes the chain in process;
- it does not start a local listener;
- protocol availability remains feature-gated;
- unsupported chain semantics fail rather than being silently omitted.

Do not document EggPool-specific behavior in Eggress.

---

# Implementation order

Implement in this order:

```text
1. Add failing two-hop embed regression
2. Replace single-hop parser/wrapper with parse_pproxy_chain()
3. Preserve one-hop direct fast path
4. Enforce fail-closed unsupported-chain handling
5. Add malformed-chain and redaction regressions
6. Add three-hop/single-hop/direct regression coverage
7. Update embed documentation
8. Run focused verification
```

The implementation should remain a small adapter correction. If the diff begins touching protocol engines, listener infrastructure, scheduler code, or broad compatibility manifests, reassess before proceeding.

---

# Verification

Keep verification proportional to the files changed.

Required:

```bash
cargo fmt --check
cargo test -p eggress-embed
cargo test -p eggress-pproxy-compat
```

If feature-specific compilation changes are made, also run the narrow lean path:

```bash
cargo check -p eggress-embed \
  --no-default-features \
  --features common,pproxy-compat
```

Run wider workspace tests only if shared config/runtime/protocol code is modified. This plan should not require that.

No external `pproxy` installation, Internet proxy, hosted service, load suite, or new CI job is required for closure.

---

# Explicit acceptance matrix

| Case | Expected result |
|---|---|
| Single SOCKS5 URI | Connector compiles one SOCKS5 hop |
| Single HTTP URI | Connector compiles one HTTP hop |
| SOCKS5 `__` HTTP | Connector compiles two ordered hops |
| Three supported hops | Connector compiles all three in source order |
| Single `direct://` | Direct connector path, zero proxy hops |
| Multi-hop containing unsupported `direct` role | Construction fails; chain is not collapsed |
| Unsupported/feature-disabled chain member | Construction fails closed |
| Trailing `__` | Deterministic config/compatibility error |
| Leading `__` | Deterministic config/compatibility error |
| Malformed credentialed chain | Error contains no plaintext credentials |
| Valid credentialed multi-hop chain | Credentials remain available internally for connection setup |
| `connect_tcp()` on compiled chain | Uses existing ChainExecutor path |
| Constructor | Starts no listener/service/process |

---

# Definition of done

This corrective pass is complete only when all of the following are true:

1. `OutboundConnector::from_pproxy_uri()` parses the full pproxy remote expression with the existing chain parser.
2. It no longer manually converts a single `PproxyUri` into a one-hop `PproxyChain`.
3. Supported `__` expressions preserve every hop and source order through compiled runtime configuration.
4. Single-hop direct behavior remains intact.
5. Unsupported chain semantics fail closed and no hop is silently dropped.
6. Malformed chained input cannot leak credentials through `EggressError`.
7. Focused regression tests prove two-hop, three-hop, single-hop, direct, malformed, unsupported, and redaction behavior.
8. Embed documentation states that pproxy chain expressions are accepted directly and executed without a listener.
9. No new protocol implementation, compatibility framework, listener architecture, or CI matrix has been introduced.
10. The focused verification commands pass.

## Handoff note

The intended implementation is deliberately small. The existing pproxy compatibility layer already owns chain grammar and translation, and the native ChainExecutor already owns execution. The correction should primarily remove the lossy single-hop adapter logic between those two working components.