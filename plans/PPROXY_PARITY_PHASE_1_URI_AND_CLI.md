# Phase 1 — Exact pproxy URI Grammar and CLI Semantics

## Status

Proposed.

## Parent roadmap

`plans/PPROXY_PRACTICAL_PARITY_ROADMAP.md`

## Depends on

Phase 0 contract reset.

## Objective

Replace the current simplified compatibility URI parser and ad hoc CLI handling with a typed representation of documented pproxy 2.7.9 syntax. Correct option arity, no-argument defaults, mixed listeners, authentication placement, fixed-target forms, and argument ownership before further routing or Python work depends on the parser.

This phase is the central compatibility correction. It should remain confined primarily to `eggress-pproxy-compat`, CLI translation, and focused config generation.

## Current defects to close

The existing parser currently:

- recognizes only a fixed list of singular schemes;
- treats credentials as conventional URL userinfo rather than fully representing pproxy's fragment-auth grammar;
- handles only terminal `+tls`, `+ssl`, and `+in` modifiers;
- does not represent combined listener protocols such as `http+socks4+socks5`;
- does not represent `httponly`;
- does not represent brace-delimited fixed targets;
- does not represent outbound local binding;
- does not represent comma-delimited plugins;
- recognizes only named `?rule=` and `?rules_file=` query parameters instead of canonical pproxy rule suffix forms;
- parses `--pac`, `--get`, and `--test` as valueless flags even though they consume values;
- changes no-argument pproxy behavior from a mixed listener to SOCKS5-only;
- may treat values belonging to those options as positional listeners or remotes.

## In scope

### URI syntax

Represent the documented pproxy URI components without requiring all components to be runtime-supported:

```text
scheme://[cipher-or-userinfo@]netloc[/@localbind][,plugins][?rules][#auth]
```

Also represent:

- combined protocol schemes separated by `+`;
- transport modifiers such as TLS/SSL and reverse `+in`;
- repeated `+in` count;
- `__` chain separation;
- brace-delimited fixed targets used by tunnel-style protocols;
- Unix-domain endpoints;
- IPv4, IPv6, hostnames, omitted hosts, and default ports;
- password-only Trojan credentials;
- Shadowsocks method/password forms;
- pproxy fragment authentication;
- local outbound bind address;
- plugin list as parsed metadata;
- per-URI rule expression or rule-file suffix as parsed metadata.

Unsupported fields must survive parsing and be reported by translation with a precise diagnostic.

### CLI syntax

Match pproxy 2.7.9 argument ownership for the supported compatibility command:

- `-l` / `--listen`;
- `-r` / `--remote`;
- `-ul` / `--udp-listen`;
- `-ur` / `--udp-remote`;
- `-s` scheduler;
- `-a` alive interval;
- `--ssl` certificate/key input;
- `-b` block expression;
- `--pac <path-or-value>`;
- `--get <path,file>`;
- `--test <url-or-target>`;
- `--sys`;
- `--log <path>` where supported by the compatibility command;
- verbosity flags;
- positional listener/remote behavior;
- repeated options;
- no-argument defaults.

Preserve unsupported flags such as daemonization and reuse as explicit diagnostics rather than implementing new subsystems.

## Out of scope

- per-remote routing semantics, which belong to Phase 2;
- wiring H2/WS/raw into runtime config, which belongs to Phase 3;
- Python top-level package compatibility;
- SSH, QUIC/H3, SSR, legacy cipher, or plugin execution;
- generalized URL parsing shared with native Eggress if compatibility quirks would leak into native behavior;
- exhaustive reproduction of argparse help formatting.

## Design

### Typed compatibility AST

Replace fields that conflate syntax and runtime meaning with a compatibility-specific AST. Suggested shape:

```rust
struct PproxyUri {
    protocol_chain: Vec<PproxyProtocolToken>,
    transport_modifiers: Vec<PproxyModifier>,
    reverse_count: u32,
    endpoint: PproxyEndpoint,
    credentials: Option<PproxyCredentials>,
    local_bind: Option<PproxyEndpoint>,
    fixed_target: Option<PproxyEndpoint>,
    plugins: Vec<PproxyPluginSpec>,
    rules: Option<PproxyRuleRef>,
    raw: String,
}
```

The exact names may differ. The important requirement is that parsing does not discard information before the translator decides whether Eggress supports it.

### Parser behavior

- Parse first, validate composition second, translate third.
- Preserve the raw form for diagnostics.
- Keep redaction centralized and cover userinfo, fragment auth, Shadowsocks passwords, Trojan passwords, and plugin secrets.
- Do not reinterpret unknown plugins as URI fragments or paths.
- Do not split on `@`, `?`, `#`, comma, or `__` while inside IPv6 brackets or brace-target syntax.
- Return stable errors for malformed delimiters, missing values, empty chain hops, invalid ports, and impossible combined listener compositions.

### Combined listeners

Map pproxy's default and explicit combined listener schemes to one Eggress listener with multiple protocol detectors when all listed protocols are sniffable.

Minimum supported combined set:

```text
http+socks4+socks5
```

If Shadowsocks, Trojan, or another non-sniffable protocol appears in a mixed list, reject the composition with a diagnostic rather than silently selecting one protocol.

### No-argument behavior

The `pproxy` executable with no arguments must generate the pproxy default mixed HTTP/SOCKS4/SOCKS5 listener on port 8080 with direct routing.

Do not alter the native `eggress` executable default unless it already intentionally shares this behavior.

### Canonical authentication

Support pproxy authentication placement and translate it into existing listener/upstream auth structures. Keep existing userinfo forms accepted where they are already useful, but classify them as compatibility extensions if they are not canonical pproxy syntax.

