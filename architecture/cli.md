# eggress-cli — Binary Targets (`eggress` and compat `pproxy`)

Installs two executables from one crate. Both end at the same
`ServiceSupervisor`; they differ only in how arguments reach config.

## Module map

| File | Role |
|---|---|
| `src/main.rs` | `eggress` binary: native CLI flags, subcommands, signal handling |
| `src/pproxy_main.rs` | `pproxy` binary: pproxy-2.7.9 compat wrapper |
| `src/lib.rs` | Shared library: exit codes, upstream test, chain executor, daemonize |

## Binaries

| Binary | Source | Build requirement |
|---|---|---|
| `eggress` | `src/main.rs` | none (always built) |
| `pproxy` | `src/pproxy_main.rs` | `required-features = ["pproxy-compat"]` |

### eggress — native CLI

Clap-derived parser (`src/main.rs:24-44`):

| Flag / Arg | Description |
|---|---|
| `-l` / `--listen URI` | Listener URI (repeatable) |
| `-r` / `--remote URI` | Upstream URI (repeatable) |
| `--config PATH` | TOML config file (incompatible with `-l`/`-r`) |
| `--rules-file PATH` | Host-regex rules file for routing |
| `--log-format FORMAT` | `pretty` (default) / `json` / `compact` |

**Subcommands** (`src/main.rs:47-54`):

| Subcommand | Feature gate | Description |
|---|---|---|
| `route <target>` | — | Offline or live route-explain (via `--admin`) |
| `upstream test` | — | Connectivity probe against upstreams (`--mode proxy\|tcp`) |
| `pproxy translate` | `pproxy-compat` | pproxy args to TOML |
| `pproxy check` | `pproxy-compat` | pproxy args parity report |
| `pproxy run` | `pproxy-compat` | Translate + gate + run supervisor |
| `system-proxy inspect` | `operations` | Read system proxy settings |

**Default behavior**: when no subcommand and no `--config`, builds a
router from `-l`/`-r` flags. Default listener is `http://127.0.0.1:8080`
(:1163-1165). Registers SIGINT/SIGTERM/SIGHUP handlers on Unix
(:1209-1251). SIGHUP without `--config` is logged and ignored (:1238).

**Upstream test** (`upstream test`): requires `--config`. Accepts `--id`
(upstream filter), `--target` (default `example.com:443`), `--mode`
(`proxy` or `tcp`), `--timeout` (default 5s), `--json`.

### pproxy — compat wrapper

`src/pproxy_main.rs` is a synchronous `fn main()`. Flow:

1. Collect `std::env::args_os().skip(1)` (:66-69).
2. `PproxyArgs::parse(&args)` or `default_args()` if no args (:71-81).
3. Handle `--version` / `--help` (:86-93).
4. Strict parser violations: unknown flags → exit code 2 (:95-98).
5. `validate_strict_values()` → exit code 2 on failure (:100-103).
6. `translate_pproxy_args()` → TOML output (:105-111).
7. `evaluate_execution_gate()` — fail-closed: unknown flags → exit 2,
   unsupported features → exit 5 (:116-140).
8. `--test` target → `run_upstream_test()` then exit (:163-177).
9. `--daemon` → `maybe_daemonize()` (Linux only) (:179-183).
10. `validate_and_compile_toml_with_warnings()` on translated TOML (:154-161).
11. `ServiceSupervisor::start_from_config_with_options()` with
    `CompatibilityOptions` (:192-214).

## Shared library (src/lib.rs)

### Exit codes

| Code | Constant | Meaning |
|---|---|---|
| 0 | `EXIT_SUCCESS` | Success |
| 1 | `EXIT_RUNTIME_FAILURE` | Runtime error |
| 2 | `EXIT_CLI_PARSE_ERROR` | CLI parse error / unknown flag |
| 3 | `EXIT_CONFIG_VALIDATION` | Config validation failure |
| 5 | `EXIT_UNSUPPORTED_FEATURE` | Unsupported pproxy feature |
| 130 | `EXIT_SIGINT` | SIGINT received |
| 143 | `EXIT_SIGTERM` | SIGTERM received |

