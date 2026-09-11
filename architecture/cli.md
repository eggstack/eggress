# eggress-cli — Binary Targets (`eggress` and compat `pproxy`)

Installs two executables from one crate. Both end at the same
`ServiceSupervisor`; they differ only in how arguments reach config.
Prebuilt GitHub Release archives (`release-binaries.yml`) ship both binaries
at one version with default features; standalone install is
`docs/INSTALLATION.md` (binary installer preferred, Cargo for custom builds).

## Module map

`src/main.rs` is a thin parse/dispatch/process boundary; command work lives
in `src/commands/`, the schema in `src/cli.rs`, logging policy in
`src/logging.rs`, self-update mechanics in `src/update/`, and the shared
compat pipeline in the `eggress-cli` library (`src/pproxy_exec.rs`).

| File | Role |
|---|---|
| `src/main.rs` | `eggress` binary: parse argv, dispatch, map result to `ExitCode` |
| `src/cli.rs` | Clap schema, closed value enums, resolved `CliContext` |
| `src/logging.rs` | `init_logging` / `init_pproxy_logging` format policy |
| `src/commands/run.rs` | Native proxy startup, listener bind/serve, signals, drain |
| `src/commands/route.rs` | Route explain (local router or `eggress-admin` client) |
| `src/commands/upstream.rs` | Upstream diagnostics over the production connector |
| `src/commands/pproxy.rs` | `translate` / `check` / `run` (run via the shared facade) |
| `src/commands/version.rs` | `eggress X.Y.Z` rendering |
| `src/commands/update.rs` | `update` command adapter over `src/update/` |
| `src/commands/system_proxy.rs` | `system-proxy inspect` (behind `operations`) |
| `src/update/` | Self-update: version/target/download/verify/install |
| `src/pproxy_main.rs` | `pproxy` binary: flat compat facade over the shared pipeline |
| `src/lib.rs` | Shared library: exit-code re-export, upstream test, daemonize |
| `src/pproxy_exec.rs` | Shared compat prepare/compile/execute + banner lines (behind `pproxy-compat`) |

## Binaries

| Binary | Source | Build requirement |
|---|---|---|
| `eggress` | `src/main.rs` | none (always built) |
| `pproxy` | `src/pproxy_main.rs` | `required-features = ["pproxy-compat"]` |

### eggress — native CLI

Clap-derived parser (`src/cli.rs`):

| Flag / Arg | Description |
|---|---|
| `-l` / `--listen URI` | Listener URI (repeatable) |
| `-r` / `--remote URI` | Upstream URI (repeatable) |
| `--log-format` | Typed enum: `pretty` (default) / `compact` / `json`; typos fail at parse |
| `-c` / `--config PATH` | Single global config source (before or after subcommand; incompatible with `-l`/`-r` for startup) |
| `--rules-file PATH` | Host-regex rules file for routing |

**Subcommands**:

| Subcommand | Feature gate | Description |
|---|---|---|
| `version` | — | Print `eggress X.Y.Z` deterministically, no runtime init |
| `update` | — | Self-update from verified GitHub Release assets (flagless) |
| `route <target>` | — | Offline or live route-explain (via `--admin`) |
| `upstream test` | — | Connectivity probe against upstreams (`--mode proxy\|tcp`) |
| `pproxy translate` | `pproxy-compat` | pproxy args to TOML |
| `pproxy check` | `pproxy-compat` | pproxy args parity report |
| `pproxy run` | `pproxy-compat` | Shared-facade translate + gate + run supervisor |
| `system-proxy inspect` | `operations` | Read system proxy settings |

**Default behavior**: when no subcommand and no `--config`, builds a
router from `-l`/`-r` flags. Default listener is `http://127.0.0.1:8080`.
All listeners bind before any accept task spawns; a bind failure exits
`4` (`EXIT_BIND_FAILURE`) instead of serving a subset. Registers
SIGINT/SIGTERM/SIGHUP handlers on Unix. SIGHUP without `--config` is
logged and ignored.

**Upstream test** (`upstream test`): requires `--config`. Accepts `--id`
(upstream filter), `--target` (default `example.com:443`), `--mode`
(typed `proxy` or `tcp`), `--timeout` (default 5s), `--json`. `proxy` mode
traverses the compiled chain with the production connector registry;
`tcp` mode keeps the intentional first-hop reachability check.

