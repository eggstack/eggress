# eggress-cli

`crates/eggress-cli/`

CLI binary providing `eggress` and a pproxy-style `pproxy` executable. The
binary is a compatibility translator, not proof of full Python or CLI parity.

## Binaries

| Binary | Source | Description |
|---|---|---|
| `eggress` | `src/main.rs` | Primary CLI binary |
| `pproxy` | `src/pproxy_main.rs` | pproxy-compatible binary |

## eggress Modes

### Config Mode (default with `--config`)

```bash
eggress --config /path/to/config.toml
```

Runs the proxy from a TOML configuration file.

### CLI Mode (default without `--config`)

```bash
eggress -l http://0.0.0.0:8080 -r socks5://127.0.0.1:1080
```

Direct CLI arguments for quick proxy setup.

### Build Variants

The CLI supports feature-gated builds:

| Build | Command | Produces |
|---|---|---|
| Full (default) | `cargo install --path crates/eggress-cli` | `eggress` + `pproxy` binaries |
| Lean HTTP/SOCKS | `cargo build -p eggress-cli --release --no-default-features --features common` | `eggress` only |
| Smallest | `cargo build -p eggress-cli --profile release-small --no-default-features --features common` | `eggress` only |

The `pproxy` binary requires the `pproxy-compat` feature. Default builds include it.
The `common` feature explicitly forwards `eggress-runtime/common`; internal
workspace edges disable dependency defaults so a common build does not
reactivate `runtime/full`. Admin and metrics remain required runtime
dependencies, while the UDP crate's Shadowsocks flow types are enabled only
through the internal `eggress-udp/shadowsocks` gate used by `extended`.

### Route Explain

```bash
eggress route-explain --target example.com:443 --config config.toml
```

Explains which routing rule matches a given target.

### Upstream Test

```bash
eggress upstream-test --uri socks5://127.0.0.1:1080
```

Tests upstream reachability.

### System Proxy Inspect

```bash
eggress system-proxy-inspect
```

Shows current system proxy configuration.

## pproxy Binary

```bash
pproxy -l http://0.0.0.0:8080 -r socks5://127.0.0.1:1080
```

Accepts pproxy-style arguments and translates them internally to eggress configuration.

`--pac <PATH>`, `--get <PATH,FILE>`, and `--test <TARGET>` consume one required
value each. Values remain owned by the option. The strict compatibility parser
does not accept positional URIs or long listener aliases. PAC and static files
use the existing admin server; `--test` runs the shared Rust upstream-test
implementation in-process and exits before listener startup.

## Key Arguments

| Flag | Description |
|---|---|
| `-l` | Listener URIs (repeatable) |
| `-r` | Upstream proxy URIs (repeatable; chains use `__`) |
| `-ul`, `-ur` | UDP listener/upstream URIs (repeatable) |
| `-b`, `-a`, `-s` | Block regex, alive interval, scheduler (`fa`, `rr`, `rc`, `lc`) |
| `-d`, `-v` | Repeatable debug/verbosity count actions; `-vv` adds traffic statistics |
| `--ssl`, `--pac`, `--get` | Listener TLS, PAC path, and static content |
| `--auth`, `--sys`, `--reuse` | Auth reuse, system proxy, and SO_REUSEPORT |
| `--daemon`, `--test`, `--version` | Daemon refusal, test-and-exit, and version |
| `--config` | TOML configuration file |
| `--admin` | Admin endpoint address |

The compatibility `pproxy` binary defaults, with no arguments, to
`http+socks4+socks5://:8080` and direct routing. Repeated options retain
declaration order. `--auth` defaults to 30 days and enables bounded source-IP
reuse only for listeners with credentials. `--sys` applies after all listeners
bind and restores captured settings on shutdown or failed startup. `--daemon`
is fatal with exit code 5 before startup; parser errors are exit code 2. `-d`
adds compatibility error visibility, `-v` emits connection events, and `-vv`
emits traffic statistics from normal session reports. `--reuse` applies
SO_REUSEPORT before TCP bind on supported Unix platforms and fails clearly on
unsupported platforms. `RUST_LOG` remains authoritative.

The frozen parser does not advertise or accept `--log`, `-f/--config`,
`--rulefile`, positional URIs, or `--listen`/`--remote` aliases. Native Eggress
configuration and migration-only translation helpers may expose separate
options, but those are not pproxy 2.7.9 executable options.

## Dependencies

- `eggress-runtime` — runtime lifecycle
- `eggress-pproxy-compat` — pproxy translation
- `eggress-system-proxy` — system proxy inspection

See [overview.md](overview.md) for context.
