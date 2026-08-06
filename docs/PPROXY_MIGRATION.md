# Migrating from pproxy to Eggress

Eggress provides a pproxy compatibility layer that translates common pproxy invocations and URI shapes into native Eggress configuration. It is a migration surface, not strict full drop-in parity.

Install the single `eggress` distribution. Its wheel includes the bounded
top-level `pproxy` package; `from eggress import pproxy` remains the explicit
migration-helper path. Uninstall upstream `pproxy` first because both wheels
own the same import namespace.

## Quick Start

### Translate pproxy arguments to Eggress TOML

```bash
eggress pproxy translate -- -l socks5://127.0.0.1:1080 -r http://proxy:8080
```

### Check compatibility of pproxy arguments

```bash
eggress pproxy check -- -l socks5://127.0.0.1:1080 -r http://proxy:8080
```

### Run directly from pproxy-style arguments

```bash
eggress pproxy run -- -l socks5://127.0.0.1:1080 -r http://proxy:8080
```

## Supported URI Forms

| Scheme | As Local Listener | As Upstream |
|--------|------------------|-------------|
| `http://` | Yes | Yes |
| `https://` | Yes (TLS) | Yes (HTTP+TLS) |
| `socks4://` | Yes | Yes |
| `socks4a://` | Yes | Yes |
| `socks5://` | Yes | Yes |
| `trojan://` | No (upstream-only) | Yes |
| `shadowsocks://` | Yes (AEAD methods only) | Yes (AEAD methods only) |
| `direct://` | No | Yes (direct connection) |
| `h2://` | No | Yes, upstream only; TLS/ALPN is implied |
| `ws://` | No | Yes, upstream only |
| `wss://` | No | Yes, upstream only; lowered to `ws+tls://` |
| `raw://` | No | Yes, upstream only; endpoint is the fixed target |
| `tunnel://` | No | Yes, upstream-only alias for raw |

### URI Format

```
scheme://[user:pass@]host:port[+tls][?rule=regex]
```

### Examples

```bash
# Local SOCKS5 proxy on port 1080
-l socks5://127.0.0.1:1080

# Local HTTP proxy with authentication
-l http://admin:secret@0.0.0.0:8080

# Upstream through HTTP proxy
-r http://proxy.example:8080

# Upstream through SOCKS5 with TLS
-r socks5+tls://secure-proxy:1080

# Trojan upstream
-r trojan://password@server:443

# Chain: SOCKS5 through HTTP then SOCKS5
-r http://proxy1:8080 -r socks5://proxy2:1080
```

## Common pproxy Commands -> Eggress Equivalents

### pproxy

```bash
python3 -m pproxy -l socks5://127.0.0.1:1080 -r http://proxy:8080
```

### Eggress (pproxy-compatible)

```bash
eggress pproxy run -- -l socks5://127.0.0.1:1080 -r http://proxy:8080
```

### Eggress (native TOML)

```toml
version = 1

[[listeners]]
name = "local"
bind = "127.0.0.1:1080"
protocols = ["socks5"]

[[upstreams]]
id = "upstream"
uri = "http://proxy:8080"

[[upstream_groups]]
id = "chain"
scheduler = "first-available"
members = ["upstream"]
fallback = "reject"

[[rules]]
id = "default"
any = true
upstream_group = "chain"
```

## Supported Features

| Feature | Status | Notes |
|---------|--------|-------|
| HTTP CONNECT | Compatible | Byte-exact payload match with differential tests |
| HTTP forward proxy | Compatible | Persistent session model with HTTP/1.1 keep-alive (Phase 19) |
| SOCKS4/4a | Compatible | Differential tests with pproxy 2.7.9 added (Phase 19) |
| SOCKS5 CONNECT | Compatible | Expanded differential evidence: auth, IPv6, domain, refused targets (Phase 19) |
| SOCKS5 UDP ASSOCIATE | Supported | Framing differs; relay success matches |
| Standalone UDP (`-ul`/`-ur`) | Compatible | pproxy-compatible standalone UDP relay mode (Phase 20) |
| Shadowsocks upstream | Supported | Standard AEAD framing; interoperable with standard Shadowsocks |
| Trojan upstream | Partial | Client-only; no Trojan server |
| HTTP/2 CONNECT | Supported | Synthetic tests; H2 CONNECT server and client implemented (Phase 26) |
| WebSocket tunnel | Supported | Synthetic tests; WS/WSS tunnel server and client implemented (Phase 26) |
| Raw fixed-target tunnel | Supported | Synthetic tests; raw TCP tunnel with no protocol negotiation (Phase 26) |
| HTTP-only upstream (`httponly://`) | Supported | Existing HTTP forward path rewrites origin-form requests to absolute-form |
| Unix-domain TCP upstream | Supported on Unix | Tokio UnixStream; UDP and Windows are rejected |
| Echo endpoint | Supported | Explicit TCP/UDP loopback utility |
| Fixed-target UDP tunnel | Supported | One configured target with bounded packet relay; no multi-hop UDP claim |
| TLS ALPN | Supported | Configurable ALPN values for H2 and HTTP/1.1 (Phase 26) |
| Hot reload | Partial | Routing/upstreams only; listener topology requires restart |

## Unsupported Features

The following pproxy features are explicitly unsupported:

