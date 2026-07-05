# pproxy CLI Inventory

Comprehensive inventory of pproxy 2.7.9 CLI flags and invocation forms,
with eggress handling status and migration notes.

## 1. pproxy CLI Flags Inventory

### Core Listener/Upstream Flags

#### `-l` / `--listen` — TCP Listen URI(s)

| Property | Value |
|----------|-------|
| pproxy behavior | Bind to one or more TCP listener URIs. Supports `http://`, `socks4://`, `socks4a://`, `socks5://`, `ss://`, `trojan://`, `redir://`, `unix://`. Multiple `-l` flags bind multiple listeners. |
| Eggress handling | **Supported** — translates to `[[listeners]]` entries in TOML. Scheme maps to `protocols` field. Credentials embedded in URI become auth config. |
| Example | `pproxy -l socks5://127.0.0.1:1080` |

#### `-r` / `--remote` — TCP Remote/Upstream URI(s)

| Property | Value |
|----------|-------|
| pproxy behavior | Specify upstream proxy URIs. Supports `http://`, `socks4://`, `socks4a://`, `socks5://`, `ss://`, `trojan://`, `ssh://`, `direct://`. Multiple `-r` flags create upstream groups. Chaining via `__` separator. |
| Eggress handling | **Supported** — translates to `[[upstreams]]` and `[[upstream_groups]]`. Multiple remotes generate a group with round-robin (default for 2+) or first-available (single). SSH rejected. `__` jump chains rejected with diagnostic. |
| Example | `pproxy -l socks5://:1080 -r http://proxy:8080` |

#### `-ul` / `--udp-listen` — UDP Listen URI(s)

| Property | Value |
|----------|-------|
| pproxy behavior | Bind a standalone UDP relay socket. Accepts a URI or plain `host:port` / `:port` / port number. No TCP control connection required (standalone mode). |
| Eggress handling | **Supported** — generates `mode = "standalone_pproxy_udp"` config on the first listener. Accepts URI, `:port`, `host:port`, and plain port formats. If no compatible `-l` is present, adds a default SOCKS5 listener on `:1080`. |
| Example | `pproxy -l http://:8080 -ul socks5://:1081` |

#### `-ur` / `--udp-remote` — UDP Remote/Upstream URI(s)

| Property | Value |
|----------|-------|
| pproxy behavior | Specify upstream for UDP traffic relayed via `-ul`. Supports same schemes as `-r`. |
| Eggress handling | **Supported** — generates a UDP upstream group (`pproxy-udp-chain`) with a transport-matching rule (`transport = "udp"`). |
| Example | `pproxy -l http://:8080 -ul :1081 -ur socks5://proxy:1080` |

### Scheduling and Load Balancing

#### `-s` / `--scheduler` — Scheduler

| Property | Value |
|----------|-------|
| pproxy behavior | Set the load-balancing algorithm. Values: `rr` (round-robin, default), `fa` (first-available), `rc` (random-choice), `lc` (least-connections). |
| Eggress handling | **Supported** — maps to `scheduler` in upstream group TOML. Recognized values: `rr`/`round_robin` → `round-robin`, `fa`/`first_available` → `first-available`, `rc`/`random_choice` → `random-choice`, `lc`/`least_connection` → `least-connections`. Unrecognized values emit a warning and default to `first-available`. |
| Example | `pproxy -l socks5://:1080 -r a://host1 -r b://host2 -s rr` |

### Authentication

#### `-a` / `--alive` — Alive Check Interval

| Property | Value |
|----------|-------|
| pproxy behavior | Set alive check interval in seconds. pproxy probes upstreams periodically and removes failed ones temporarily. |
| Eggress handling | **Partial** — maps alive check interval to health probe config in TOML. The `-a <seconds>` value is translated to a health probe interval. |
| Example | `pproxy -l socks5://:1080 -r http://proxy:8080 -a 10` |

### TLS/SSL

#### `--ssl` — SSL Listener

| Property | Value |
|----------|-------|
| pproxy behavior | Enable TLS on the listener. Takes `certfile[,keyfile]` as value. Wraps the inbound connection in TLS. |
| Eggress handling | **Partial** — generates TLS listener config in TOML from `cert[,key]` value. The `--ssl` flag translates to a `listeners.tls` block with the provided certificate and key paths. |
| Example | `pproxy -l socks5://:1080 --ssl cert.pem,key.pem` |

### Traffic Filtering

#### `-b` / `--block` — Block Rules