**Update** (`update`): discovers the latest stable GitHub Release, no-ops
when current, otherwise downloads the exact target archive + checksum,
verifies SHA-256 then both staged versions (each must equal the tag and
agree), and replaces the `eggress`/sibling-`pproxy` pair transactionally.
See `src/update/` and `docs/INSTALLATION.md`.

### pproxy — compat wrapper

`src/pproxy_main.rs` is a thin synchronous facade over
`eggress_cli::pproxy_exec`. Flow:

1. Collect `std::env::args_os().skip(1)`.
2. `pproxy_exec::prepare()` — parse (or `default_args()`), `--version` /
   `--help` detection, strict violations → 2, `validate_strict_values()`
   → 2, `translate_pproxy_args()` → 3, shared fail-closed
   `evaluate_execution_gate()` (unknown flags → 2, unsupported → 5).
3. Version/help render the flat pproxy surface and exit 0.
4. Non-test runs print the startup banner from
   `pproxy_exec::banner_lines()` (structured parser state: listeners,
   remotes, UDP addrs, TLS/PAC flags, reuse), then `pproxy_exec::compile()`
   validates the translated TOML in memory.
5. `init_logging()` resolves `-d`/`-v` via `default_log_level()`
   (`RUST_LOG` precedence), then `pproxy_exec::execute()` runs warnings,
   `--test` diagnostics, `--daemon` transition, hooks
   (`CompatibilityRuntimeHooks::from_facade`) and
   `ServiceSupervisor::start_from_config_with_compatibility()`.

`eggress pproxy run` runs the same `prepare`/`compile`/`execute` sequence
through `src/commands/pproxy.rs` with an empty diagnostic prefix and no
banner. Help text stays custom for pproxy fidelity, but
`help_covers_every_recognized_option` pins its inventory to
`PproxyArgs::recognized_option_names()` so parser and help cannot drift.

## Shared library (src/lib.rs + src/pproxy_exec.rs)

### Exit codes

Single owner: `eggress-pproxy-compat::exit_codes` (typed `ProcessExit`
plus the numeric constants). `eggress-cli` re-exports them; without
`pproxy-compat` it mirrors the same numbers with a pinned equality test.

| Code | Constant | Meaning | Wired call sites |
|---|---|---|---|
| 0 | `EXIT_SUCCESS` | Success | everywhere |
| 1 | `EXIT_RUNTIME_FAILURE` | Runtime error | supervisor/drain/serialize/daemon-spawn/update-verify failures |
| 2 | `EXIT_CLI_PARSE_ERROR` | CLI parse error / unknown flag / bad closed-domain value / bad target | Clap rejections, strict gate, invalid release tag |
| 3 | `EXIT_CONFIG_VALIDATION` | Config validation failure | TOML load/compile, empty upstream set |
| 4 | `EXIT_BIND_FAILURE` | Listener bind failure | native bind-before-spawn |
| 5 | `EXIT_UNSUPPORTED_FEATURE` | Unsupported feature / composition | compat gate blockers |
| 6 | `EXIT_PLATFORM_MISSING` | Required platform facility unavailable | `--daemon` off Linux, updater on unsupported target |
| 7 | `EXIT_EXTERNAL_DEPENDENCY` | External dependency unavailable | updater discovery/download/tool failures |
| 130 | `EXIT_SIGINT` | SIGINT received | Unix Ctrl-C / Windows Ctrl-C |
| 143 | `EXIT_SIGTERM` | SIGTERM received | Unix SIGTERM |

### Upstream test functions

| Function | Description |
|---|---|
| `run_upstream_test()` | Delegates to `run_upstream_test_with_mode` with `mode="proxy"` |
| `run_upstream_test_with_mode()` | Shared impl: iterates upstreams, runs proxy or TCP test |
| `build_test_chain_executor()` | Returns the **production** `eggress_server::build_chain_executor` registry |
| `run_async_test()` | Spawns a dedicated Tokio runtime on a named thread `"eggress-cli-test"` |
| `parse_pproxy_test_target()` | Parses URL-shaped `--test` value into `TargetAddr` |

