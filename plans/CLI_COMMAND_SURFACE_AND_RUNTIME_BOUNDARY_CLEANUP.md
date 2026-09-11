# CLI Command Surface and Runtime Boundary Cleanup

## Status

Implementation plan for handoff. This plan is the Phase 1 execution document referenced by `CLI_CLEANUP_AND_BINARY_DELIVERY_ROADMAP.md`.

## Objective

Reduce maintenance cost and behavioral drift in `eggress-cli` without reducing proxy capability, changing pproxy compatibility semantics, or adding new native proxy features.

The desired end state is a thin process-facing CLI layer that owns argument parsing, user-facing rendering, and process exit behavior, while configuration compilation, admin control-plane calls, protocol connection establishment, and service lifecycle remain in the appropriate runtime/library crates.

## Current problems to correct

### 1. `src/main.rs` is a responsibility monolith

The native executable currently combines:

- Clap schema definitions;
- configuration path/value resolution;
- route explanation logic;
- a hand-written HTTP/1.1 admin client;
- upstream diagnostic execution;
- pproxy translate/check/run dispatch;
- logging setup;
- runtime construction and shutdown;
- signal/process exit handling.

This makes changes to small CLI concerns disproportionately risky and encourages runtime behavior to be reimplemented at the process boundary.

### 2. Configuration ownership is duplicated

`Cli` exposes top-level `--config`, while route and upstream diagnostic commands also carry their own `--config` fields. The project should have one clear CLI-level configuration source and one resolution policy.

### 3. Closed value domains are stringly typed

Examples include:

- `--log-format` where unknown values currently fall back to pretty output;
- `upstream test --mode` where only `proxy` is treated specially and arbitrary other strings reach the alternate path;
- route protocol selection implemented by manual string matching.

These should fail at argument parsing when a value is invalid.

### 4. Admin route explanation reimplements HTTP

The remote route-explain path builds an HTTP request string manually, opens a `TcpStream`, scans for the HTTP body separator, and parses the status line itself. This duplicates protocol mechanics inside a command handler and creates a future maintenance trap for admin endpoint changes.

### 5. Upstream tests use a reduced CLI-local protocol registry

`eggress-cli` currently builds a diagnostic `ChainExecutor` containing CLI-local HTTP, SOCKS5, and SOCKS4 hop handlers. Production Eggress supports a wider runtime connector surface. As a result, `eggress upstream test` and pproxy `--test` can lag actual runtime support.

The diagnostic path must use the same production connector/handler registry as live connections.

### 6. pproxy execution orchestration is duplicated

The standalone `pproxy` binary and `eggress pproxy run` are alternate entry points to the same compatibility intent, but parsing/gating/config/runtime orchestration is spread across binary-specific code. The two entry points should converge on one typed execution facade.

### 7. Compatibility presentation contains secondary sources of truth

The standalone compatibility binary has a large static help string, parser state lives elsewhere, compatibility capability state is partly represented through string buckets such as `known_unsupported`, and the startup banner infers translated features by scanning those strings.

The parser/capability model should be authoritative; presentation should consume structured state rather than reverse-engineer it.

### 8. Exit-code ownership is split

CLI constants and pproxy-compat exit-code definitions should not independently encode overlapping numeric process contracts.

## Non-goals

Do not:

- redesign the public pproxy syntax;
- add native Eggress equivalents for every pproxy option;
- introduce a generic command/plugin framework;
- change the native proxy configuration model;
- broaden protocol support as part of this cleanup;
- add self-update/release logic here (covered by later plans);
- turn this into a workspace-wide architecture refactor.

## Phase A — Establish a thin command-dispatch structure

Refactor `crates/eggress-cli/src/main.rs` into bounded modules. A suitable shape is:

```text
crates/eggress-cli/src/
  main.rs
  logging.rs
  commands/
    mod.rs
    run.rs
    route.rs
    upstream.rs
    pproxy.rs
    system_proxy.rs        # feature-gated if retained here
```

The exact module names are not contractual. The responsibility split is.

`main.rs` should be limited to:

1. parsing argv;
2. resolving the selected command/global context;
3. initializing process-wide facilities that must be initialized once;
4. dispatching to a command function;
5. translating a typed command result into `ExitCode`/process termination.

Do not move code merely to make files smaller if responsibilities remain tangled. Each extracted command module should receive typed inputs and call library/runtime APIs.

### Acceptance criteria

- `main.rs` no longer contains route HTTP parsing, protocol hop handler implementations, or the substantive pproxy runtime execution flow.
- command modules can be unit-tested without reconstructing the entire process where practical.
- no CLI syntax changes are introduced solely by the file split.
- `cargo fmt`, focused clippy, and `eggress-cli` tests remain clean.

## Phase B — Centralize global configuration resolution

Make the native configuration path a single global CLI concern.

Preferred implementation:

- keep one `--config <PATH>` definition on the top-level `Cli`;
- mark it global in Clap so commands may consume it consistently regardless of whether users place it before or after the subcommand, if current compatibility tests permit that behavior;
- remove duplicate `config` fields from `RouteExplain` and `UpstreamTest`;
- pass a `CliContext` or equivalent resolved global value into command handlers.