| Property | Value |
|----------|-------|
| pproxy behavior | Block connections matching regex patterns against the target hostname. |
| Eggress handling | **Partial** — generates reject rules with host-regex matcher in TOML. The `-b <pattern>` flag is translated to a `[[rules]]` entry with `action = "reject"` and a `host_regex` matcher. |
| Example | `pproxy -l http://:8080 -b ".*\\.example\\.com"` |

### Routing Rules

#### `--rulefile` — Rule File

| Property | Value |
|----------|-------|
| pproxy behavior | Load routing rules from a file (line-based format with regex patterns and destination actions). |
| Eggress handling | **Partial** — parses line-based rulefile format; generates reject rules for simple patterns, warnings for complex rules. Simple host-regex patterns are translated to TOML `[[rules]]` entries; unsupported patterns emit a warning. |
| Example | `pproxy -l http://:8080 --rulefile rules.txt` |

### Process Management

#### `--daemon` / `-d` — Daemonize

| Property | Value |
|----------|-------|
| pproxy behavior | Fork into background and run as a daemon. |
| Eggress handling | **Unsupported** — emits unsupported feature diagnostic ("--daemon mode is not supported; use systemd or process manager"). Use systemd, supervisord, or another process manager. |
| Example | `pproxy -l socks5://:1080 -r direct --daemon` |

### Logging

#### `-v` / `--verbose` — Verbose Logging

| Property | Value |
|----------|-------|
| pproxy behavior | Enable verbose/debug logging output. |
| Eggress handling | **Partial** — emits a warning ("pproxy -v flag detected; set RUST_LOG=debug environment variable for equivalent behavior"). No flag-based logging control; use `RUST_LOG` environment variable. |
| Example | `pproxy -l socks5://:1080 -v` |

#### `--log` / `-log` — Log File

| Property | Value |
|----------|-------|
| pproxy behavior | Write log output to a file instead of stderr. |
| Eggress handling | **Partial** — emits diagnostic about tracing-subscriber; redirect stderr for file logging. The `--log` flag is acknowledged with a message explaining that eggress uses `tracing-subscriber` and stderr can be redirected for log file output. |
| Example | `pproxy -l socks5://:1080 --log access.log` |

### Connection Behavior

#### `--reuse` — Port Reuse

| Property | Value |
|----------|-------|
| pproxy behavior | Enable connection reuse / pooling for upstream connections. |
| Eggress handling | **Intentional non-parity** — emits unknown-flag warning. Connection pooling is not implemented; eggress uses one upstream connection per proxy session. This is a deliberate design choice. |
| Example | `pproxy -l socks5://:1080 -r http://proxy:8080 --reuse` |

### PAC and System Proxy

#### `--pac` — PAC File Serving

| Property | Value |
|----------|-------|
| pproxy behavior | Serve a PAC (Proxy Auto-Configuration) file for browser auto-configuration. |
| Eggress handling | **Partial** — emits diagnostic pointing to admin PAC serving. The `--pac` flag is acknowledged with a message directing users to configure PAC in the TOML `admin.pac` block. |
| Example | `pproxy -l http://:8080 --pac /path/to/proxy.pac` |

#### `--sys` — Set System Proxy

| Property | Value |
|----------|-------|
| pproxy behavior | Automatically configure system proxy settings (macOS/Windows). |
| Eggress handling | **Partial** — emits diagnostic pointing to `eggress system-proxy inspect`. The `--sys` flag is acknowledged with a message directing users to the dedicated system proxy inspection command. |
| Example | `pproxy -l http://:8080 --sys` |

#### `--get` — URL Fetch Helper

| Property | Value |
|----------|-------|
| pproxy behavior | Fetch a URL through the configured proxy (utility for testing). |
| Eggress handling | **Partial** — emits diagnostic pointing to `curl --proxy`. The `--get` flag is acknowledged with a message directing users to use curl with the `--proxy` flag for equivalent functionality. |
| Example | `pproxy -l http://:8080 --get http://example.com` |

### Testing

#### `--test` — Test Mode

| Property | Value |
|----------|-------|
| pproxy behavior | Test all remote proxies and exit. Verifies upstream connectivity. |
| Eggress handling | **Partial** — emits diagnostic pointing to `eggress upstream test -c config`. The `--test` flag is acknowledged with a message directing users to the equivalent eggress command with a config file argument. |
| Example | `pproxy -l http://:8080 -r http://proxy:8080 --test` |

### Config and Help

#### `-f` / `--config` — Config File

