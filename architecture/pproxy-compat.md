# pproxy Compatibility Layer — Rust Crate + Python Distribution

Compatibility is treated as an evidence-backed contract, not a marketing claim.
This component owns argument/URI translation, tier classification, structured
diagnostics, and the fail-closed startup gate.

## Layout / module map

### Rust crate (`crates/eggress-pproxy-compat/src/`)

| File | Role |
|---|---|
| `lib.rs` | Public re-exports: `PproxyArgs`, `translate_pproxy_args`, `translate_from_uris`, `classify_aggregate_tier`, `evaluate_execution_gate`, `ManifestTier`, `DiagnosticCode`, `StructuredDiagnostic`, `CompatRegex`, `PproxyRuleFile` |
| `args.rs` | `PproxyArgs`: frozen pproxy 2.7.9 flag parser; strict violations for unknown flags/values |
| `uri.rs` | `PproxyUri`/`PproxyChain`/`PproxyPluginSpec` — separate parser from native eggress grammar |
| `translate.rs` | `translate_pproxy_args()` / `translate_from_uris()` -> TOML + warnings + unsupported list |
| `tier.rs` | `ManifestTier` enum (5 variants) + `classify_aggregate_tier` + `manifest_tier_for_category` |
| `diagnostics.rs` | `DiagnosticCode` enum (26 variants), `StructuredDiagnostic` JSON output, `classify_unsupported_feature_tier` |
| `gate.rs` | `ExecutionGate` / `BlockReason` — fail-closed startup gate |
| `warnings.rs` | `CompatWarning`, `UnsupportedFeature`, `TranslationOutput` |
| `exit_codes.rs` | 10 stable exit codes (0-7, 130, 143) |
| `error.rs` | `CompatError` enum |
| `regex_compat.rs` | `CompatRegex`, `PproxyRuleFile`, `RegexBackend` — pproxy line-based rule-file parsing |
| `diagnose.rs` | Diagnostic helpers |
| `tests.rs` | Integration tests |

### Five-tier vocabulary (`tier.rs`)

```
ManifestTier::DropIn                   = "drop_in"
ManifestTier::CompatibleWithWarning    = "compatible_with_warning"
ManifestTier::NativeEquivalent         = "native_equivalent"
ManifestTier::IntentionalNonParity     = "intentional_non_parity"
ManifestTier::Unsupported              = "unsupported"
```

Aggregate classification (`classify_aggregate_tier` at `tier.rs:103`) picks the
worst tier from all warnings and unsupported features. Severity order:
unsupported > intentional_non_parity > compatible_with_warning >
native_equivalent > drop_in. Unknown categories/features fail closed to
`Unsupported`.

### Stable exit codes (`exit_codes.rs`)

| Code | Name | Meaning |
|---|---|---|
| 0 | `success` | Clean startup |
| 1 | `runtime_failure` | Runtime error |
| 2 | `cli_parse_error` | Unknown flag or malformed argument |
| 3 | `config_validation` | TOML validation failed |
| 4 | `bind_failure` | Could not bind listener |
| 5 | `unsupported_feature` | Unsupported feature detected |
| 6 | `platform_missing` | Platform capability missing |
| 7 | `external_dependency` | External dependency unavailable |
| 130 | `interrupted_by_sigint` | SIGINT received |
| 143 | `terminated_by_sigterm` | SIGTERM received |

### Execution gate (`gate.rs`)

`evaluate(args, output)` at `gate.rs:56` combines:
1. Parser-side unknown flags (`args.strict_parser_violations()`) -> `BlockReason::UnknownFlag`
2. Translator-side unsupported features (`output.unsupported`) -> `BlockReason::Unsupported`
3. Benign warnings (`output.warnings`) -> no block

`ExecutionGate.allows_start()` returns `false` if any blocker exists.
The CLI binary and `eggress pproxy run` both apply this gate before startup.

### Diagnostic codes (`diagnostics.rs`)

26 stable `DiagnosticCode` variants (snake_case serialized):
`unsupported_protocol`, `unsupported_transport_wrapper`, `unsupported_flag`,
`unsupported_platform`, `unsupported_security_sensitive_legacy_feature`,
`invalid_uri_syntax`, `invalid_chain_composition`, `missing_target`,
`missing_credential`, `invalid_cipher_method`, `bind_failure`,
`privilege_capability_missing`, `external_dependency_missing`,
`rulefile_error`, `invalid_regex_pattern`, `fancy_regex_backend`,
`uri_preserved_unsupported_component`, `h2_handshake_failure`,
`h2_connect_rejected`, `h2_stream_reset`, `h2_goaway_received`,
`h2_pool_exhausted`, `h2_flow_control_stall`, `h2_auth_failure`,
`h2_unsupported_cleartext`, `h2_tls_alpn_mismatch`.

Each `StructuredDiagnostic` carries: `code`, optional `feature_id`,
optional `tier`, `message`, optional `suggestion`.

### Unsupported feature classification (`diagnostics.rs:382`)

`classify_unsupported_feature` maps feature strings to
`(DiagnosticCode, tier, suggestion)` triples. Key mappings:

| Feature | Code | Tier |
|---|---|---|
| `ssh-listener`, `ssh-upstream` | `unsupported_protocol` | `intentional_non_parity` |
| `ssr-listener`, `ssr-upstream`, `ssr-udp` | `unsupported_security_sensitive_legacy_feature` | `intentional_non_parity` |
| `daemon` | `unsupported_flag` | `unsupported` |
| `legacy-cipher` | `invalid_cipher_method` | `unsupported` |
| `socks4-bind`, `socks5-bind` | `unsupported_protocol` | `unsupported` |
| `system-proxy`, `auth-timeout` | `unsupported_flag` | `compatible_with_warning` |