If existing process tests prove that argument placement is intentionally different, preserve compatibility and still centralize the resolved value internally. Do not break accepted invocations merely to make the parser prettier.

Explicitly define precedence where both direct `-l/-r` arguments and `--config` can participate in native startup. Preserve current semantics unless they are already documented as invalid.

### Tests

Add process-level tests covering at minimum:

```text
eggress --config cfg.toml route example.com:443
eggress route example.com:443 --config cfg.toml      # if global placement is intentionally accepted
eggress --config cfg.toml upstream test
eggress upstream test --config cfg.toml              # if global placement is intentionally accepted
```

Also retain tests for normal top-level proxy startup with `--config`.

### Acceptance criteria

- there is one authoritative parsed config path in the native CLI model;
- route/upstream commands no longer independently define the same public option;
- previous valid invocation ordering remains valid unless a documented breaking change is explicitly approved.

## Phase C — Replace stringly typed closed domains

Introduce small Clap `ValueEnum` types or equivalent parser functions for finite sets.

At minimum:

```text
LogFormat:
  pretty
  compact
  json

UpstreamTestMode:
  proxy
  tcp

RouteProtocol:
  http
  socks4
  socks5
```

If route protocol intentionally needs to expand dynamically with runtime protocol registration, keep the public parser extensible; otherwise use the closed enum currently implemented manually.

Remove silent fallback behavior for invalid values. A typo such as `--log-format jsno` must fail as a CLI parse error rather than silently selecting pretty output.

Align option names and diagnostics. In particular, if the public option remains `--admin`, errors must say `--admin`, not `--admin-url`. If the project chooses `--admin-url`, add a compatibility alias/deprecation path rather than silently renaming an existing option.

### Acceptance criteria

- invalid closed-domain values exit through the normal Clap parse-error path;
- command handlers receive typed values rather than repeat string matching;
- help output displays allowed values where Clap supports doing so;
- tests cover one invalid value for each closed domain.

## Phase D — Move route admin transport out of the command handler

Remove the hand-written HTTP/1.1 client from the route command.

Preferred ownership is `eggress-admin` if that crate already represents the admin protocol/control plane. Add a minimal client-facing function/type there, for example conceptually:

```rust
AdminClient::route_explain(request) -> Result<RouteExplanation, AdminClientError>
```

The implementation may remain deliberately small and HTTP/1.1-only if that is the admin protocol contract, but request construction, status parsing, body extraction, transport errors, and response deserialization must not live in `eggress-cli::commands::route`.

If `eggress-admin` is server-only by design and adding a client API would invert crate dependencies, place the transport in the narrowest neutral control-plane module that can be shared. Do not introduce `reqwest` solely to simplify a few lines if an existing internal HTTP implementation can provide a correct bounded client.

The CLI route command should only:

- parse/validate user target/listener/protocol inputs;
- choose local vs remote explanation;
- call the router or admin client;
- render text/JSON;
- map errors to process outcomes.

### Tests

- IPv4 admin endpoint;
- bracketed IPv6 endpoint;
- non-200 admin response;
- malformed response/body;
- connection failure;
- JSON and human rendering remain unchanged unless explicitly corrected.

### Acceptance criteria

- no manual HTTP request string or response delimiter scan remains in the CLI route handler;
- admin transport behavior has a focused test surface in its owning crate/module;
- route CLI behavior remains externally compatible.

## Phase E — Make upstream testing use production connector machinery

This is the most important correctness item in this cleanup.

Remove CLI-local protocol implementations such as `HttpHopHandler`, `Socks5HopHandler`, `Socks4HopHandler`, and the CLI-created reduced `ChainExecutor` registry.

Expose a runtime/library diagnostic API that uses the same handler/transport registration as production outbound connections. A suitable conceptual API is:

```rust
pub async fn test_upstream(
    runtime: &RuntimeConfig,
    upstream_id: &str,
    target: &TargetAddr,
    timeout: Duration,
    mode: UpstreamTestMode,
) -> UpstreamTestResult
```

or, if existing connector APIs are already sufficient, have the CLI call those directly and keep only result formatting in `eggress-cli`.

For `proxy` mode, the test must traverse the compiled upstream chain using production connector behavior. For `tcp` mode, retain the intentionally simpler first-hop reachability check if that remains useful and documented.

The API must return structured failure information without calling `process::exit` internally.

`pproxy --test` must reuse the same implementation.

### Tests

Test at least representative production-supported paths that previously exceeded the CLI-local registry, choosing protocols enabled by default and practical for deterministic tests. Examples may include Shadowsocks/Trojan/WebSocket depending on existing testkit support. The goal is to prove registry parity, not to duplicate every protocol interoperability suite.

Also cover:

- direct/basic HTTP/SOCKS behavior still works;
- timeout behavior;
- unknown/missing upstream id;
- empty upstream set;
- target parse failures;
- JSON result serialization.