| Property | Value |
|----------|-------|
| pproxy behavior | Load configuration from a TOML/YAML/JSON config file (alternative to CLI flags). |
| Eggress handling | **Supported** — eggress uses `eggress --config path/to/config.toml` natively. Different schema. |
| Example | `pproxy -f /etc/pproxy/config.toml` |

#### `--version` — Version Display

| Property | Value |
|----------|-------|
| pproxy behavior | Print version information and exit. |
| Eggress handling | **Supported** — `eggress --version` shows version info. |
| Example | `pproxy --version` |

#### `--help` — Help Output

| Property | Value |
|----------|-------|
| pproxy behavior | Print usage information and exit. |
| Eggress handling | **Supported** — `eggress --help` shows help for all subcommands. |
| Example | `pproxy --help` |

### Positional Arguments

#### Positional URIs (alternative to `-l`/`-r`)

| Property | Value |
|----------|-------|
| pproxy behavior | First positional arg is treated as `-l` (listen), second as `-r` (remote). Subsequent positionals alternate. |
| Eggress handling | **Supported** — the compat parser handles positional args the same way. First positional → local, subsequent → remote. |
| Example | `pproxy socks5://127.0.0.1:1080 http://proxy:8080` |

## 2. Summary Table

| Flag | Aliases | pproxy Behavior | Eggress Status | Diagnostic |
|------|---------|-----------------|----------------|------------|
| `-l` | `--listen` | TCP listen URI | Compatible | — |
| `-r` | `--remote` | TCP upstream URI | Compatible | — |
| `-ul` | `--udp-listen` | UDP listen URI | Compatible | — |
| `-ur` | `--udp-remote` | UDP upstream URI | Compatible | — |
| `-s` | (none) | Scheduler algorithm | Compatible | Warning for unrecognized values |
| `-a` | (none) | Alive check interval | Partial | Maps to health probe config in TOML |
| `--ssl` | (none) | TLS listener cert/key | Partial | Generates TLS listener config in TOML |
| `-b` | (none) | Block regex rules | Partial | Generates reject rules with host-regex matcher |
| `--rulefile` | `-rulefile` | Rule file path | Partial | Parses rulefile; generates reject rules for simple patterns |
| `--daemon` | `-d` | Daemonize | Unsupported | Use systemd/process manager |
| `-v` | (none) | Verbose logging | Partial | Set `RUST_LOG=debug` |
| `--log` | `-log` | Log file path | Partial | Warning: use tracing-subscriber; redirect stderr |
| `--reuse` | (none) | Connection reuse | Intentional non-parity | Connection pooling not implemented |
| `--pac` | (none) | PAC file serving | Partial | Configure PAC in TOML admin.pac block |
| `--sys` | (none) | System proxy | Partial | Use eggress system-proxy inspect |
| `--get` | (none) | URL fetch | Partial | Use curl --proxy |
| `--test` | (none) | Test upstreams | Partial | Use eggress upstream test -c config |
| `-f` | `--config` | Config file | Supported | Different schema |
| `--version` | (none) | Version | Supported | — |
| `--help` | (none) | Help | Supported | — |
| Positional | — | Alt for -l/-r | Compatible | — |

## 3. Eggress CLI Structure

### Top-Level Commands

```
eggress [--config PATH] [-l URI...] [-r URI...] [OPTIONS]
eggress route <TARGET> [OPTIONS]
eggress upstream test [OPTIONS]
eggress pproxy translate [--annotate] -- <pproxy args...>
eggress pproxy check [--json] -- <pproxy args...>
eggress pproxy run [--log-format FORMAT] -- <pproxy args...>
```

### `eggress` (direct mode)

| Flag | Description |
|------|-------------|
| `--config PATH` | Load TOML config file |
| `-l URI`, `--listen URI` | TCP listen URI (repeatable) |
| `-r URI`, `--remote URI` | TCP upstream URI (repeatable) |
| `--log-format FORMAT` | Log format: `pretty` (default), `json`, `compact` |
| `--rules-file PATH` | Host-regex rules file (alternative to TOML rules) |

When `--config` is provided, `-l` and `-r` are rejected (mutually exclusive).

### `eggress route <TARGET>`

Explain which upstream a target would be routed to.

| Flag | Description |
|------|-------------|
| `-c`, `--config PATH` | TOML config file |
| `--listener NAME` | Listener name context |
| `--protocol PROTO` | Inbound protocol: `http`, `socks4`, `socks5` |
| `--json` | Output as JSON |
| `--admin URL` | Query a running eggress admin API instead of local config |