### Chain executor (test mode)

No CLI-local handshake implementations remain. `build_test_chain_executor()`
delegates to `eggress_server::build_chain_executor` (unified by Cargo
features with the live data plane), so diagnostic coverage tracks
production support — including Shadowsocks/Trojan/WebSocket under
`extended`, SSH/QUIC where enabled. `production_registry_tests` proves a
Shadowsocks hop resolves to connection behavior, never "no handler".

### Compat execution facade (`src/pproxy_exec.rs`)

`prepare()` (parse → strict → translate → gate) returns
`PrintVersion`/`PrintHelp`/`Run(GatedRun)` or a typed `PrepareFailure`;
`compile()` validates translated TOML in memory; `execute()` renders
warnings, `--test`, daemon transition, and supervisor startup with a
caller-supplied diagnostic prefix. Banner lines come from structured
`PproxyArgs` accessors, not string scans.

### Daemonize

`maybe_daemonize()`: Linux-only safe re-exec. Sets
`EGRESS_PPROXY_DAEMON_CHILD` env var, spawns child with same args, parent
exits with code 0. Guard: if env var is already set, returns immediately.
Failures are typed (`DaemonizeError::UnsupportedPlatform` → 6,
`DaemonizeError::Spawn` → 1).

## Feature map

| Feature | Includes | Description |
|---|---|---|
| `full` (default) | `common`+`extended`+`operations`+`reverse`+`pproxy-compat` | All standard protocols |
| `common` | `eggress-runtime/common` | HTTP, SOCKS4/5, Shadowsocks, Trojan |
| `extended` | `eggress-runtime/extended` | WebSocket, raw, H2 |
| `pproxy-compat` | `dep:eggress-pproxy-compat` | pproxy translation + check + run |
| `operations` | `dep:eggress-system-proxy` | `system-proxy inspect` |
| `reverse` | `eggress-runtime/reverse` | Reverse proxy control channel |
| `ssh` | `eggress-runtime/ssh`, `eggress-pproxy-compat/ssh` | SSH transport |
| `quic` | `eggress-config/quic`, `eggress-runtime/quic`, `eggress-pproxy-compat/quic`, `dep:eggress-transport-quic`, `dep:eggress-protocol-h3` | QUIC + H3 |
| `insecure-quic` | `eggress-runtime/insecure-quic`, `eggress-transport-quic/insecure-quic` | Test-only cert bypass (never part of the routine product gate) |
| `pproxy-legacy` | `eggress-runtime/pproxy-legacy` | Bounded SSR TCP framing + six built-in plugins (opt-in; not in default `full`) |
| `legacy-crypto` | `eggress-runtime/legacy-crypto`, `eggress-pproxy-compat/legacy-crypto` | Legacy Shadowsocks ciphers |
| `pproxy-daemon` | `pproxy-compat`, `eggress-pproxy-compat/daemon` | Linux `--daemon` re-exec |

Lean build: `cargo build -p eggress-cli --release --no-default-features --features common`

Routine CI adds one bounded compile-only gate for the product-relevant
optional compatibility bundle (same job, no new workflow or OS matrix;
`insecure-quic` excluded):

```bash
cargo check -p eggress-cli --locked --no-default-features \
  --features full,ssh,quic,pproxy-legacy,legacy-crypto,pproxy-daemon \
  --bins
```

## Test suite (19 integration files + binary unit tests)

