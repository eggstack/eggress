# pproxy Maintenance Phase 2 — URI and Protocol Metadata Convergence

## Status

**PLANNED**

## Parent roadmap

[`PPROXY_MAINTENANCE_CONVERGENCE_ROADMAP.md`](PPROXY_MAINTENANCE_CONVERGENCE_ROADMAP.md)

## Dependency

Phase 1 must be complete first so the current compatibility contract is truthful before parser internals move.

## Objective

Reduce duplicated URI lexical logic and protocol-token classification between `eggress-uri` and `eggress-pproxy-compat` without collapsing the native and pproxy grammars into one over-generalized parser.

The target architecture is:

```text
shared URI lexical / endpoint primitives
        |
        +--> native Eggress grammar -> ProxyChainSpec / ProxyHopSpec
        |
        +--> pproxy compatibility grammar -> PproxyUri / compatibility-only metadata
                                            -> native lowering
```

The native AST remains native. pproxy-only constructs remain explicit compatibility concepts.

## Confirmed overlap

At the planning baseline:

- `crates/eggress-uri/src/lib.rs` contains a large native chain parser with hop splitting, scheme/protocol parsing, endpoint handling, userinfo, local bind handling, IPv6 handling, redaction, and `ProtocolSpec`.
- `crates/eggress-pproxy-compat/src/uri.rs` contains a second substantial parser with its own delimiter scanning, endpoint/userinfo parsing, protocol/modifier classification, redacted rendering, chain splitting, plugin/auth-fragment handling, and pproxy-only reverse semantics.
- The duplicate parser is partly necessary because pproxy syntax includes constructs that do not belong in the native grammar: `+in`, repeated inbound count, `bind://`, `listen://`, `backward://`, `rebind://`, plugin suffixes, auth fragments, rule-file forms, and compatibility aliases.
- Protocol names are also represented separately by `eggress_uri::ProtocolSpec`, `eggress_core::ProtocolId`, and compatibility token strings.

The defect is therefore not "two parsers exist." The defect is that identical lexical/endpoint rules and overlapping native protocol-name recognition are implemented independently.

## Governing constraints

1. Preserve all current public URI behavior unless a focused differential test demonstrates a bug.
2. Do not turn `eggress-uri` into a parser for pproxy reverse/plugin semantics.
3. Do not force pproxy pseudo-protocols such as `bind`, `listen`, `backward`, or `rebind` into `ProtocolSpec` or `ProtocolId` merely to reduce match arms.
4. Do not introduce a dynamic protocol registry, trait-object parser registry, proc macro, code generator, or build script.
5. Do not add a new crate. Shared primitives belong in `eggress-uri` if they are genuinely URI syntax concerns.
6. Preserve credential redaction guarantees.
7. Preserve exact IPv6 bracket behavior, chain separator behavior, and fail-closed malformed-input handling.
8. Preserve pproxy compatibility diagnostics and stable error categories where they are public/tested.
9. Preserve current chain lowering and runtime semantics; this phase is parser/metadata convergence, not proxy execution redesign.
10. Keep the dependency direction `eggress-pproxy-compat -> eggress-uri`; do not introduce a reverse dependency.

## Workstream 1 — Inventory duplicated lexical primitives

Before changing code, enumerate and compare the following behavior in both parsers:

- splitting `__` chain hops outside bracketed IPv6;
- scanning delimiters outside IPv6 brackets;
- parsing `scheme://authority` boundaries;
- parsing bracketed IPv6 host/port;
- parsing unbracketed host/port;
- parsing userinfo / locating the credential `@` outside brackets;
- formatting IPv6 hosts back into URI form;
- redacting userinfo without leaking passwords;
- validating empty host / empty token / duplicate separator cases;
- parsing local bind syntax where semantics are actually shared;
- normalizing native protocol aliases.

Create a small source-level inventory in the implementation/PR summary. Do not add another permanent inventory document unless it is needed in `architecture/uri.md` to explain ownership.

## Workstream 2 — Extract shared lexical and endpoint helpers into `eggress-uri`

### Preferred ownership