### `eggress upstream test`

Test upstream connectivity.

| Flag | Description |
|------|-------------|
| `-i`, `--id ID` | Test a specific upstream by ID |
| `-t`, `--target HOST:PORT` | Target address for proxy-mode test (default: `example.com:443`) |
| `-c`, `--config PATH` | TOML config file (required) |
| `--timeout SECS` | Connection timeout in seconds (default: 5) |
| `--mode MODE` | `proxy` (default) or `tcp` |
| `--json` | Output as JSON |

### `eggress pproxy translate`

Translate pproxy CLI arguments to eggress TOML configuration.

| Flag | Description |
|------|-------------|
| `--annotate` | Add explanatory comments to generated TOML |
| `--` | Separator before pproxy-style arguments |

**Example:**
```bash
eggress pproxy translate -- -l socks5://:1080 -r http://proxy:8080
eggress pproxy translate --annotate -- -l socks5://:1080 -r http://proxy:8080
```

### `eggress pproxy check`

Check pproxy arguments and report parity tier.

| Flag | Description |
|------|-------------|
| `--json` | Output as JSON with structured diagnostics |
| `--` | Separator before pproxy-style arguments |

**Example:**
```bash
eggress pproxy check -- -l socks5://:1080 -r http://proxy:8080
eggress pproxy check --json -- -l socks5://:1080 -r ssh://rejected
```

### `eggress pproxy run`

Translate pproxy arguments and start the service.

| Flag | Description |
|------|-------------|
| `--log-format FORMAT` | Log format: `pretty` (default), `json`, `compact` |
| `--` | Separator before pproxy-style arguments |

**Example:**
```bash
eggress pproxy run -- -l socks5://:1080 -r http://proxy:8080
```

## 4. Exit Code Reference

| Code | Constant | Meaning |
|------|----------|---------|
| 0 | `EXIT_SUCCESS` | Successful execution |
| 1 | `EXIT_RUNTIME_FAILURE` | Runtime error (failed to start, IO error, admin failure) |
| 2 | `EXIT_CLI_PARSE_ERROR` | CLI argument parse error (missing value, invalid URI, missing config) |
| 3 | `EXIT_CONFIG_VALIDATION` | Config file validation failed (load or compile error) |
| 4 | `EXIT_BIND_FAILURE` | Listener bind failure (address in use) |
| 5 | `EXIT_UNSUPPORTED_FEATURE` | Unsupported feature encountered during pproxy translation |
| 6 | `EXIT_PLATFORM_MISSING` | Platform capability missing (e.g., transparent proxy on non-Linux) |
| 7 | `EXIT_EXTERNAL_DEPENDENCY` | External dependency required but unavailable (e.g., pproxy for differential tests) |
| 130 | `EXIT_SIGINT` | Interrupted (SIGINT / Ctrl-C) |
| 143 | `EXIT_SIGTERM` | Terminated (SIGTERM) |

## 5. pproxy Chaining Syntax

pproxy supports chaining multiple upstream proxies with the `__` (double underscore) separator:

```bash
# Single-hop (equivalent to no chain)
pproxy -l http://:8080 -r direct://

# Two-hop chain: SOCKS5 → HTTP → direct
pproxy -l http://:8080 -r socks5://hop1:1080__http://hop2:8080__direct://

# Equivalent using multiple -r flags (for load balancing, not chaining)
pproxy -l http://:8080 -r socks5://hop1:1080 -r http://hop2:8080
```

Eggress handles these differently:
- **`__` separator** within a single `-r` value: parsed as a multi-hop chain via `ProxyHopSpec`.
- **Multiple `-r` flags**: parsed as separate upstreams in an upstream group with load balancing.

The pproxy compat layer rejects `__` jump chains in backward/upstream URIs with an
unsupported feature diagnostic ("backward-jump-chain"). Each hop must be a separate
`-r` argument for backward proxying.

## 6. pproxy Reverse Proxy URI Forms

pproxy supports reverse proxying via special URI forms:

| URI Scheme | Role | Description |
|------------|------|-------------|
| `bind://` | Acceptor | Listen on a port and accept connections |
| `listen://` | Acceptor | Alias for bind |
| `backward://` | Control client | Dial out to acceptor; receive streams |
| `rebind://` | Control client | Alias for backward |

The `+in` modifier on any protocol scheme activates reverse/backward mode:
```
scheme+in://[auth@]host:port
```