- **Trojan listeners** -- Trojan is supported for both inbound and upstream
- **`--daemon` mode** -- Use systemd or a process manager instead
- **`-d` debug** -- Enables debug-level diagnostics; equivalent to RUST_LOG=debug
- **`--ssl` TLS listeners** -- Configure TLS in eggress TOML directly
- **`-b` block regex rules** -- Use eggress TOML routing rules
- **`--rulefile`** -- simple reject/block entries are translated; use Eggress TOML routing rules for complete semantics
- **`--reuse`** -- SO_REUSEPORT on listener sockets (not connection pooling)
- **`--log`** -- Use `RUST_LOG=debug` environment variable
- **`--sys`** -- System proxy inspection supported; use `eggress system-proxy apply --apply` for mutation
- **Multi-hop UDP** -- Not supported
- **macOS PF transparent destination recovery** -- Intentional non-parity; requires privileged `/dev/pf` ioctl access
- **Backward TLS/mixed reverse chains** -- Intentional partial compatibility; reverse framing is not a normal chain stream
- **SSH protocol** -- Not supported (SSH transport is out-of-scope for a proxy)
- **H3/QUIC transport** -- Deferred; pproxy H3 behavior is experimental and unstable. See ADR at `docs/adr/ADR_quic_h3_pproxy_parity.md`.
- **Shadowsocks stream ciphers** -- Not supported (insecure; use AEAD methods). Detected during URI parsing; produces `LegacyMethodUnsupported` error. See `docs/adr/ADR_legacy_shadowsocks_ssr_compatibility.md`.
- **ShadowsocksR** -- Not supported (non-standard extension). `ssr://` URIs are recognized and rejected with structured `UnsupportedFeature` diagnostics (categories: `ssr-listener`, `ssr-upstream`). See `docs/adr/ADR_legacy_shadowsocks_ssr_compatibility.md`.

Unsupported features produce structured diagnostics when encountered in pproxy compat mode.

## Exit Codes

Eggress pproxy subcommands use granular exit codes to indicate failure classes:

| Code | Name | Meaning |
|------|------|---------|
| 0 | `success` | Command succeeded |
| 1 | `runtime_failure` | Runtime error (e.g. JSON serialization failure) |
| 2 | `cli_parse_error` | CLI argument parsing failed (unknown flags, bad syntax) |
| 3 | `config_validation` | Translated config failed validation |
| 4 | `bind_failure` | Could not bind to listen address |
| 5 | `unsupported_feature` | An unsupported pproxy feature was encountered |
| 6 | `platform_missing` | Required OS capability not available (e.g. Linux-only feature) |
| 7 | `external_dependency` | External dependency required but unavailable |
| 130 | `interrupted_by_sigint` | Process interrupted by SIGINT |
| 143 | `terminated_by_sigterm` | Process terminated by SIGTERM |

pproxy uses a generic exit code of `1` for all failures. Eggress provides
differentiated codes to enable scripted error handling.

`eggress pproxy check` always exits 0 regardless of compatibility findings —
it reports parity tiers without failing.

## The `--json` Flag

The `pproxy check` subcommand accepts `--json` for machine-readable output:

```bash
eggress pproxy check --json -- -l socks5://127.0.0.1:1080 -r http://proxy:8080
```

The JSON output includes:

- `tier` — overall compatibility tier (`compatible`, `supported`, `unsupported`)
- `diagnostics` — array of structured diagnostic objects (see below)
- `features` — per-feature info with name, tier, and diagnostic code
- `raw_args` — the original pproxy-style arguments
- `parsed_uris` — parsed listener and remote URIs (redacted)

The `route explain` and `upstream test` subcommands also support `--json`.

## Structured Diagnostics

When pproxy features are encountered during translation, eggress produces
structured diagnostics with stable codes, optional tier classification, and
actionable suggestions. Each diagnostic carries:

- `code` — stable `DiagnosticCode` (e.g. `unsupported_protocol`, `invalid_cipher_method`)
- `feature_id` — the pproxy feature name, if applicable
- `tier` — compatibility tier (`unsupported`, `partial`, `intentional_non_parity`)
- `message` — human-readable description
- `suggestion` — eggress-native alternative, if one exists

Example diagnostic codes:

| Code | Example trigger |
|------|----------------|
| `unsupported_protocol` | `ssh://` or unrecognized scheme |
| `unsupported_flag` | `--daemon`, `--reuse`, unknown flags |
| `unsupported_security_sensitive_legacy_feature` | SSR URIs (`ssr://`) |
| `invalid_cipher_method` | Legacy stream cipher (e.g. `aes-128-ctr`) |
| `invalid_uri_syntax` | Malformed URI or argument list |
| `invalid_chainComposition` | Conflicting protocol chain |
| `missing_target` | No `-l` argument provided |
| `missing_credential` | URI requires password but none given |
| `bind_failure` | Could not bind to address (port in use) |
| `privilege_capability_missing` | Linux-only feature on macOS |
| `external_dependency_missing` | Required external tool not found |

Diagnostics are produced by the `StructuredDiagnostic` type in
The internal `eggress-pproxy-compat` crate powers these diagnostics and they
are serializable to JSON. It is not a separate Python distribution.

The pproxy 2.7.9 CLI argument shapes are preserved: `--pac` takes a path,
`--test` takes a URL, and repeatable `--get` takes `PATH,FILE` values. The
translator consumes these values before processing positional arguments.

## Parity Tiers

When you run `eggress pproxy check`, it reports a parity tier:

- **Compatible** -- Full behavioral match with pproxy
- **Supported** -- Works correctly with minor warnings
- **Partial** -- Some features unsupported; service may not behave as expected

## Credential Handling

- Credentials in generated TOML are stored in plaintext (config file only)
- Credentials are **never** printed in warnings or error messages
- The `--annotate` flag adds comments but still redacts credentials in warnings

## Troubleshooting

### "unsupported protocol" error

Check that your URI scheme is one of: `http`, `socks4`, `socks5`, `trojan`.

### "no local listener specified"

You must provide at least one `-l` argument.

### Generated TOML doesn't validate

Run `eggress pproxy translate` and pipe to `eggress --config /dev/stdin` to test.