Add a narrow public or `#[doc(hidden)]` reusable helper module in `eggress-uri` for syntax operations that are equally valid for both grammars. Suitable responsibilities include:

```text
uri::syntax
  split_top_level(...)
  split_chain_hops(...)
  find_userinfo_separator(...)
  parse_authority(...)
  format_host(...)
  redact_userinfo(...)
```

Exact names may differ. The interface should return neutral lexical/endpoint structures, not `ProxyChainSpec`, so the compatibility parser can apply its own semantics afterward.

A reasonable neutral authority result is conceptually:

```rust
struct ParsedAuthority {
    username: Option<String>,
    password: Option<String>,
    host: String,
    port: Option<u16>,
}
```

but do not make this public API unless external consumers genuinely need it. Prefer the narrowest visibility that still allows the compatibility crate to reuse it; a small intentionally public syntax helper surface is acceptable if cross-crate reuse requires it.

### Required properties

- bracketed IPv6 parsing is implemented once;
- `@` detection cannot confuse IPv6 or later syntax with userinfo;
- redaction uses one tolerant credential-hiding primitive for both native and compatibility diagnostics;
- malformed unmatched brackets fail deterministically;
- duplicate/empty `__` hop cases retain current behavior;
- helpers do not know about routing rules, reverse semantics, plugins, TLS policy, or runtime protocols.

### Do not over-extract

If local-bind syntax differs materially between native and pproxy grammars, keep the semantic interpretation separate and share only the neutral delimiter/authority handling. Do not force identical helper use where it obscures differing rules.

## Workstream 3 — Make native protocol recognition canonical

`ProtocolSpec` should own canonical names and native aliases for URI-level protocols.

Preferred shape:

```rust
impl ProtocolSpec {
    pub fn canonical_name(self) -> &'static str;
    pub fn parse_name(name: &str) -> Option<Self>;
}
```

or an equivalent `FromStr`/`Display` pairing if that fits current APIs better.

Requirements:

1. One native recognition path handles `http`, `httponly`, SOCKS variants, Shadowsocks/SSR, Trojan, H2/H3, QUIC, WebSocket, raw/tunnel aliasing, SSH, Unix, and any other protocol already represented by `ProtocolSpec`.
2. Alias behavior must be explicit and tested; e.g. if `ss` and `shadowsocks` map to the same native syntax type, tests should state that.
3. Compatibility parsing should call the native recognition path for native-capable tokens rather than maintain an independent complete whitelist.
4. The compatibility parser should retain an explicit small table/match for compatibility-only pseudo-protocols/modifiers and any pproxy token whose semantics intentionally differ from native parsing.
5. Do not add pproxy-only reverse words to the native enum solely for exhaustiveness.

## Workstream 4 — Make syntax-to-runtime conversion exhaustive and obvious

The distinction between `ProtocolSpec` and `ProtocolId` is useful: not every syntax concept is a directly dispatchable runtime handler. Preserve that distinction.

Audit every conversion site from URI/config protocol representation to runtime `ProtocolId`.

Required result:

- mappings are centralized into the smallest practical number of conversion functions;
- conversions use exhaustive enum matches rather than stringly typed fallthrough;
- unsupported/no-runtime-role variants fail explicitly;
- `HttpOnly`, Unix transport, SSH transport, raw/tunnel aliases, H3/QUIC, and compatibility pseudo-protocols have clear documented ownership rather than being silently dropped;
- tests enumerate all current `ProtocolSpec` variants and verify their runtime disposition.

Do not move transport concepts into `ProtocolId` unless runtime dispatch truly needs them. The objective is explicit conversion, not a single universal enum.

## Workstream 5 — Reduce `eggress-pproxy-compat/src/uri.rs` after canonical helpers exist

Only after shared helpers and protocol recognition are stable:

1. Replace duplicate bracket/delimiter/userinfo/host formatting/redaction code with shared helpers.
2. Retain pproxy-specific AST fields and parsing stages for:
   - `+in` and repeated backward count;
   - TLS/SSL/secure modifiers where pproxy semantics differ;
   - reverse/bind/listen/backward/rebind pseudo-schemes;
   - rule/rules-file/rule-suffix forms;
   - fixed target syntax;
   - plugin metadata;
   - auth fragments;
   - legacy `;` compatibility behavior if still required.
