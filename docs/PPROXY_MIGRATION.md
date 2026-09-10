# Migrating from pproxy to Eggress

Eggress provides a pproxy compatibility layer that translates common pproxy invocations and URI shapes into native Eggress configuration. It is a migration surface, not strict full drop-in parity.

The compatibility oracle is pinned to `pproxy==2.7.9` at commit
`09d4752f17ed6787e1a073c93980eec019887ee3`. Per-feature compatibility truth
lives in the canonical manifest
(`docs/parity/pproxy_capability_manifest.toml`) and the maintained human
matrix (`docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`). This guide
describes migration mechanics and important boundaries only; it does not
maintain a second exhaustive supported/unsupported inventory. Where a feature
example is needed below, consult the matrix/manifest for the authoritative
status.

Install the `eggress` distribution. `from eggress import pproxy` is the explicit
migration-helper path. For a bounded top-level `pproxy` import, additionally
install the optional `eggress-pproxy-compat` distribution from a repository
checkout (`pip install ./python-pproxy-compat`). Uninstall upstream `pproxy`
first because it owns the same import namespace as the compatibility
distribution.

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

## URI and chain mechanics

pproxy URIs follow `scheme://[user:pass@]host:port[+tls][?rule=regex]`, and
multi-hop chains join hops with `__` (double underscore, left-to-right with
the first hop nearest). Semicolon and comma are rejected with a structured
diagnostic suggesting `__`.

The translator accepts the common listener/upstream URI shapes (HTTP,
SOCKS4/4a, SOCKS5, Shadowsocks, Trojan, direct, H2/WS/raw fixed-target forms,
and reverse `bind`/`listen`/`backward`/`rebind` with `+in`) and lowers them to
native `[[listeners]]` / `[[upstreams]]` TOML. Role support (listener vs
upstream), TLS modifiers, and optional transports differ per scheme; see the
[compatibility matrix](parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md) and
the [capability manifest](parity/pproxy_capability_manifest.toml) for the
authoritative per-scheme status rather than a duplicated table here.

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

## Compatibility boundaries (summary)

Native Eggress capability does not automatically imply exact pproxy
compatibility. The matrix/manifest record the authoritative per-feature tier;
the notes below call out the boundaries users most often misread:

- **Trojan** — client (upstream) and server (listener) roles are both
  implemented; see `uri.scheme_trojan` in the manifest.
- **`--daemon`** — supported with a warning behind the opt-in Linux
  `pproxy-daemon` feature (safe re-exec after validation); feature-off or
  non-Linux builds fail closed with a structured diagnostic. It is distinct
  from `-d`/debug.
- **`--sys`** — supported with a warning in compatibility mode: after
  listeners bind, the runtime applies the selected local SOCKS5 (or HTTP
  fallback) listener through the existing system-proxy backend and restores
  prior settings on shutdown or failed startup.
- **`-d`, `-v`, `--ssl`, `-b`, `--reuse`, `--pac`, `--get`, `--auth`,
  `--test`** — parsed with compatibility or native-equivalent semantics
  (tracing defaults, TLS config, reject rules, `SO_REUSEPORT`, admin
  PAC/static content, bounded auth reuse, in-process upstream test). See
  `cli.*` entries in the manifest for the exact tier.
- **SSH** — upstream-only behind the opt-in `ssh` feature; listeners are
  refused. Host-key acceptance is warning-bearing to match pproxy's
  permissive behavior.
- **H3/QUIC** — behind the opt-in `quic` feature; `h3://` is HTTP/3 CONNECT
  and `quic+http://` is raw QUIC streams. Listeners require certificate/key
  material; UDP association mode is an explicit unsupported composition.
- **Shadowsocks legacy/SSR** — modern AEAD is the default path. Legacy
  stream ciphers and OTA are behind the explicit `legacy-crypto` feature;
  SSR TCP framing plus the six built-in plugins are behind the opt-in
  `pproxy-legacy` feature (not in the default CLI `full`). UDP SSR and
  external/SIP003 plugins remain unsupported; `cast5-cfb`, `idea-cfb`,
  `rc2-cfb`, and `seed-cfb` are intentional non-parity.
- **Platform boundaries** — Linux `redir://` and Unix `unix://` apply where
  the OS facility exists; macOS PF transparent recovery is intentional
  non-parity. Listener topology is not hot-reloaded; routing/upstreams may
  be replaced atomically.

The default CLI `full` feature group intentionally does not enable every
optional legacy/transport feature above. Build with the documented opt-in
features when the migrated deployment needs them.

Unsupported transports or roles fail with structured, actionable diagnostics
rather than silent fallback.

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

- `tier` — overall compatibility tier using the canonical five-level
  vocabulary (`drop_in`, `compatible_with_warning`, `native_equivalent`,
  `intentional_non_parity`, `unsupported`; see
  `docs/parity/README.md`)
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
- `tier` — compatibility tier from the canonical five-level vocabulary
- `message` — human-readable description
- `suggestion` — eggress-native alternative, if one exists

Example diagnostic codes (non-exhaustive; the manifest is authoritative):

| Code | Example trigger |
|------|----------------|
| `unsupported_protocol` | `ssh://` as listener, or unrecognized scheme |
| `unsupported_flag` | `--daemon` without the opt-in feature, unknown flags |
| `unsupported_security_sensitive_legacy_feature` | SSR URIs (`ssr://`) without `pproxy-legacy` |
| `invalid_cipher_method` | Legacy stream cipher without `legacy-crypto` |
| `invalid_uri_syntax` | Malformed URI or argument list |
| `invalid_chainComposition` | Conflicting protocol chain |
| `missing_target` | No `-l` argument provided |
| `missing_credential` | URI requires password but none given |
| `bind_failure` | Could not bind to address (port in use) |
| `privilege_capability_missing` | Linux-only feature on macOS |
| `external_dependency_missing` | Required external tool not found |

Diagnostics are produced by the `StructuredDiagnostic` type in the internal
`eggress-pproxy-compat` crate and are serializable to JSON. The Rust crate is
not a separate Python distribution.

The pproxy 2.7.9 CLI argument shapes are preserved: `--pac` takes a path,
`--test` takes a URL, and repeatable `--get` takes `PATH,FILE` values. The
translator consumes these values before processing positional arguments.
`--get` serves the supplied file as native admin static content; malformed or
unreadable values fail closed.

## Parity Tiers

When you run `eggress pproxy check`, it reports a tier from the canonical
five-level vocabulary (`drop_in`, `compatible_with_warning`,
`native_equivalent`, `intentional_non_parity`, `unsupported`). Tier
semantics are defined in `docs/parity/README.md` and enforced by
`eggress-pproxy-compat::tier`; this guide does not maintain a separate tier
table.

## Credential Handling

- Credentials in generated TOML are stored in plaintext (config file only)
- Credentials are **never** printed in warnings or error messages
- The `--annotate` flag adds comments but still redacts credentials in warnings

## Troubleshooting

### "unsupported protocol" error

Check the [compatibility matrix](parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md)
for the scheme/role inventory; common listener/upstream schemes include
`http`, `socks4`, `socks5`, `trojan`, Shadowsocks AEAD, direct, and the
opt-in H2/WS/raw/SSH/SSR forms.

### "no local listener specified"

You must provide at least one `-l` argument.

### Generated TOML doesn't validate

Run `eggress pproxy translate` and pipe to `eggress --config /dev/stdin` to test.
