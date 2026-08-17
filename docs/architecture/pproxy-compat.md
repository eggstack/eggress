# eggress-pproxy-compat

`crates/eggress-pproxy-compat/`

pproxy 2.7.9 compatibility layer for CLI argument translation, URI parsing,
and parity classification. The oracle is frozen to tag commit
`09d4752f17ed6787e1a073c93980eec019887ee3` from
`qwj/python-proxy`; see the phase-0 manifest for source evidence.

This is an internal Rust crate, not a separately published Python package.
The Python compatibility surface is bundled in the `eggress` distribution as
`eggress.pproxy` for translation helpers, and a complete Phase 0 top-level
`pproxy` module namespace for the documented public factory/protocol/cipher,
metadata, entry-point, verbose, and system-proxy surfaces, and
a stable exception hierarchy (`PProxyCompatibilityError`,
`UnsupportedPProxyFeature`) for unsupported operations. The
upstream `pproxy` distribution must not be installed alongside Eggress.

## Key Types

| Type | Description |
|---|---|
| `PproxyArgs` | Parsed pproxy CLI arguments |
| `PproxyUri` | Parsed pproxy URI |
| `PproxyChain` | Parsed pproxy chain specification |
| `ManifestTier` | Parity tier classification |
| `StructuredDiagnostic` | JSON diagnostic output |
| `DiagnosticCode` | Stable diagnostic code enum |
| `CompatRegex` | pproxy-compatible regex parser |
| `PproxyRuleFile` | pproxy rule file parser |

## Translation Functions

| Function | Description |
|---|---|
| `translate_pproxy_args(args)` | Convert pproxy CLI args to TOML |
| `translate_from_uris(listen, remote)` | Convert pproxy URIs to TOML |

## Parity Classification

`ManifestTier` is the **single executable source of truth** for
compatibility tier classification. `manifest_tier_for_category()` maps
diagnostic warning categories to tiers, and `classify_unsupported_feature_tier()`
maps unsupported feature names to tiers. The canonical manifest and Python
compatibility layer must agree with these functions; a cross-check test in
`eggress-testkit` prevents reporter/manifest tier drift.

Five-tier vocabulary:
- `drop_in` — no warning expected
- `compatible_with_warning` — works but emits a diagnostic
- `native_equivalent` — outcome same as pproxy, different mechanism
- `intentional_non_parity` — flag parsed, no plan to implement
- `unsupported` — flag or feature not implemented

`classify_aggregate_tier()` picks the worst tier from a set of warnings
and unsupported features with this dominance order (worst first):
`unsupported` > `intentional_non_parity` > `compatible_with_warning` >
`native_equivalent` > `drop_in`.

The aggregate classifier consults the native per-diagnostic tier of every
unsupported feature id via `manifest_tier_for_unsupported_feature()`
(which reuses the per-diagnostic tier owned by
`classify_unsupported_feature_tier()`), so known intentional exclusions
(SSR listener/upstream, legacy Shadowsocks ciphers) report as
`intentional_non_parity` rather than being collapsed into generic
`unsupported`. SSH upstreams are now feature-gated compatibility support;
SSH listener use remains an upstream-only structured refusal. The old SSR
listener/upstream category names are retained only for diagnostic read
compatibility; new unsupported SSR output is limited to SSR UDP. Unknown
warning categories and unknown unsupported feature ids fail closed to
`unsupported`.

The classifier is the single executable source of truth for both
per-diagnostic and aggregate tier semantics. The Rust CLI
(`pproxy check`) and the Python `check_pproxy_args()` reporter both
consume the same native aggregate result via the `tier` property on
`PyTranslationResult`; Python does not maintain an independent tier
table, severity order, or intentional-exclusion set.

## Flag Mapping

| pproxy Flag | eggress TOML |
|---|---|
| `-l uri` | `[[listeners]]` with parsed URI |
| `-r uri` | `[[upstreams]]` and `[[upstream_groups]]` |
| `-ul uri` / `-ur uri` | Standalone UDP listener/upstream configuration |
| `-s` | Scheduler choice (`fa`, `rr`, `rc`, `lc`) |
| `-v` | Counted connection events; `-vv` also reports traffic totals |
| `-d` | Counted debug mode; compatibility task failures become visible errors |
| `--ssl` | TLS configuration |
| `-b regex` | Native reject rule |
| `-a seconds` | Native health interval with a compatibility warning |
| `--pac path` | PAC serving configuration at the supplied admin path, with a compatibility warning |
| `--get path,file` | Native admin static content |
| `--auth seconds` | Enables bounded source-IP authentication reuse for compatibility listeners |
| `--sys` | Applies the selected bound SOCKS5/HTTP listener through `eggress-system-proxy`, then restores prior settings |
| `--reuse` | SO_REUSEPORT before TCP bind where supported |
| `--daemon` | Parsed, then rejected before startup (Phase 9 daemonization remains out of scope) |
| `--test` | Native URL test for each remote, then exit before listener startup |

`-h/--help` and `--version` are parsed actions, not listener arguments. The
standalone binary, `eggress pproxy run`, and `python -m pproxy` use the same
Rust `PproxyArgs` action/value model. The Python entry point also passes the
compatibility runtime options for `--auth`, `--sys`, `-d`, and `-v` into the
native supervisor before waiting for SIGINT/SIGTERM.

The tagged parser does not declare `--log`, `-f/--config`, `--rulefile`,
positional URIs, or long listener aliases such as `--listen` and `--remote`.
Native Eggress configuration and migration-only translation helpers may accept
separate extension names, but the executable parser rejects them before any
listener, system-proxy, or runtime side effect.