Multiple `+in` tokens stack for parallel connections:
```
socks5+in+in://acceptor:1080    # 2 parallel backward connections
```

Eggress translates these to `[[reverse_servers]]` (for bind/listen/backward/rebind listeners)
and `[[reverse_clients]]` (for backward/upstream with `+in` modifier). TLS on backward
connections (`+ssl`) is not supported; jump chains (`__`) are rejected.

## 7. Logging and Verbosity

### pproxy Logging Flags

#### `-v` / `--verbose`

pproxy's `-v` flag enables verbose/debug logging. When set, pproxy writes
detailed debug output to stderr including connection events, protocol
negotiation, and upstream health.

#### `--log FILE` / `-log FILE`

pproxy's `--log` flag redirects log output to a file instead of stderr.
Accepts a file path as value (e.g., `--log access.log`).

### Eggress Logging

#### `--log-format FORMAT`

Eggress natively supports `--log-format` with three values:

| Format | Behavior |
|--------|----------|
| `pretty` (default) | Compact human-readable output |
| `json` | Structured JSON log lines |
| `compact` | Compact single-line format |

#### `RUST_LOG` Environment Variable

Eggress uses `tracing-subscriber` with `EnvFilter`. The `RUST_LOG`
environment variable controls log verbosity directly:

```bash
RUST_LOG=info  eggress --config config.toml   # default behavior
RUST_LOG=debug eggress --config config.toml   # debug-level output
RUST_LOG=trace eggress --config config.toml   # trace-level (max verbosity)
```

Default level when `RUST_LOG` is unset: `info`.

### Compatibility Notes

| pproxy Flag | Eggress Equivalent | Notes |
|-------------|-------------------|-------|
| `-v` | `RUST_LOG=debug` | pproxy `-v` maps to debug-level tracing via a compat warning |
| `--log FILE` | Not supported | Eggress logs to stderr only; redirect with shell `>` if needed |
| (none) | `--log-format FORMAT` | Eggress-native format control (pretty/json/compact) |
| (none) | `RUST_LOG=<level>` | Standard Rust/tracing log level control |

**Credential safety**: Neither pproxy nor eggress log credentials at any
verbosity level. pproxy redacts `user:pass` in URI display; eggress uses
`****:****@` redaction in all log output. Debug-level tracing does not
expose authentication material.

**Migration path**: Replace `pproxy -v` with `RUST_LOG=debug` in
invocation scripts. For log-to-file behavior, use shell redirection:
`RUST_LOG=debug eggress --config config.toml > access.log 2>&1`.

## 8. Examples

### Common pproxy Invocations → Eggress Equivalents

**HTTP forward proxy:**
```bash
pproxy -l http://:8080 -r direct
# → eggress pproxy translate -- -l http://:8080 -r direct
```

**SOCKS5 proxy through HTTP upstream:**
```bash
pproxy -l socks5://:1080 -r http://proxy:8080
# → eggress pproxy translate -- -l socks5://:1080 -r http://proxy:8080
```

**SOCKS5 with auth:**
```bash
pproxy -l socks5://user:pass@:1080 -r direct
# → eggress pproxy translate -- -l socks5://user:pass@:1080 -r direct
```

**UDP relay alongside SOCKS5:**
```bash
pproxy -l http://:8080 -ul socks5://:1081 -ur socks5://proxy:1080
# → eggress pproxy translate -- -l http://:8080 -ul :1081 -ur socks5://proxy:1080
```

**Shadowsocks server:**
```bash
pproxy -l ss://aes-256-gcm:pass@:8388 -r direct
# → eggress pproxy translate -- -l ss://aes-256-gcm:pass@:8388 -r direct
```

**Round-robin load balancing:**
```bash
pproxy -l socks5://:1080 -r http://a:8080 -r socks5://b:1080 -s rr
# → eggress pproxy translate -- -l socks5://:1080 -r http://a:8080 -r socks5://b:1080 -s rr
```

**Transparent proxy (Linux):**
```bash
pproxy -l redir://0.0.0.0:8080 -r direct
# → eggress pproxy translate -- -l redir://0.0.0.0:8080 -r direct
```

**Unix domain socket listener:**
```bash
pproxy -l unix:///var/run/proxy.sock -r direct
# → eggress pproxy translate -- -l unix:///var/run/proxy.sock -r direct
```

**Reverse proxy (backward client):**
```bash
pproxy -l socks5://:1080 -r socks5+in://acceptor:8080
# → eggress pproxy translate -- -l socks5://:1080 -r socks5+in://acceptor:8080
```
