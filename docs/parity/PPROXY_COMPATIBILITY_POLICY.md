# pproxy Compatibility Policy

This document defines the vocabulary, governance rules, and certification
process for eggress's pproxy compatibility claims. It is the authoritative
reference for interpreting tier classifications, evidence requirements, and
release decisions.

## Compatibility Vocabulary

### Terminal Release States

These states may appear in a final certification. A full certification
cannot contain unresolved implementation-progress states.

| State | Definition |
|-------|-----------|
| `drop_in` | Strict behavioral match: oracle and candidate produce identical structured observations for all registered scenarios covering this capability. All layers complete. Differential evidence required. |
| `known_upstream_defect` | The oracle (pproxy) itself has a reproducible defect. The candidate either matches the defect or handles it correctly with an approved exception. Requires a registry entry in `compat/pproxy-2.7.9/known-defects.toml`. |
| `platform_constraint` | The capability cannot be implemented on the target platform due to OS or kernel limitations (e.g., SO_ORIGINAL_DST on macOS). Requires explicit platform annotation. |
| `not_applicable` | The concept does not apply to this capability (e.g., UDP layers for a TCP-only protocol). |

### Implementation Progress States

These states track work in progress. They may appear in development
manifests but must be resolved before full certification.

| State | Definition |
|-------|-----------|
| `behavioral_match` | Observation-level match on a subset of scenarios; full differential evidence not yet complete. |
| `wire_match` | Protocol bytes are compatible but behavioral observation differences remain unclassified. |
| `source_compatible` | Python source code is importable and callable but behavioral equivalence is unproven. |
| `migration_compatible` | The capability can be achieved through eggress-native configuration with documented migration steps. |
| `native_equivalent` | Achieves the same outcome through a different mechanism (not constrained to pproxy's API surface). |
| `compatible_with_warning` | Works but emits a diagnostic or differs in a known, documented way. |
| `intentional_non_parity` | Deliberately not replicated with explicit rationale and ADR reference. |
| `unsupported` | Not implemented. No code path exists. |

## Governing Rules

### Rule 1: Candidate-only tests do not prove compatibility

A test that runs only against eggress (the candidate) without a paired
oracle run cannot establish compatibility. Candidate-only tests prove
correctness, not compatibility.

### Rule 2: Importability does not prove functional behavior

A successful `import pproxy` or `from pproxy import Connection` in the
eggress environment proves only that the namespace is populated. It does
not prove behavioral equivalence.

### Rule 3: Equivalent Rust-native APIs do not prove Python source compatibility

A Rust-native API that achieves the same outcome (e.g., `eggress.Connection`)
does not prove that pproxy Python source code will work unchanged with
eggress. Source compatibility requires that pproxy's own code runs against
the eggress implementation.

### Rule 4: A skipped external test is not a pass

When an external differential test is skipped (due to missing dependencies,
platform constraints, or gating), the skip must be recorded as incomplete.
Skipped tests cannot contribute to `drop_in` certification.

### Rule 5: Documentation cannot override differential evidence

If documentation claims `drop_in` but differential evidence shows a mismatch,
the evidence prevails. Documentation claims are advisory until confirmed
by paired oracle/candidate testing.

### Rule 6: Known upstream defects require reproduction

A `known_upstream_defect` classification requires:
- A reproducible oracle-only failure (pproxy fails, eggress succeeds or handles correctly)
- An entry in `compat/pproxy-2.7.9/known-defects.toml`
- Explicit approval in the defect registry
- A regression test that verifies the classification

### Rule 7: Security hardening that changes observable behavior must be classified

If eggress applies security hardening that changes observable behavior
(e.g., DNS rebinding protection, reserved IP rejection), the resulting
behavioral difference must be:
- Documented in the strict manifest
- Classified as `platform_constraint` or have an approved exception
- Not hidden behind broad normalization

### Rule 8: The canonical eggress namespace is not constrained

The `eggress` Python package (the Rust-backed wheel) is not constrained
by strict pproxy compatibility. Only the `eggress-pproxy-compat` wheel
(top-level `pproxy` import) must satisfy the strict contract.

## Certification Process

### Full Certification Requirements

A full certification requires all of the following:

1. Every strict manifest entry has a terminal state (`drop_in`, `known_upstream_defect`, `platform_constraint`, or `not_applicable`).
2. No entry has an implementation-progress state.
3. Every `drop_in` entry has differential evidence from paired oracle/candidate testing.
4. Every `known_upstream_defect` has a registry entry and regression test.
5. Generated reports are consistent with the manifest.
6. Release documents accurately reflect the certification scope.

### Certification Scope

The certification covers:
- pproxy 2.7.9 on the specified Python versions (3.9–3.13)
- The `eggress-pproxy-compat` package (top-level `pproxy` import)
- The `pproxy` CLI drop-in binary
- All capabilities in `docs/parity/pproxy_2_7_9_strict_manifest.toml`

### What is NOT Covered

- The `eggress` Python package API (different namespace)
- Features behind environment gates that require root or special permissions
- Platform-specific features on platforms where they are not applicable
- Protocols intentionally excluded by ADR (SSH, QUIC/H3, SSR)

## Relationship to Existing Manifests

This policy governs the **strict manifest**
(`pproxy_2_7_9_strict_manifest.toml`), which is separate from:

- The **canonical capability manifest** (`pproxy_capability_manifest.toml`)
  — broader scope, includes native-equivalent and migration-compatible claims.
- The **composition matrix** (`composition_matrix.toml`) — protocol×role graph.
- The **legacy evidence index** (`tests/compat/pproxy_manifest.toml`) — deprecated.

The strict manifest is the source of truth for release certification.
The canonical capability manifest remains the source of truth for feature
planning and development tracking.

## References

- `compat/pproxy-2.7.9/provenance.toml` — oracle package pinning
- `compat/pproxy-2.7.9/known-defects.toml` — upstream defect registry
- `docs/parity/pproxy_2_7_9_strict_manifest.toml` — strict capability manifest
- `docs/parity/pproxy_capability_manifest.toml` — canonical capability manifest
- `plans/MILESTONE_A_HONEST_CONTRACT.md` — implementation plan