These are part of the compatibility surface (:3-4).

### Upstream test functions

| Function | Line | Description |
|---|---|---|
| `run_upstream_test()` | :107 | Delegates to `run_upstream_test_with_mode` with `mode="proxy"` |
| `run_upstream_test_with_mode()` | :117 | Shared impl: iterates upstreams, runs proxy or TCP test |
| `build_test_chain_executor()` | :378 | Creates `ChainExecutor` with HTTP/SOCKS5/SOCKS4 hop handlers |
| `run_async_test()` | :236 | Spawns a dedicated Tokio runtime on a named thread `"eggress-cli-test"` |
| `parse_pproxy_test_target()` | :64 | Parses URL-shaped `--test` value into `TargetAddr` |

### Chain executor (test mode)

`build_test_chain_executor()` (:378-385) registers three `HopHandler`
implementations:

| Handler | Protocol | Line |
|---|---|---|
| `HttpHopHandler` | `ProtocolSpec::Http` | :281 |
| `Socks5HopHandler` | `ProtocolSpec::Socks5` | :314 |
| `Socks4HopHandler` | `ProtocolSpec::Socks4` | :348 |

### Daemonize

`maybe_daemonize()` (:18-42): Linux-only safe re-exec. Sets
`EGGRESS_PPROXY_DAEMON_CHILD` env var, spawns child with same args, parent
exits with code 0. Guard: if env var is already set, returns immediately.

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
| `insecure-quic` | `eggress-runtime/insecure-quic`, `eggress-transport-quic/insecure-quic` | Test-only cert bypass |
| `legacy-crypto` | `eggress-runtime/legacy-crypto`, `eggress-pproxy-compat/legacy-crypto` | Legacy Shadowsocks ciphers |
| `pproxy-daemon` | `pproxy-compat`, `eggress-pproxy-compat/daemon` | Linux `--daemon` re-exec |

Lean build: `cargo build -p eggress-cli --release --no-default-features --features common`

## Test suite (17 integration files)

| File | What it exercises |
|---|---|
| `cli_exit_codes.rs` | Exit codes 0/1/2/3/5 for various CLI invocations |
| `cli_tests.rs` | General CLI flag parsing and behavior |
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

## Concurrency & lifecycle

- `run()` in `main.rs` (:1081) is an async function driven by `#[tokio::main]`.
- Signal handling: SIGINT triggers `EXIT_SIGINT` (130), SIGTERM triggers
  `EXIT_SIGTERM` (143). On non-Unix, only ctrl-c is handled.
- Connection drain: 30-second deadline after cancel, polling `ACTIVE_CONNECTIONS`
  every 100ms (:1256-1268).
- `pproxy` binary is synchronous — no Tokio runtime unless upstream test
  triggers `run_async_test()`.

## Reviewer gotchas

- `--config` and `-l`/`-r` are mutually exclusive (:1126-1129) — mixing
  them exits with code 2.
- The `route explain` command supports both offline mode (no config →
  default `Router::new(vec![], RouteActionSpec::Direct)`) and online mode
  (via `--admin` URL for live state).
- `run_async_test()` (:236-261) detects whether a Tokio runtime is already
  current. If so, it spawns a dedicated OS thread with a new multi-thread
  runtime to avoid nested runtime panics.
- `pproxy_run` sets `CompatibilityOptions` (:786-792) including
  `auth_timeout`, `system_proxy`, `debug`, `verbose_level` from parsed
  pproxy args. No config file path is provided, so SIGHUP reload is
  disabled in compat mode.

## See also

- [embed.md](embed.md) — Rust embed API alternative to CLI
- [transports-ssh-quic-h3.md](transports-ssh-quic-h3.md) — SSH/QUIC/H3 features

## Review entry points

- `cargo test -p eggress-cli --test cli_exit_codes`
- `cargo test -p eggress-cli --test pproxy_translation_golden`