| File | What it exercises |
|---|---|
| `cli_exit_codes.rs` | Stable exit codes (0–7, 130, 143) for various CLI invocations |
| `version.rs` | `eggress version` stable line, `eggress --version`, `pproxy --version` |
| `release_contract.rs` | Installer/workflow/docs drift checks (targets, archives, checksums, no-sudo, default-features builds, `update` docs/targets) |
| `cli_tests.rs` | Help, global `--config` placement (before/after/`-c`), closed-domain rejections, occupied-port bind exit 4 |
| `integration.rs` | End-to-end proxy startup and forwarding |
| `reply_order.rs` | HTTP reply ordering guarantees |
| `feature_boundary_negative.rs` | Feature-gated exclusions (e.g., SSH without feature) |
| `pproxy_cli.rs` | `eggress pproxy translate/check/run` subcommand tests |
| `pproxy_binary.rs` | `pproxy` binary compat wrapper tests |
| `pproxy_translation_golden.rs` | Golden-file TOML translation tests |
| `pproxy_run_process.rs` | Process-level pproxy run tests |
| `pproxy_differential.rs` | Differential tests against pproxy oracle |
| `differential_pproxy.rs` | Extended differential tests (opt-in, `EGRESS_REQUIRE_EXTERNAL_INTEROP`) |
| `oracle.rs` | Oracle interpreter tests |
| `interoperability_pproxy.rs` | Live interop with pproxy (opt-in) |
| `interoperability_shadowsocks.rs` | Live interop with Shadowsocks (opt-in) |
| `interoperability_trojan.rs` | Live interop with Trojan (opt-in) |
| `interoperability_curl.rs` | curl-based HTTP proxy tests |
| `advanced_transport_interop.rs` | SSH/QUIC transport interop |

Opt-in suites are gated by env vars (`EGRESS_REQUIRE_EXTERNAL_INTEROP=1`,
`EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1`).

Unit tests also live in the binaries: `src/update/` (version/target/URL/
checksum/version-parse/sibling/rollback plus offline `file://`
end-to-end), `src/pproxy_exec.rs` (prepare/gate/compile/banner),
`src/cli.rs` neighbors in `tests/cli_tests.rs`, and the `pproxy_main`
help-drift guard.

## Concurrency & lifecycle

- `run()` in `main.rs` is an async function driven by `#[tokio::main]`;
  handlers returning codes stay synchronous except `route --admin` and
  native startup, which await.
- Signal handling: SIGINT triggers `EXIT_SIGINT` (130), SIGTERM triggers
  `EXIT_SIGTERM` (143). On non-Unix, only ctrl-c is handled.
- Connection drain: 30-second deadline after cancel, woken by
  `ACTIVE_CONNECTIONS_DRAIN` as connections complete.
- `pproxy` binary is synchronous — no Tokio runtime unless upstream test
  triggers `run_async_test()`.

## Reviewer gotchas

- `--config` and `-l`/`-r` are mutually exclusive — mixing them exits
  with code 2.
- `--config` is global: `eggress --config f route t` ≡
  `eggress route t --config f` (also `-c`). Route/upstream handlers read
  it from `CliContext`, never from their own duplicate option.
- `--log-format`, `upstream test --mode`, and `route --protocol` are
  Clap value enums: typos exit 2 at parse time. Diagnostics that mention
  the admin option always say `--admin` (the actual option name).
- The `route explain` command supports both offline mode (no config →
  default `Router::new(vec![], RouteActionSpec::Direct)`) and online mode
  (via `--admin` URL through `eggress-admin::client::route_explain`).
- `run_async_test()` detects whether a Tokio runtime is already
  current. If so, it spawns a dedicated OS thread with a new multi-thread
  runtime to avoid nested runtime panics.
- Compat execution builds `CompatibilityRuntimeHooks` from parsed pproxy
  args: `AuthReuseCache::new(effective_auth_timeout())` (always,
  preserving the 30-day default), `--sys` as `Some(SystemProxyRequest)`,
  SSH env via `ssh_insecure_acknowledged()` (warns when verification stays
  on under `ssh` feature). `-d`/`-v` never enter the supervisor; they were
  resolved into the tracing filter before construction. No config file path
  is provided, so SIGHUP reload is disabled in compat mode.
- `update` never scans `PATH` for a sibling: the `pproxy` path derives
  from the running executable and its identity is verified (exists +
  reports the current version) before anything is downloaded.

## See also

- [embed.md](embed.md) — Rust embed API alternative to CLI
- [transports-ssh-quic-h3.md](transports-ssh-quic-h3.md) — SSH/QUIC/H3 features

## Review entry points

- `cargo test -p eggress-cli --test cli_exit_codes`
- `cargo test -p eggress-cli --test cli_tests`
- `cargo test -p eggress-cli --test version`
- `cargo test -p eggress-cli --test release_contract`
- `cargo test -p eggress-cli --bin eggress update`
- `bash packaging/tests/test-install.sh`
- `cargo test -p eggress-cli --test pproxy_translation_golden`
