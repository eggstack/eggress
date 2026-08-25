# eggress-cli — Binary Targets (`eggress` and compat `pproxy`)

Installs two executables. Both end at the same supervisor; they differ in how
arguments reach config.

## Binaries

| Binary | Source | Behavior |
|---|---|---|
| `eggress` | `src/main.rs` | Native CLI. `-l` listeners, `-r` upstreams, `--config`, `--rules-file`, `--log-format`. Subcommands: `route <target>` (explain offline or via admin), `upstream test` (proxy/tcp modes), `pproxy translate/check/run`, `system-proxy inspect` (feature). Defaults to an HTTP listener on 127.0.0.1:8080 |
| `pproxy` | `src/pproxy_main.rs` | pproxy-2.7.9-style wrapper: parse args → translate to TOML → execution gate → run supervisor |

## Shared library (`src/lib.rs`)

- Exit codes: 0 success · 1 runtime failure · 2 CLI parse error · 3 config
  validation · 5 unsupported feature · 130 SIGINT · 143 SIGTERM.
- `run_upstream_test[_with_mode]` + `build_test_chain_executor`: reuse the real
  chain machinery for connectivity probes.
- `maybe_daemonize` behind `pproxy-daemon` (Linux safe re-exec).

## Feature map (this crate defines the lean-build story)

`full` (default) = common+extended+operations+reverse+pproxy-compat; optional:
`ssh`, `quic`, `legacy-crypto`, `pproxy-daemon`.
Lean build: `cargo build -p eggress-cli --release --no-default-features --features common`

## Test suite (17 integration files)

Exit codes, golden translation files, differential vs. oracle pproxy,
interoperability (pproxy/shadowsocks/trojan/curl/advanced transports), reply
ordering, feature-boundary negatives. Most interop suites are opt-in via env
gates (`EGRESS_REQUIRE_EXTERNAL_INTEROP=1`, etc.).

## Review entry points

- Verify: `cargo test -p eggress-cli --test cli_exit_codes`
