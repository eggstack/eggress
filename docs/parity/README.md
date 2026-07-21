# pproxy Parity Manifest

This directory contains the **authoritative compatibility contract** for
eggress's pproxy parity. The manifest and validator replace the ad-hoc
capability claims previously spread across README tables, CLI inventory
docs, and test comments.

## Files

| File | Purpose |
|------|---------|
| `pproxy_capability_manifest.toml` | Machine-readable manifest of all pproxy capabilities |
| `pproxy_2_7_9_strict_manifest.toml` | **Strict behavioral manifest** (Milestone A) — 194 capabilities validated by paired oracle/candidate testing |
| `PPROXY_COMPATIBILITY_POLICY.md` | **Compatibility vocabulary and governance rules** (Milestone A) |
| `PPROXY_2_7_9_STRICT_REPORT.md` | Generated strict report (from strict manifest) |
| `composition_schema.toml` | Schema definition for the composition matrix |
| `composition_matrix.toml` | Machine-readable composition graph (protocol×role×traffic_kind) |
| `PPROXY_PARITY_REPORT.md` | Human-readable summary with tier counts and next steps |
| `README.md` | This file — explains tiers, layers, evidence, and rules |

The validator lives at `scripts/validate_pproxy_parity_manifest.py`.

## Two Manifest Systems

This directory contains two manifest systems with different purposes:

### Canonical Capability Manifest (`pproxy_capability_manifest.toml`)
- **Scope**: All pproxy capabilities, including native-equivalent and migration-compatible
- **Use case**: Feature planning, development tracking, release notes
- **Tiers**: drop_in, compatible_with_warning, native_equivalent, intentional_non_parity, unsupported
- **Validated by**: `scripts/validate_pproxy_parity_manifest.py` (14 rules)

### Strict Behavioral Manifest (`pproxy_2_7_9_strict_manifest.toml`) — Milestone A
- **Scope**: Behaviorally testable capabilities against pproxy==2.7.9
- **Use case**: Release certification, differential evidence tracking
- **Status vocabulary**: gap, drop_in, known_upstream_defect, platform_constraint, not_applicable
- **Validated by**: `eggress-testkit::strict_manifest` (6 rules, 28 tests)
- **Comparators**: `eggress-testkit::strict_comparators` (11 comparators, 44 tests)
- **Policy**: `PPROXY_COMPATIBILITY_POLICY.md` — governing rules for compatibility claims

The strict manifest is the source of truth for release certification.
The canonical capability manifest remains the source of truth for feature planning.

## Manifest Schema

Each entry represents one pproxy capability at a granular level.

```toml
[[capability]]
id = "cli.listen"
category = "cli"
pproxy_surface = "-l / --listen"
pproxy_behavior = "Bind one or more TCP listener URIs."
eggress_behavior = "Translates to listener config and runs through supervisor."
tier = "drop_in"
parser = "complete"
translator = "complete"
config = "complete"
runtime = "complete"
cli = "complete"
python = "not_applicable"
docs = "complete"
evidence = "integration"
caveat_class = "protocol_crate_only"
```

## Tiers

| Tier | Meaning | Promotion criteria |
|------|---------|-------------------|
| `drop_in` | Drop-in replacement for this pproxy feature | All required layers `complete`; evidence ≥ integration |
| `compatible_with_warning` | Works but emits a diagnostic or differs in a known way | Diagnostic code or migration note required |
| `native_equivalent` | Achieves the same outcome through a different mechanism | Rationale not required (the equivalence *is* the rationale) |
| `intentional_non_parity` | Deliberately not replicated | Explicit rationale required |
| `unsupported` | Not implemented | — |

## Layers

Each capability reports implementation status across seven layers:

| Layer | What it covers |
|-------|---------------|
| `parser` | pproxy URI/arg parsing in `eggress-pproxy-compat` |
| `translator` | Translation from parsed pproxy args to eggress TOML |
| `config` | Config compiler accepting translated TOML |
| `runtime` | Runtime execution (listener, connector, chain, relay) |
| `cli` | CLI binary recognizing and processing the flag/feature |
| `python` | Python bindings exposing the feature |
| `docs` | Documentation of the feature and migration path |

Layer values: `complete`, `partial`, `not_started`, `not_applicable`, `refused`.

`refused` means eggress intentionally does not implement this layer for
this capability (e.g., `runtime = "refused"` for `--daemon`).

## Evidence Levels

| Evidence | Meaning |
|----------|---------|
| `differential` | Tested against live pproxy with behavioral comparison |
| `integration` | End-to-end test in eggress (no live pproxy) |
| `unit` | Unit test only (parser, codec, translation) |
| `synthetic` | Implemented but only tested in isolation |
| `docs_only` | Documented as unsupported; no code path |
| `none` | No tests |

## Caveat Classification

Optional `caveat_class` field on manifest entries classifies why a capability
with `refused` layers cannot achieve `drop_in`. Used by the report generator
to produce accurate caveat sections.

| Value | Meaning |
|-------|---------|
| `protocol_crate_only` | Parser/protocol crate exists, but config/runtime refuses it |
| `missing_protocol_command` | Protocol exists but a command/mode is missing (e.g., SOCKS BIND) |
| `missing_protocol_role` | Protocol client exists but server/listener role is missing (e.g., Trojan server) |
| `missing_protocol_transport` | Transport implementation needed (e.g., SSH) |
| `deferred_by_adr` | Deliberately deferred by Architecture Decision Record |
| `intentional_non_parity` | Deliberate non-parity with rationale |
| `cli_process_model` | CLI/process behavior limitation (e.g., daemon mode) |
| `translator_scope_gap` | Translator/rule compatibility gap (e.g., full rulefile parity) |

## Validation Rules

