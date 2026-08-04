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
value each. Values remain owned by the option rather than becoming positional
listener or remote URIs. PAC and static files use the existing admin server;
`--test` delegates the supplied target to `eggress upstream test`.

## Key Arguments

| Flag | Description |
|---|---|
| `-l, --listen` | Listener URIs (multiple allowed) |
| `-r, --remote` | Upstream proxy URIs (chains with `__`) |
| `--config` | TOML configuration file |
| `--admin` | Admin endpoint address |

The compatibility `pproxy` binary defaults, with no arguments, to
`http+socks4+socks5://:8080` and direct routing. Repeated `-l`, `-r`, `-ul`, and
`-ur` options retain input order. Unsupported daemonization, reuse-port, and
plugin execution remain explicit diagnostics.

## Dependencies

- `eggress-runtime` — runtime lifecycle
- `eggress-pproxy-compat` — pproxy translation
- `eggress-system-proxy` — system proxy inspection

See [overview.md](overview.md) for context.
