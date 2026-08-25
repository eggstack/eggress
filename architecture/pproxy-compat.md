# pproxy Compatibility Layer — Rust Crate + Python Distribution

Compatibility is treated as an evidence-backed contract, not a marketing claim.
This component owns argument/URI translation, tier classification, structured
diagnostics, and the fail-closed startup gate.

## Rust crate (`crates/eggress-pproxy-compat`)

| File | Role |
|---|---|
| `src/args.rs` | `PproxyArgs`: frozen pproxy 2.7.9 flag parser (`-l -r -ul -ur -b -a -s -d -v --ssl --pac --get --auth --sys --reuse --daemon --test --version --help`); strict violations for unknown flags/values |
| `src/uri.rs` | `PproxyUri`/`PproxyChain`/plugins — separate parser from native eggress grammar |
| `src/translate.rs` | `translate_pproxy_args()` / `translate_from_uris()` → TOML + warnings + unsupported list |
| `src/tier.rs` | Five-tier vocabulary: drop_in · compatible_with_warning · native_equivalent · intentional_non_parity · unsupported; aggregate classification |
| `src/diagnostics.rs` | Stable `DiagnosticCode` enum + JSON `StructuredDiagnostic` output |
| `src/gate.rs` | `ExecutionGate`: unknown flags ⇒ exit 2, unsupported features ⇒ exit 5 — startup never silently degrades |
| `src/regex_compat.rs` | pproxy line-based rule-file parsing shared conceptually with routing's `CompatRegexRule` |

## Python distribution (`python-pproxy-compat/`)

setuptools package named `eggress-pproxy-compat` that OWNS the top-level
`pproxy` namespace by mapping to `../python/pproxy/` (shim modules re-exporting
from `eggress.*`). Depends on `eggress==<same version>` + `cryptography`.

Hard rules:
- MUST NOT be installed beside upstream `pproxy` (same import namespace).
- The canonical `eggress` wheel must never install `pproxy`.
- Version moves in lockstep with the workspace (tag/version mismatch fails CI).

## Review entry points

- Golden translation tests + differential suites live in eggress-cli tests;
  manifest contracts in `docs/parity/*.toml` are the source of truth for what
  each tier claims. Update claims only with oracle evidence.

- Verify: `cargo test -p eggress-pproxy-compat`
