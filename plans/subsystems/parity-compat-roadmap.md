# Parity Contract and Compatibility Translation Roadmap

Status: closed

Long-term references:

- `plans/000-long-term-specification.md` §4 (invariant 9), §5 (compat contract)
- `plans/001-terminology-and-domain-model.md` §3–§4 (Tier vs Status, manifest authority)
- `plans/002-long-term-roadmap.md` (Phases 7–8, 36–42)

Related ADRs:

- `plans/adrs/ADR-0001-planning-conventions.md` (process only)

## 1. Purpose and ownership boundary

Owns the pproxy compatibility surface: `eggress-pproxy-compat` translation (CLI flags, URIs incl. `__` chains/modifiers/special schemes, rulefiles, `-a`/`--pac`/`--test`/`--sys`/`--log`/`--get` semantics), tier diagnostics (`tier.rs` single owner), the capability manifest + validator + generated report/matrix. The manifest is authoritative; reports follow it. MUST NOT own native runtime behavior or the oracle itself (oracle is external, opt-in).

Architecture: `architecture/pproxy-compat.md`; contract: `docs/parity/pproxy_capability_manifest.toml`, `docs/parity/PPROXY_PRACTICAL_COMPATIBILITY_MATRIX.md`, `docs/parity/README.md`.

## 2. Work classification

### Invariants

- Tier vocabulary (`matched`/`supported_difference`/`platform_limited`/`intentional_non_parity`; diagnostic five-tier in `tier.rs`); claim changes require manifest update + oracle/differential/interop runs.

### Capabilities

- Flag/URI/rulefile translation with structured diagnostics; migration guide; differential harness scenarios.

### Infrastructure

- Manifest validator (`scripts/validate_pproxy_parity_manifest.py`), differential harness (`eggress-testkit::differential`), oracle runner (external, gated).

### Polish

- Matrix/README wording harmonization, report generation.

## 3. Non-goals

- Strict drop-in beyond the practical-compat target; upstream pproxy changes (frozen `2.7.9` pin).

## 4. Current state

Manifest complete with no unresolved `gap` outside tiered boundaries; report generated from manifest with CI consistency check; differential suites (27 + 11 scenarios, Python structural tests) complete under the two-gate strategy. External interop suites remain opt-in.

## 5. Target architecture

Manifest-governed claim set with derived reports and gated evidence — attained.

## 6. Dependency graph

```text
Parity spec + tiers (hard)
    `--> Translator surface (hard)
             `--> Differential/oracle evidence (operational: external runners)
                      `--> Corrective consistency (soft)
```

## 7. Milestones

### Milestone 1 — Contract and translator

Class: capability. Objective: translator + tiers + matrix + validator. Exit: Phases 7–8, 36–39, 42 complete. Status: closed (historical, archive).

### Milestone 2 — Evidence closure and strict-phase completion

Class: capability/polish. Objective: differential/interop evidence, strict phases 0–10, consistency passes. Exit: manifest `gap`-free per §4. Status: closed. Evidence: archive `PPROXY_*`, `MILESTONES_A_C_*`, `API_BOUNDARY_*` records.

## 8. Cross-cutting requirements

Compat: five-tier diagnostics never disagree (single owner). Security: redaction in repr/TOML. CI: `--check-report` consistency; external suites only when the claim changed. Docs: parity README + matrix + skill updates together.

## 9. Verification strategy

Manifest validator (strict + report-check), differential suites, opt-in external interop (`EGRESS_REQUIRE_EXTERNAL_INTEROP=1 … differential_pproxy --ignored`, `EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1 … interoperability_shadowsocks --ignored`), each only when claims changed.

## 10. Risks and decision points

None active. Any new parity claim requires manifest + evidence + matrix regeneration.

## 11. Completion definition

Manifest-authoritative contract with `gap`-free tiered coverage and derived reports — attained.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| 1 | closed (historical) | archive parity records | archive + manifest history | — |
| 2 | closed (historical) | archive strict/differential records | manifest + CI history | — |