The validator (`scripts/validate_pproxy_parity_manifest.py`) enforces 14
rules (Phase 37 + Phase 42 + caveat classification). Errors block CI; warnings are advisory (or
errors in `--strict` mode).

| # | Rule | Severity |
|---|------|----------|
| 1 | Unknown tier/layer/evidence value | ERROR |
| 2 | Duplicate capability ID | ERROR |
| 3 | `drop_in` with any required layer ≠ `complete` | ERROR |
| 4 | `drop_in` with evidence weaker than integration (no `differential_exception`) | ERROR |
| 5 | `compatible_with_warning` without diagnostic or migration note | WARNING |
| 6 | `intentional_non_parity` without rationale | ERROR |
| 7 | `unsupported` with `runtime = "complete"` or contradictory layers | ERROR |
| 8 | `drop_in` while `runtime = "refused"` | ERROR |
| 9 | Protocol-crate-only feature marked `drop_in` before config/compiler/runtime | ERROR |
| 10 | CLI capability with no stdout/stderr/exit-code expectation | WARNING |
| 11 | Python capability marked `drop_in` with no test evidence | ERROR |
| 12 | Stale "not recognized"/"unknown-flag" wording in `notes` (Phase 42) | WARNING |
| 13 | `config = "not_applicable"` with parser + translator `complete` without justification (Phase 42) | WARNING |
| 14 | Unknown `caveat_class` value; `refused` layers without classification or rationale; `protocol_crate_only` without crate/refused mention; `deferred_by_adr` without ADR reference | WARNING |

## Usage

```bash
# Validate the manifest
python3 scripts/validate_pproxy_parity_manifest.py docs/parity/pproxy_capability_manifest.toml

# Strict mode (warnings become errors)
python3 scripts/validate_pproxy_parity_manifest.py --strict docs/parity/pproxy_capability_manifest.toml

# Schema-only validation
python3 scripts/validate_pproxy_parity_manifest.py --validate-only docs/parity/pproxy_capability_manifest.toml

# Regenerate the parity report from the manifest (Phase 42)
python3 scripts/validate_pproxy_parity_manifest.py --write-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml

# Verify the parity report is consistent with the manifest (Phase 42; CI runs this)
python3 scripts/validate_pproxy_parity_manifest.py --check-report docs/parity/PPROXY_PARITY_REPORT.md docs/parity/pproxy_capability_manifest.toml
```

## Design Principles

1. **Underclaim**: It is better to mark a feature `native_equivalent` or
   `compatible_with_warning` than to overclaim `drop_in`. Earn `drop_in`
   with evidence.

2. **Granular IDs**: One entry per capability, not per feature family.
   `protocol.socks5.connect.server` not `socks5`.

3. **Layer honesty**: A feature is only `drop_in` if the *entire stack*
   is complete — parser, translator, config, runtime, CLI, and docs.

4. **Protocol-crate-only**: Features implemented only in protocol crates
   (H2, WebSocket, raw) and explicitly refused by the runtime/config
   compiler cannot be `drop_in`.

5. **No aspirational entries**: If it is not implemented, mark it
   `unsupported` with `evidence = "none"`.

## Composition Matrix (Phase A2)

The composition matrix (`composition_matrix.toml`) extends the flat capability
manifest with an explicit **protocol×role×traffic_kind** graph. It prevents
false parity claims by requiring every supported combination to be declared.

### Structure

```toml
[matrix]
schema_version = "1"
manifest_ref = "docs/parity/pproxy_capability_manifest.toml"

[[cell]]
protocol = "socks5"
role = "upstream"
traffic_kind = "tcp"
tier = "drop_in"
evidence = "integration"
capability_ids = ["protocol.socks5.connect_ipv4", ...]

[[chain]]
from_protocol = "socks5"
to_protocol = "http"
traffic_kind = "tcp"
tier = "drop_in"

[[constraint]]
type = "no_udp"
applies_to = ["http", "socks4", "socks4a", "trojan"]
```

### Dimensions

| Dimension | Values |
|-----------|--------|
| **protocol** | direct, http, https, socks4, socks4a, socks5, shadowsocks, trojan, ssh, ws, wss, raw, tunnel, h2, quic, h3, unix, redir |
| **role** | listener, upstream, chain_hop, terminal, reverse_server, reverse_client |
| **traffic_kind** | tcp, udp |
| **tier** | drop_in, compatible_with_warning, native_equivalent, intentional_non_parity, unsupported |

### Constraint Types

| Type | Meaning |
|------|---------|
| `chain_max_hops` | Maximum chain length (currently 10) |
| `no_udp` | Protocols that do not support UDP relay |
| `no_chain` | Protocols that cannot participate in chains |
| `requires_tls` | Protocols requiring TLS transport wrapper |
| `protocol_crate_only` | Implemented in protocol crates but refused by config/runtime |

### Validation

```bash
# Rust validator (21+ tests)
cargo test -p eggress-testkit composition

# Python validator
python3 scripts/validate_pproxy_parity_manifest.py \
    --check-matrix docs/parity/composition_matrix.toml \
    docs/parity/pproxy_capability_manifest.toml

# Config compiler integration (produces warnings for unsupported compositions)
# Integrated into validate_config_composition() in eggress-config/src/validate.rs
```

### Design Principles

1. **Explicit composition**: Every supported protocol×role×traffic_kind must
   have a cell in the matrix. Implicit support is not allowed.

2. **Capability cross-reference**: Drop_in cells must reference manifest
   capability IDs via `capability_ids`.

3. **Protocol-crate-only honesty**: Protocols implemented only in protocol
   crates have `intentional_non_parity` tier with `caveat_class = "protocol_crate_only"`.

4. **Separate from manifest**: The composition matrix is a separate TOML file,
   not an extension of the capability manifest. This keeps concerns separated.
