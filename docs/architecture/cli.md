# eggress-cli

`crates/eggress-cli/`

CLI binary providing `eggress` and `pproxy` executables.

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

## Key Arguments

| Flag | Description |
|---|---|
| `-l, --listen` | Listener URIs (multiple allowed) |
| `-r, --remote` | Upstream proxy URIs (chains with `__`) |
| `--config` | TOML configuration file |
| `--admin` | Admin endpoint address |

## Dependencies

- `eggress-runtime` — runtime lifecycle
- `eggress-pproxy-compat` — pproxy translation
- `eggress-system-proxy` — system proxy inspection

See [overview.md](overview.md) for context.
