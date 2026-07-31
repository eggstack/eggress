# eggress-pproxy-compat

`crates/eggress-pproxy-compat/`

pproxy 2.7.9 compatibility layer for CLI argument translation, URI parsing, and parity classification.

This is an internal Rust crate, not a separately published Python package.
The Python compatibility surface is bundled in the `eggress` distribution as
`eggress.pproxy`; the wheel does not install top-level `pproxy`.

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

`ManifestTier` classifies features:
- `compatible` — matched for the defined scenario
- `supported` — usable with documented differences
- `partial` — partially implemented
- `intentional_non_parity` — deliberate divergence
- `experimental` — unstable
- `unsupported` — not implemented

## Flag Mapping

| pproxy Flag | eggress TOML |
|---|---|
| `-l uri` | `[[listeners]]` with parsed URI |
| `-r uri` | `[[upstreams]]` and `[[upstream_groups]]` |
| `-s` | Server mode |
| `-v` | Verbose logging |
| `--ssl` | TLS configuration |
| `-b addr` | Bind address |
| `--rulefile path` | Rule file parsing |
| `--pac path` | PAC serving configuration at the supplied admin path |
| `--sys` | System proxy inspection |

The translator parses combined protocols, fragment auth, local binding, fixed
targets, plugins, and canonical raw rule suffixes without discarding them.
Unsupported fields are reported after parsing. Common HTTP/SOCKS, AEAD
Shadowsocks, H2, WS/WSS, raw, and tunnel upstream flows all lower through the
same native URI/config path. H2 and WSS normalize to the native `+tls` form;
raw/tunnel brace-delimited targets become the native raw endpoint. These
transports are upstream-only and TCP-only in the compatibility surface, so
listener and UDP uses receive `unsupported_role` diagnostics.

Compatibility routing follows pproxy 2.7.9's ordered `fa` model. A remote URI
query (`?rule=...` or a raw query suffix) becomes a route predicate matching
the requested hostname or decimal destination port. Ruled remotes are lowered
to deterministic one-member groups in declaration order; unruled remotes share
a final first-available group. When no predicate matches, the translator emits
a direct fallback. `-b {regex}` is a high-priority hostname block, while a
non-braced `-b PATH` and `--rulefile PATH` load pproxy's plain regex-line file
format. Missing or malformed rule files fail translation. Generated
compatibility rule IDs include the declaration index, source, and pattern for
`route explain`. Explicit `-s fa`, `rr`, `rc`, and `lc` values map to the native scheduler names; native TOML
groups retain their own configured defaults.

`--pac <path>`, `--get <path,file>`, and `--test <target>` are value-taking
options. PAC and static content use the existing admin server, while test mode
passes the target to the existing upstream test command.

## Diagnostics

All translation produces structured diagnostics with stable codes:
```json
{
  "code": "UNSUPPORTED_FEATURE",
  "message": "Feature X is not supported",
  "severity": "warning"
}
```

## Dependencies

- `eggress-uri` — URI parsing

See [overview.md](overview.md) for context.