## How it works

1. **Argument parsing**: `PproxyArgs::parse()` freezes the pproxy 2.7.9 CLI
   grammar. Unknown flags are collected as `strict_parser_violations()`.

2. **Translation**: `translate_pproxy_args()` converts parsed args to eggress
   TOML. `translate_from_uris()` does the same from pre-parsed URI objects.
   Both produce `TranslationOutput { toml, warnings, unsupported }`.

3. **Gate evaluation**: `evaluate_execution_gate()` combines parser violations
   and unsupported features into an `ExecutionGate`. The gate is checked
   before any runtime startup, system modification, or temp config creation.

4. **Tier classification**: `classify_aggregate_tier()` picks the worst tier
   from all diagnostics. The `PyTranslationResult.tier` getter in the Python
   bindings calls this function directly.

5. **Python facade**: `eggress.pproxy.TranslationResult` wraps
   `PyTranslationResult` and exposes `.tier`, `.ok`, `.toml`, `.warnings`,
   `.unsupported`. `CompatibilityReport` aggregates diagnostics, parsed URIs,
   and feature info for `check_pproxy_args()`.

## Namespace / boundary rules

### Rust crate

- Public API is explicitly re-exported in `lib.rs` — no wildcard imports.
- `PproxyArgs`, `translate_pproxy_args`, `classify_aggregate_tier` are the
  primary entry points consumed by `eggress-embed`, `eggress-cli`, and
  `eggress-python`.

### Python distribution (`python-pproxy-compat/`)

- `pyproject.toml` declares setuptools with `package-dir = {pproxy = "../python/pproxy"}`.
- The distribution **owns** the top-level `pproxy` namespace, mapping it to
  `../python/pproxy/` (shim modules re-exporting from `eggress.*`).
- Depends on `eggress==<same version>` + `cryptography>=42,<47`.
- **MUST NOT** be installed alongside upstream `pproxy` (same import namespace).
- The canonical `eggress` wheel must never install `pproxy`.
- Version moves in lockstep with the workspace (tag/version mismatch fails CI).

## Verification workflow

```bash
# Rust crate tests
cargo test -p eggress-pproxy-compat

# Python compat tests
python3 -m venv .venv
.venv/bin/python -m pip install "maturin>=1.0,<2.0" pytest "pytest-asyncio>=0.23,<1" "cryptography>=42,<47"
(cd crates/eggress-python && ../../.venv/bin/maturin develop)
.venv/bin/python -m pip install --no-deps ./python-pproxy-compat
.venv/bin/python -m pytest tests/compat -q

# Differential/oracle (opt-in, requires pproxy==2.7.9)
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 \
  cargo test -p eggress-cli --test differential_pproxy -- --ignored --test-threads=1
```

## Test coverage map

| Location | What it covers |
|---|---|
| `crates/eggress-pproxy-compat/src/tests.rs` | Translation correctness, gate evaluation, tier classification |
| `crates/eggress-pproxy-compat/src/tier.rs` (inline tests) | Aggregate tier logic, category/feature mapping |
| `crates/eggress-pproxy-compat/src/diagnostics.rs` (inline tests) | Diagnostic code display, JSON serialization, redaction |
| `crates/eggress-pproxy-compat/src/gate.rs` (inline tests) | Gate evaluation, blocker summary |
| `tests/compat/test_pproxy_api_contract.py` | API contract validation against extracted pproxy 2.7.9 contract |
| `tests/compat/fixtures/` | URI corpus, CLI cases, API snapshots, behavioral docs |
| `python/tests/test_pproxy_*.py` | Translation, diagnostics, differential, drop-in, redaction |
| `compat/pproxy-2.7.9/` | Frozen oracle assets (provenance, hashes, known defects, baselines) |
| `docs/parity/pproxy_capability_manifest.toml` | Canonical tier contract |
| `docs/parity/pproxy_2_7_9_strict_manifest.toml` | Strict behavioral contract |

## Reviewer gotchas

- `manifest_tier_for_category` at `tier.rs:39` defaults unknown categories to
  `Unsupported` to surface new gaps. Do not add catch-all branches.
- `classify_aggregate_tier` at `tier.rs:103` consults per-diagnostic native
  tiers for unsupported features. SSH/SSR report as `intentional_non_parity`,
  not generic `unsupported`.
- `ExecutionGate` does NOT use exit codes directly — the CLI entry point
  maps `BlockReason` to `exit_codes.rs` constants.
- `validate_pproxy_args` (Python binding) validates the frozen parser contract
  without starting a service; `check_pproxy_args` does full translation.
- The `pproxy.server.Server` / `pproxy.server.Connection` in the shim
  distribution are URI factories (`proxies_by_uri`), NOT lifecycle managers.
  `eggress.pproxy.Server` is the lifecycle class.

## See also

- [python-bindings.md](python-bindings.md) — Python bindings architecture
- [testing-and-tooling.md](testing-and-tooling.md) — test infrastructure
- `docs/parity/pproxy_capability_manifest.toml` — canonical tier contract
- `docs/parity/pproxy_2_7_9_strict_manifest.toml` — strict behavioral contract
- `docs/PPROXY_PARITY_SPEC.md` — compatibility vocabulary and tier definitions
- `python-pproxy-compat/pyproject.toml` — distribution configuration
