# Phase 39: pproxy URI grammar and chain semantics

## Goal

Close pproxy URI grammar and chain-semantics gaps so existing pproxy command lines can be accepted and translated without requiring users to rewrite URI forms into eggress-native syntax.

The highest-priority gap is raw pproxy `__` jump-chain syntax. The compat translator currently detects `__` in remote URIs and marks it unsupported, even though eggress has a native chain representation and supports three-or-more-hop TCP chains. This phase should accept supported `__` chains, validate each hop, and compile them into the native chain model.

## Current context

Relevant implementation areas:

- `crates/eggress-pproxy-compat/src/uri.rs` parses pproxy URI forms.
- `crates/eggress-pproxy-compat/src/args.rs` assigns `-l`, `-r`, `-ul`, `-ur`, and positional arguments.
- `crates/eggress-pproxy-compat/src/translate.rs` translates parsed local/remote URIs into TOML.
- Current translation pre-filters remote URIs containing `__` and reports unsupported `backward-jump-chain`.
- Reverse/backward URI translation exists for some `+in`, `bind`, `listen`, `backward`, and `rebind` forms.
- Runtime support is stronger for TCP chains than for UDP/reverse complex chains.

## Primary deliverables

- Full pproxy URI grammar fixture suite.
- `__` chain parsing into a structured pproxy chain type.
- Translation from supported pproxy chain syntax to native eggress upstream chains.
- Structured diagnostics for unsupported chain combinations.
- Parity manifest updates for each URI grammar surface.
- Updated CLI inventory and migration docs.

## URI surfaces to cover

### Scheme aliases and protocol mapping

Cover these listener/upstream forms:

- `http://`
- `https://`
- `socks4://`
- `socks4a://`
- `socks5://`
- `ss://`
- `shadowsocks://`
- `ssr://`
- `trojan://`
- `ssh://`
- `direct://`
- `redir://`
- `unix://`
- `bind://`
- `listen://`
- `backward://`
- `rebind://`
- `raw://`
- `tunnel://`
- `ws://`
- `wss://`
- `h2://`

Do not mark all of these runtime-supported. The goal is correct parsing, classification, translation if supported, and refusal if unsupported.

### Modifiers

Support or explicitly classify:

- `+ssl`
- `+tls`
- `+in`
- repeated `+in`
- combinations such as `socks5+ssl://`, `socks5+tls://`, `socks5+in://`, and `socks5+in+in://`

Use pproxy behavior as the source of truth. If pproxy treats `+ssl` and `+tls` differently, preserve the distinction. If eggress collapses them into one TLS transport, report a compatible-with-warning tier where needed.

### Address forms

Cover:

- omitted host, such as `socks5://:1080`
- explicit IPv4
- bracketed IPv6
- domain names
- default port inference
- path-style Unix sockets
- URI paths where pproxy uses them
- query strings, especially rule parameters
- percent-encoded credentials
- empty usernames/passwords
- password-only Trojan userinfo
- Shadowsocks method/password userinfo

### Chain syntax

Support:

- `-r http://hop1:8080__socks5://hop2:1080`
- three-or-more-hop chains
- credentialed hops inside chains
- TLS modifiers inside chains
- Shadowsocks as a hop if supported by runtime
- direct terminal behavior where pproxy allows it

Reject with precise diagnostics:

- chain containing SSH until SSH is implemented
- chain containing SSR
- chain containing runtime-refused WebSocket/raw/H2 unless promoted in a later phase
- UDP multi-hop chains until the UDP phase completes them
- reverse/backward chains where jump-chain composition is not implemented

## Suggested internal model changes

Add an intermediate representation if current `PproxyUri` is too single-hop oriented:

```rust
pub struct PproxyChain {
    pub raw: String,
    pub hops: Vec<PproxyUri>,
    pub transport: PproxyTransportKind,
}
```

or equivalent. Avoid encoding multi-hop chains as strings until final TOML generation. The translator should validate a structured chain before generating output.

## Translation rules

### TCP remotes