### Fixed targets and local bind

The parser must represent these even where runtime support is deferred:

- tunnel/raw fixed destination;
- WebSocket/H2 fixed target, if pproxy syntax permits it;
- outbound source address binding.

Phase 1 acceptance requires correct translation or a precise `parsed_but_not_supported` diagnostic. Runtime implementation may occur in Phase 3 or Phase 5.

## Workstream 1.1 — Build oracle fixture table

Before changing code, add a compact table of approximately 20 to 30 canonical examples under the existing compatibility fixtures. Include:

- default mixed listener;
- each common singular protocol;
- combined listener;
- IPv6;
- fragment auth;
- Shadowsocks and Trojan credentials;
- local bind;
- fixed target;
- plugin list;
- rule suffix;
- two-hop chain;
- malformed variants;
- CLI options with required values.

For each case, record only the parse outcome and key normalized fields. Do not create a new general observation framework.

## Workstream 1.2 — Refactor the URI parser

1. Introduce the typed AST without changing translation output.
2. Port existing parser tests.
3. Add canonical grammar cases.
4. Update redaction.
5. Update chain parsing to operate on top-level delimiters only.
6. Add composition validation separate from syntax parsing.
7. Remove legacy parser branches made redundant by the AST.

Keep the refactor inside the compatibility crate unless a small shared endpoint parser is clearly reusable.

## Workstream 1.3 — Correct CLI argument parsing

1. Replace raw string flags with typed option variants where practical.
2. Make every value-taking option consume exactly one following token or produce a missing-value error.
3. Preserve repeated `-l`, `-r`, `-ul`, and `-ur` order.
4. Ensure values beginning with `-` can still be consumed where argparse would accept them.
5. Correct positional handling after value-taking options.
6. Implement no-argument compatibility defaults.
7. Keep unknown flags visible as warnings or hard errors according to oracle behavior.

## Workstream 1.4 — Correct `--pac`, `--get`, and `--test`

Use existing Eggress components rather than building new servers:

- `--pac VALUE` should pass the supplied value into the existing PAC/admin configuration rather than acting as a boolean only.
- `--get PATH,FILE` should configure static file serving through the existing admin HTTP server or the smallest existing HTTP-serving component. Validate the pair and reject unsafe or malformed paths.
- `--test TARGET` should invoke the existing upstream/test request path with the supplied target and exit rather than merely printing a recommendation.

If exact pproxy output formatting is expensive, match functional behavior, exit status class, and clear diagnostics first; record formatting as partial until closure.

## Workstream 1.5 — Translate mixed listeners and canonical auth

1. Map the default combined scheme to `protocols = ["http", "socks4", "socks5"]`.
2. Ensure the runtime's existing sniff buffer is used.
3. Translate fragment auth to listener or upstream credentials according to URI role.
4. Reject ambiguous auth forms with actionable diagnostics.
5. Preserve all secrets in config but redact them from displays, warnings, and reports.

## Workstream 1.6 — Documentation updates

Update only affected references:

- URI grammar documentation;
- CLI option table;
- no-argument behavior;
- unsupported plugin and legacy transport diagnostics;
- examples for mixed listeners, auth, and fixed targets.

Do not regenerate broad historical parity reports during implementation.

## Acceptance criteria

Phase 1 is complete when:

- the parser represents every documented top-level URI component listed above;
- canonical pproxy examples parse without losing syntax fields;
- unsupported plugin/transport components produce precise diagnostics after parsing;
- `http+socks4+socks5://:8080` translates to one mixed listener;
- no-argument `pproxy` behavior creates that mixed listener on port 8080;
- `--pac`, `--get`, and `--test` consume their values and do not leak them into positional parsing;
- fragment authentication is translated and redacted correctly;
- brace fixed-target syntax and local bind syntax are represented;
- repeated options preserve order;
- malformed delimiter cases fail deterministically;
- native Eggress URI/config behavior is unchanged unless explicitly shared;
- existing common HTTP/SOCKS translation tests remain green.

## Focused verification

```bash
cargo fmt --check
cargo test -p eggress-pproxy-compat uri
cargo test -p eggress-pproxy-compat args
cargo test -p eggress-pproxy-compat translate
cargo test -p eggress-cli --test cli_tests pproxy
```

Add a small optional oracle test command for the canonical fixture table. It should skip cleanly when pproxy is unavailable and must not become a routine CI requirement.

## Regression examples

These exact classes of bugs must have tests:

```text
pproxy --pac /proxy.pac -l http://:8080
pproxy --get /health,health.txt -l http://:8080
pproxy --test https://example.com -r socks5://proxy:1080
pproxy -l http+socks4+socks5://:8080
pproxy -l http://:8080#user:pass
pproxy -r tunnel{example.com:443}://gateway:9000
pproxy -r socks5://proxy:1080/@192.0.2.10
pproxy -r socks5://proxy:1080?rules.txt
```

The value following `--pac`, `--get`, or `--test` must never become a listener or remote.

## Rollback and compatibility notes

This phase changes parser behavior and may expose previously ignored syntax. Preserve legacy Eggress-compatible URI forms where they do not conflict with pproxy syntax. If a legacy interpretation conflicts, prefer pproxy behavior under the `pproxy` executable and compatibility Python namespace; native Eggress entry points may retain the old form.

## Handoff guidance

Implement in small commits:

1. AST and parser tests;
2. CLI typed options and arity;
3. mixed listener/default behavior;
4. PAC/get/test functional wiring;
5. docs and cleanup.

Do not combine routing semantics or Python package work into this phase.