The translator parses combined protocols, fragment auth, local binding, canonical
`tunnel{host:port}://listener`, `ws{host:port}://listener`, and
`wss{host:port}://listener` fixed targets, the retained legacy raw fixed-target
extension, canonical raw rule suffixes, and the bounded pproxy SSR plugin
list. Unknown plugins fail during URI parsing; plugin-bearing SSR
configuration requires the `pproxy-legacy` feature and unsupported contexts
fail before startup. Unsupported fields are reported after parsing. Common HTTP/SOCKS, HTTP-only,
AEAD Shadowsocks, H2, WS/WSS, raw, tunnel, and Unix-domain TCP upstream flows lower through the
same native URI/config path. H2 and WSS normalize to the native `+tls` form;
raw/tunnel brace-delimited targets become the native raw endpoint. Bounded
listener forms include TCP/UDP echo and fixed-target forwarding; Unix upstreams
are TCP-only and platform-gated.

Compatibility advanced listeners include `h2://listener` and fixed-target
`ws{host:port}://listener` / `wss{host:port}://listener` forms. H2 multiplexes
CONNECT streams; WS/WSS completes an HTTP upgrade and relays binary frames.
`--auth` is consulted by HTTP, SOCKS4, SOCKS5, H2, and WS handshakes when
listener credentials are configured. The cache is keyed only by normalized
peer IP, uses monotonic expiry, is bounded, and is never created for native
mode.

TCP fixed-target configuration remains on the listener's TCP field. UDP echo or
fixed-target mode is added only for an explicit `-ul` URI, so enabling one role
cannot erase the other. `httponly` is intentionally upstream-only and receives a
role-specific diagnostic when supplied as a listener.

Outbound local binds are carried into native per-connection socket options for
direct routes and the first supported upstream hop. Family mismatches fail
before connect, and Unix paths remain redacted in compatibility displays.

Compatibility routing follows pproxy 2.7.9's ordered `fa` model. A remote URI
query (`?rule=...` or a raw query suffix) becomes a route predicate matching
the requested hostname or decimal destination port. Ruled remotes are lowered
to deterministic one-member groups in declaration order; unruled remotes share
a final first-available group. When no predicate matches, the translator emits
a direct fallback. `-b {regex}` is a high-priority hostname block. Eggress's
extension bridge may also accept a non-braced `-b PATH` rule file, but that is
not an upstream `--rulefile` claim. Missing or malformed rule files fail
translation. Generated
compatibility rule IDs include the declaration index, source, and pattern for
`route explain`. Explicit `-s fa`, `rr`, `rc`, and `lc` values map to the native scheduler names; native TOML
groups retain their own configured defaults.

`--pac <path>`, `--get <path,file>`, and `--test <target>` are value-taking
options. PAC and valid static content use the existing admin server; malformed
or unreadable static-content values fail closed. Both the standalone `pproxy`
binary and `eggress pproxy run` pass the supplied URL-shaped test target to the
native upstream test command and do not print a startup banner or bind a
listener in test mode.

PAC and verbosity are supported with compatibility warnings. PAC is served at
the mapped Eggress admin route. `-d` and `-v` use argparse count semantics,
including short clusters; `RUST_LOG` controls the final tracing filter. The
Python process installs the same default filter only when no explicit
`RUST_LOG` is present and leaves an existing embedded subscriber untouched.

## Compatibility process lifecycle

The shared execution gate runs after parsing/translation and before temporary
configuration, system-proxy mutation, or runtime startup. It rejects unknown
options, unsupported features, and non-equivalent extensions. `--auth` uses
the oracle default of 2,592,000 seconds and is passed to the bounded source-IP
reuse cache only for authenticated compatibility listeners.

TCP sockets are created with SO_REUSEPORT before bind when `--reuse` is set;
Unix socket tests verify that two listeners can share an address. Unsupported
platforms return a clear startup error. Runtime shutdown preserves the existing
ordering: readiness false, listener stop, UDP closure, reverse/task
cancellation, connection drain, admin shutdown, and system-proxy rollback.

## Regex and rule trust model

pproxy compatibility regex and rule definitions are **trusted local
configuration**, not hostile network input. Patterns originate from
command-line arguments, configuration files, or rule files controlled
by the local operator. No unauthenticated network client can supply
an arbitrary compatibility regex pattern at runtime.

The fast `regex` backend is the default compilation path. When a
pattern uses Perl/Python-like constructs (look-around, backreferences),
compilation falls back to the `fancy_regex` backend. Pattern length
(`MAX_PATTERN_LEN = 4096` bytes) and rule count
(`MAX_RULE_ENTRIES = 10_000`) are enforced before compilation. The
`fancy_regex` backend includes a built-in backtrack limit (default
1,000,000 steps) that prevents catastrophic backtracking from
blocking indefinitely, but operators must not load untrusted rule sets.

Native Eggress routing rules (`host_regex`, `destination_port_regex`)
use the standard `regex::Regex` crate directly and do not invoke the
fancy fallback path. The `fancy_regex` backend is confined to pproxy
compatibility translation and rule-file validation.

After translation, rule-file patterns are lowered to native
`regex::Regex` in the generated TOML config. The runtime routing
engine matches against bounded destination attributes (hostname string,
decimal port string), not arbitrary network payload data.

## Phase 5 boundary decisions

macOS PF transparent-destination recovery remains intentional non-parity: it
requires privileged `/dev/pf` ioctl access and has no disposable test in the
current platform abstraction. Backward TLS and mixed reverse chains likewise
remain unsupported because they require composing TLS around reverse framing.

## Diagnostics

All translation produces structured diagnostics with stable codes:
```json
{
  "code": "UNSUPPORTED_FEATURE",
  "message": "Feature X is not supported",
  "severity": "error"
}
```

## Dependencies

- `eggress-uri` — URI parsing

See [overview.md](overview.md) for context.