A pproxy remote chain should become one native upstream entry whose `uri` or chain field can represent all hops. If the current TOML model only stores one URI per upstream, inspect whether native multi-hop chain support is represented elsewhere. Do not fake a chain by creating multiple independent upstreams in a round-robin group.

Expected behavior:

- one pproxy `-r` with `__` means one ordered chain
- multiple pproxy `-r` arguments mean multiple route choices/group members
- each chain member preserves credentials, TLS modifiers, and scheme normalization

### UDP remotes

For `-ur` values containing `__`, do not silently translate to a partial first hop. If UDP multi-hop is not implemented, emit `unsupported` or `compatible_with_warning` according to the manifest rules.

### Reverse/backward forms

Do not overclaim. Basic reverse server/client mapping may exist, but jump-chain composition on relayed streams remains a known gap. Chain syntax combined with reverse/backward forms should be either fully implemented or rejected with a stable diagnostic.

## Diagnostics

Add stable diagnostic codes for:

- unsupported scheme in chain
- unsupported modifier
- unsupported chain transport
- unsupported UDP multi-hop
- unsupported reverse chain composition
- malformed chain segment
- empty chain segment
- credential parse failure
- invalid IPv6/address form
- runtime-refused protocol in URI

Diagnostics must include the redacted URI/segment and a migration suggestion when possible.

## Tests

### Parser unit tests

Add fixtures for:

- simple single-hop URI
- every supported scheme alias
- every refused scheme
- IPv6 listener and upstream forms
- percent-encoded credentials
- password-only Trojan
- Shadowsocks method/password
- `+ssl`, `+tls`, `+in`, repeated `+in`
- `__` two-hop and three-hop chains
- malformed chains: leading, trailing, and doubled separators

### Translator tests

Add tests for:

- one `-r` chain becomes one ordered upstream chain
- two `-r` chains become two upstream group members, not one flattened chain
- unsupported SSH chain hop reports `ssh-upstream` or chain-specific SSH diagnostic
- SSR chain hop reports SSR diagnostic
- UDP chain reports unsupported until implemented
- reverse/backward chain reports unsupported until implemented

### Config compile tests

For every translated supported chain fixture, compile the generated TOML. This is required; parser-only support is not enough.

### Runtime smoke tests

Where the testkit supports it, create HTTP/SOCKS chain smoke tests that perform real CONNECT through two hops. If this already exists elsewhere, wire it into the parity manifest evidence.

## Documentation updates

Update:

- `docs/cli/PPROXY_CLI_INVENTORY.md`
- `docs/parity/PPROXY_PARITY_REPORT.md`
- compatibility/migration docs if present
- README only if it currently overstates `__` chain support

Document the important distinction:

- pproxy `remote1__remote2` is an ordered chain.
- multiple `-r` entries are route choices/group members.

## Acceptance criteria

- `__` TCP chains containing supported hops are no longer rejected.
- Chain translation preserves ordered semantics and does not flatten chains into load-balancing members.
- Unsupported chain combinations fail with stable structured diagnostics.
- URI parser fixtures cover all pproxy scheme aliases and modifiers tracked in the parity manifest.
- Generated TOML for supported chain fixtures compiles.
- `eggress pproxy check --json` reports chain features with manifest-aligned IDs and tiers.
- Docs no longer instruct users to rewrite simple `__` chains into separate `-r` flags when an ordered chain is intended.

## Verification commands

Run at minimum:

```bash
cargo fmt --all -- --check
cargo test -p eggress-pproxy-compat
cargo test -p eggress-uri
cargo test -p eggress-config
cargo test --workspace
```

Also run manifest validation from Phase 37 if present.

## Non-goals

- Do not implement SSH transport in this phase.
- Do not implement SSR.
- Do not implement QUIC/H3.
- Do not implement UDP multi-hop unless it is trivial and fully testable; classify it instead.
- Do not promote WebSocket/raw/H2 runtime support unless Phase 44 work is explicitly pulled forward.

## Handoff notes

The main correctness risk is confusing an ordered chain with a group of alternatives. Preserve pproxy's chain-vs-group semantics exactly, or reject with diagnostics rather than generating a misleading config.