3. Split the compatibility URI module internally only if a clean responsibility boundary emerges, for example `lex.rs`, `parse.rs`, `model.rs`, `display.rs`; do not split solely to reduce line count.
4. Preserve `PproxyUri` public behavior and existing redacted display semantics unless a test proves current behavior wrong.

## Workstream 6 — Add cross-parser equivalence tests for shared syntax

Create deterministic table-driven tests covering syntax both grammars intentionally share.

Minimum corpus:

- IPv4 endpoint;
- DNS hostname endpoint;
- bracketed IPv6 endpoint;
- username/password userinfo;
- empty listener host where allowed by the owning grammar;
- `__` two-hop chain;
- three-hop chain;
- TLS/native modifiers that both grammars intentionally normalize the same way;
- aliases such as `ss`/`shadowsocks`, `raw`/`tunnel`, `ws`/`wss` as applicable;
- credential redaction;
- password containing percent-encoded delimiter-like bytes if currently supported;
- unmatched `[` / `]`;
- duplicate hop separator;
- malformed/missing port;
- unbracketed IPv6 rejection where required.

The tests should compare neutral shared syntax results or lowered native results, not require the two ASTs to become identical.

Also retain compatibility-only tests proving that `+in`, plugin metadata, fragments, and reverse pseudo-protocols remain accepted/diagnosed exactly as before.

## Workstream 7 — Architecture documentation

Update:

- `architecture/uri.md`
- `architecture/pproxy-compat.md`
- `docs/URI_GRAMMAR.md` if shared syntax ownership changes user-facing grammar descriptions

Document the intentional two-layer design:

```text
shared lexical primitives != shared grammar
```

This distinction should be explicit so a future cleanup does not attempt to merge pproxy-only semantics into the native AST.

## Required verification

Focused:

```bash
cargo test -p eggress-uri
cargo test -p eggress-pproxy-compat
cargo test -p eggress-config
```

Run focused CLI/native equivalence tests that consume parsed chains, including existing tests under `eggress-pproxy-compat/tests/native_equivalence.rs` or their current equivalent.

If compatibility behavior is intended to be unchanged, external pproxy differential tests are optional. If any user-visible URI acceptance/rejection or lowering semantics change, run the relevant ignored oracle tests with the pinned 2.7.9 interpreter.

At closure:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Because URI parsing is fuzz-sensitive, update/check the existing URI/config fuzz targets if helper movement changes their entry points. Do not add an unrelated fuzzing framework.

## Acceptance criteria

Phase 2 is complete only when all are true:

- native and compatibility parsing no longer independently implement bracket-aware endpoint/userinfo parsing where semantics are identical;
- credential redaction for proxy URIs funnels through one tolerant shared primitive or one clearly canonical lower-level implementation;
- native protocol-name/alias recognition has one canonical typed path;
- the pproxy compatibility parser delegates native-capable token recognition to that path and retains only explicit pproxy-specific token/modifier classification;
- `ProtocolSpec` to runtime disposition is exhaustively tested and unsupported/non-dispatchable variants fail explicitly;
- no pproxy-only reverse/plugin concepts were added to the native URI AST merely for deduplication;
- `PproxyUri` still represents pproxy-only metadata explicitly;
- representative native and pproxy-compatible URIs lower to the same native chain/config as before;
- malformed IPv6, userinfo, chain-separator, and credential-redaction behavior is regression-tested;
- no new parser registry, proc macro, build script, crate, or dependency was introduced;
- architecture docs explain the shared-lexing/separate-grammar boundary;
- focused parser/config tests and the broad workspace gate pass.

## Explicit non-goals

Do not change routing semantics, add protocols, add pproxy features, redesign configuration, merge `ProtocolSpec` and `ProtocolId` into one universal enum, or alter public compatibility tiers in this phase unless a parser bug proven against pproxy 2.7.9 requires it.
