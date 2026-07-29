# eggress-pproxy-compat

`crates/eggress-pproxy-compat/`

pproxy 2.7.9 compatibility layer for CLI argument translation, URI parsing, and parity classification.

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
- `compatible` — full behavioral match
- `supported` — works with minor differences
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
| `--pac` | PAC configuration |
| `--sys` | System proxy inspection |

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
