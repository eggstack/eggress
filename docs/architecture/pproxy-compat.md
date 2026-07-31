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
| `--pac` | PAC serving configuration; pproxy's flag takes no path argument |
| `--sys` | System proxy inspection |

The translator currently supports common HTTP/SOCKS and AEAD Shadowsocks
flows. H2, WS/WSS, raw, and tunnel remain native-runtime features rather than
complete compatibility-translator paths. Per-remote rule routing is not yet
complete; `--rulefile` only translates the supported simple rule subset.

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