### Acceptance criteria

- no protocol handshake implementation is owned by `eggress-cli` solely for diagnostics;
- adding a production-supported upstream protocol no longer requires separately registering it in CLI test code;
- `eggress upstream test --mode proxy` and `pproxy --test` use the same production connector path;
- targeted tests prove at least one formerly-unrepresented production protocol can be diagnosed correctly.

## Phase F — Centralize pproxy execution

Create one typed pproxy execution facade in the compatibility/runtime boundary.

The facade should own the common sequence:

1. consume already-decoded pproxy arguments or argv according to the chosen boundary;
2. strict parser/value validation;
3. translate to structured/native configuration;
4. evaluate execution gate/blockers/warnings;
5. compile translated config;
6. process `--test` through the shared upstream diagnostic path;
7. construct compatibility runtime hooks;
8. start/run the service when appropriate.

Process-specific rendering can remain in the binaries, but business decisions must be returned as typed results rather than duplicated.

A useful split is:

```text
compat facade:
  prepare_execution(...)
  run_prepared(...)

standalone pproxy binary:
  exact argv/help/version/process UX
  render warnings/errors

eggress pproxy run:
  native nested-command argv adapter
  same prepared execution
```

Do not force the standalone pproxy parser through the native Eggress Clap hierarchy; preserving pproxy argument behavior takes priority.

### Acceptance criteria

- `pproxy` and `eggress pproxy run -- ...` do not each implement their own translation/gating/runtime pipeline;
- compatibility blockers/warnings are derived from the same structured result;
- representative invocations produce equivalent runtime configuration and failure classification through both entry points;
- standalone pproxy remains flat and pproxy-like.

## Phase G — Reduce compatibility presentation drift

### Help metadata

The standalone pproxy help output may remain custom if exact pproxy compatibility requires it, but recognized option metadata must not be independently hand-maintained in multiple places.

Choose one authoritative option description source adjacent to the compatibility parser and render help from that metadata, or add an invariant test that exhaustively compares parser-recognized options to the static help model. Prefer generation when it does not compromise parser fidelity.

### Structured capability state

Stop inferring startup banner features by scanning strings in a bucket named `known_unsupported` for prefixes such as `ssl=` or `pac=` when those features can be supported/native-equivalent.

Introduce or reuse structured fields that describe parsed/translated features. Names must reflect semantics. `unsupported` collections should contain unsupported items only.

### Acceptance criteria

- supported TLS/PAC/UDP/etc. state used for presentation is not recovered by parsing diagnostic strings;
- help/parser option inventories cannot silently drift;
- no compatibility tier semantics are changed as a side effect of the cleanup.

## Phase H — Establish one exit-code owner

Inventory every process exit class currently used by:

- native `eggress`;
- standalone `pproxy` facade;
- nested `eggress pproxy` commands;
- documentation.

Then define one process outcome/exit-code owner in the narrowest shared module. Prefer a typed enum with an explicit numeric conversion over free constants spread across crates.

Do not blindly add documented codes that are not actually produced. The existing documentation mentions classes such as bind/platform/dependency failures; confirm call sites and either wire them consistently or correct documentation in the documentation phase.

Signal exits (`130`, `143`) should remain stable where currently part of the user contract.

### Acceptance criteria

- overlapping numeric exit-code constants are not independently defined in multiple crates;
- docs can point to one stable mapping;
- process tests verify representative parse/config/runtime/unsupported outcomes.

## Phase I — Focused verification and regression closure

Run at minimum:

```bash
cargo fmt --all -- --check
cargo clippy -p eggress-cli -p eggress-pproxy-compat --all-targets -- -D warnings
cargo test -p eggress-cli --locked
cargo test -p eggress-pproxy-compat --locked
```

Add any runtime/admin crate tests required by moved code.

Run existing pproxy differential/process tests that cover affected CLI execution paths. Do not require unrelated performance/security suites unless the implementation changes those surfaces.

Capture `eggress --help`, `eggress route --help`, `eggress upstream test --help`, and `pproxy --help` before/after and intentionally account for every visible difference.

## Final acceptance criteria

This plan is complete when:

1. `eggress-cli/src/main.rs` is a thin parse/dispatch/process entry point rather than the owner of substantive runtime logic.
2. native config path resolution has one CLI owner.
3. finite CLI value domains reject invalid values at parse time.
4. route explanation no longer contains an ad hoc HTTP client in its command handler.
5. upstream proxy tests use production connector/handler registration.
6. `pproxy --test` shares that upstream diagnostic implementation.
7. standalone `pproxy` and `eggress pproxy run` share one compatibility execution pipeline.
8. standalone pproxy remains a flat compatibility facade.
9. compatibility banner/help logic consumes authoritative structured/parser state rather than string heuristics where practical.
10. exit-code ownership is centralized and documented behavior matches produced behavior.
11. existing proxy startup, route explanation, upstream test, pproxy translation/check/run, signal handling, and system-proxy inspection tests remain green.
12. no new generic framework, protocol feature, or release mechanism is introduced in this phase.
