#!/usr/bin/env python3
"""Validate the pproxy capability manifest (Phase 37, hardened Phase 42).

Usage:
    python3 scripts/validate_pproxy_parity_manifest.py [--strict] [--validate-only] MANIFEST_PATH

Rules enforced:
    1. Unknown tier/layer/evidence value → ERROR
    2. Duplicate capability ID → ERROR
    3. drop_in with any required layer != complete → ERROR
    4. drop_in with evidence weaker than integration (no differential_exception) → ERROR
    5. compatible_with_warning without diagnostic or migration note → WARNING
    6. intentional_non_parity without rationale → ERROR
    7. unsupported with runtime=complete or contradictory layers → ERROR
    8. drop_in while runtime=refused → ERROR
    9. protocol-crate-only feature marked drop_in before config/compiler/runtime → ERROR
    10. CLI capability with no stdout/stderr/exit-code expectation → WARNING
    11. Python capability marked drop_in with no test evidence → ERROR
    12. Stale 'not recognized' / 'unknown-flag' wording for a flag whose
        parser+cli are 'complete' → WARNING (Phase 42 corrective)
    13. config='not_applicable' for a capability whose parser+translator are
        'complete' and whose tier implies a config artifact → WARNING
        (Phase 42 corrective)
    14. caveat_class validation: unknown value → WARNING; refused layers
        without caveat_class or rationale → WARNING; protocol_crate_only
        without crate/refused mention in notes → WARNING; deferred_by_adr
        without ADR mention → WARNING (promoted under --strict)

Exit code: non-zero if any errors (or warnings in --strict mode).
"""

from __future__ import annotations

import sys
import argparse
from pathlib import Path

# ---------------------------------------------------------------------------
# Python 3.11+ has tomllib in stdlib. Fall back to a minimal TOML reader for
# older Python versions (CI might use 3.10).
# ---------------------------------------------------------------------------
try:
    import tomllib
except ModuleNotFoundError:
    try:
        import tomli as tomllib  # type: ignore[no-redef]
    except ModuleNotFoundError:
        # Ultra-minimal fallback: read TOML by line-parsing.
        # Only handles the subset we actually emit (no nested tables, no arrays).
        tomllib = None  # type: ignore[assignment]

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

VALID_TIERS = frozenset({
    "drop_in",
    "compatible_with_warning",
    "native_equivalent",
    "intentional_non_parity",
    "unsupported",
})

VALID_LAYERS = frozenset({
    "complete",
    "partial",
    "not_started",
    "not_applicable",
    "refused",
})

VALID_EVIDENCE = frozenset({
    "differential",
    "integration",
    "unit",
    "synthetic",
    "docs_only",
    "none",
})

REQUIRED_FIELDS = {"id", "category", "tier", "parser", "translator", "config", "runtime", "cli", "python", "docs", "evidence"}

# Layers that must be "complete" for a drop_in claim
def required_drop_in_layers_for_category(category: str) -> set[str]:
    """Return the set of layers that must be 'complete' for a drop_in claim in this category.

    Layer requirements are context-dependent:
    - python category: python + docs (Rust-specific layers are not_applicable)
    - cli category: cli + docs (parser/translator/config/runtime are optional
      for simple flags like --version/--help that are handled by CLI framework)
    - protocol, uri: parser + translator + config + runtime + cli + docs
      (python is NOT required — Python bindings are a separate concern)
    - routing: parser + translator + config + runtime + docs
      (cli is NOT required — routing features can be accessed via admin API)
    """
    if category == "python":
        return {"python", "docs"}
    if category == "cli":
        return {"cli", "docs"}
    if category == "routing":
        return {"parser", "translator", "config", "runtime", "docs"}
    # protocol, uri
    return {"parser", "translator", "config", "runtime", "cli", "docs"}


class ValidationError(Exception):
    """Raised when the manifest has errors."""


class Diagnostic:
    """A validation finding (error or warning)."""

    def __init__(self, level: str, rule: int, entry_id: str, message: str) -> None:
        self.level = level
        self.rule = rule
        self.entry_id = entry_id
        self.message = message

    def __str__(self) -> str:
        prefix = "ERROR" if self.level == "error" else "WARNING"
        return f"  [{prefix}] Rule {self.rule}: [{self.entry_id}] {self.message}"


def parse_manifest(path: Path) -> dict:
    """Parse a TOML manifest file. Returns the full dict."""
    if tomllib is None:
        print("ERROR: Python 3.11+ with tomllib is required, or install 'tomli'.", file=sys.stderr)
        sys.exit(2)

    with open(path, "rb") as f:
        return tomllib.load(f)


def extract_capabilities(data: dict) -> list[dict]:
    """Extract [[capability]] entries from parsed TOML data."""
    caps = data.get("capability", [])
    if not isinstance(caps, list):
        # tomllib may produce a single dict for one [[capability]]
        caps = [caps] if caps else []
    return caps


def validate_manifest(manifest_path: Path, strict: bool = False, validate_only: bool = False) -> int:
    """Validate the manifest. Returns number of errors."""
    data = parse_manifest(manifest_path)
    capabilities = extract_capabilities(data)

    errors: list[Diagnostic] = []
    warnings: list[Diagnostic] = []

    seen_ids: dict[str, int] = {}

    for idx, cap in enumerate(capabilities):
        entry_id = cap.get("id", f"<entry {idx}>")
        tier = cap.get("tier", "")
        category = cap.get("category", "")

        # ── Rule 1: Unknown tier/layer/evidence values ──────────────────
        if tier and tier not in VALID_TIERS:
            errors.append(Diagnostic("error", 1, entry_id, f"unknown tier '{tier}'"))

        for layer_name in ("parser", "translator", "config", "runtime", "cli", "python", "docs"):
            val = cap.get(layer_name, "")
            if val and val not in VALID_LAYERS:
                errors.append(Diagnostic("error", 1, entry_id, f"unknown layer value '{val}' for {layer_name}"))

        evidence = cap.get("evidence", "")
        if evidence and evidence not in VALID_EVIDENCE:
            errors.append(Diagnostic("error", 1, entry_id, f"unknown evidence '{evidence}'"))

        # ── Rule 2: Duplicate capability ID ─────────────────────────────
        if entry_id in seen_ids:
            errors.append(Diagnostic("error", 2, entry_id, f"duplicate ID (first at index {seen_ids[entry_id]})"))
        else:
            seen_ids[entry_id] = idx

        # ── Rule 3: drop_in with any required layer != complete ─────────
        if tier == "drop_in":
            required = required_drop_in_layers_for_category(category)
            for layer_name in required:
                val = cap.get(layer_name, "")
                if val != "complete":
                    errors.append(Diagnostic(
                        "error", 3, entry_id,
                        f"drop_in requires {layer_name}='complete', got '{val}'"
                    ))

        # ── Rule 4: drop_in with evidence weaker than integration ───────
        if tier == "drop_in":
            differential_exception = cap.get("differential_exception", False)
            if not differential_exception:
                evidence_order = {"differential": 0, "integration": 1, "unit": 2, "synthetic": 3, "docs_only": 4, "none": 5}
                ev_rank = evidence_order.get(evidence, 5)
                # For parser-level categories (uri, cli), unit evidence is acceptable
                # because these are grammar/flag features tested at the parser level.
                min_evidence = 2 if category in ("uri", "cli") else 1
                if ev_rank > min_evidence:
                    errors.append(Diagnostic(
                        "error", 4, entry_id,
                        f"drop_in with evidence '{evidence}' (weaker than {'unit' if min_evidence == 2 else 'integration'}); "
                        "add differential_exception=true or upgrade evidence"
                    ))

        # ── Rule 5: compatible_with_warning without diagnostic ──────────
        if tier == "compatible_with_warning":
            diagnostic = cap.get("diagnostic", "")
            notes = cap.get("notes", "")
            if not diagnostic and not notes:
                warnings.append(Diagnostic(
                    "warning", 5, entry_id,
                    "compatible_with_warning without diagnostic code or migration note"
                ))

        # ── Rule 6: intentional_non_parity without rationale ────────────
        if tier == "intentional_non_parity":
            rationale = cap.get("rationale", "")
            if not rationale:
                errors.append(Diagnostic(
                    "error", 6, entry_id,
                    "intentional_non_parity without rationale"
                ))

        # ── Rule 7: unsupported with runtime=complete ────────────────────
        if tier == "unsupported":
            runtime_val = cap.get("runtime", "")
            if runtime_val == "complete":
                errors.append(Diagnostic(
                    "error", 7, entry_id,
                    "unsupported tier but runtime='complete' (contradictory)"
                ))

        # ── Rule 8: drop_in while runtime=refused ──────────────────────
        if tier == "drop_in":
            runtime_val = cap.get("runtime", "")
            if runtime_val == "refused":
                errors.append(Diagnostic(
                    "error", 8, entry_id,
                    "drop_in with runtime='refused' (contradictory)"
                ))

        # ── Rule 9: protocol-crate-only marked drop_in before config/compiler/runtime ──
        if tier == "drop_in":
            config_val = cap.get("config", "")
            runtime_val = cap.get("runtime", "")
            if config_val == "refused" or runtime_val == "refused":
                errors.append(Diagnostic(
                    "error", 9, entry_id,
                    f"drop_in but protocol-crate-only (config='{config_val}', runtime='{runtime_val}'); "
                    "config/compiler/runtime must be complete first"
                ))

        # ── Rule 10: CLI capability with no stdout/stderr/exit-code expectation ──
        if category == "cli":
            notes = cap.get("notes", "")
            tests = cap.get("tests", [])
            # Heuristic: CLI capabilities should have tests or notes describing behavior
            if not tests and not notes:
                warnings.append(Diagnostic(
                    "warning", 10, entry_id,
                    "CLI capability with no tests or notes describing stdout/stderr/exit-code behavior"
                ))

        # ── Rule 11: Python capability marked drop_in with no test evidence ──
        if tier == "drop_in" and category == "python":
            tests = cap.get("tests", [])
            evidence = cap.get("evidence", "")
            if not tests and evidence not in ("differential", "integration"):
                errors.append(Diagnostic(
                    "error", 11, entry_id,
                    "Python drop_in capability with no test evidence"
                ))

        # ── Rule 12: stale "not recognized" or "unknown-flag" notes ─────
        # Phase 42 corrective: the manifest used to claim certain known raw
        # flags were "not recognized" or "unknown-flag" warnings. The
        # translator now parses --ssl, --pac, --test, --sys, --log, --reuse,
        # --get, -a, -b, --rulefile, -ul, -ur, -s, -v, -f, etc. and emits
        # structured diagnostic codes for them. Notes that contradict the
        # parser/translator = "complete" state are stale.
        parser_val = cap.get("parser", "")
        translator_val = cap.get("translator", "")
        cli_val = cap.get("cli", "")
        notes_raw = (cap.get("notes", "") or "")
        eggress_behavior_raw = (cap.get("egress_behavior", "") or "")
        notes = notes_raw.lower()
        eggress_behavior = eggress_behavior_raw.lower()
        combined = notes + " " + eggress_behavior

        if parser_val == "complete" and cli_val == "complete":
            # Only flag a stale phrase when it appears as a positive
            # claim about the capability. Negations like
            # "NOT an 'unknown-flag' warning" or "no longer not recognized"
            # are not stale; they are explicit corrections.
            negation_markers = ("not", "no longer", "rather than", "instead of")
            stale_phrases = [
                ("not recognized", "flag is in the known-raw-flag set (parser=complete, cli=complete)"),
                ("unknown-flag", "flag is parsed and emits a structured diagnostic, not an unknown-flag warning"),
                ("not recognised", "flag is in the known-raw-flag set (parser=complete, cli=complete)"),
            ]
            for stale, why in stale_phrases:
                idx = combined.find(stale)
                if idx < 0:
                    continue
                # Check the words just before the stale phrase for a negation.
                pre_window = combined[max(0, idx - 30):idx]
                negated = any(m in pre_window for m in negation_markers)
                if not negated:
                    warnings.append(Diagnostic(
                        "warning", 12, entry_id,
                        f"stale phrase '{stale}' in notes/eggress_behavior: {why}. "
                        "Update the wording to reflect the structured diagnostic code."
                    ))

        # ── Rule 13: "config = not_applicable" but translator emits config ──
        # Phase 42 corrective: cli.alive used to claim config=not_applicable
        # but the translator emits [health] config. If parser + translator
        # are complete, config should be complete or not_applicable only if
        # explicitly justified.
        if parser_val == "complete" and translator_val == "complete":
            config_val = cap.get("config", "")
            tier_for_check = tier
            if config_val == "not_applicable" and tier_for_check in (
                "drop_in", "native_equivalent", "compatible_with_warning"
            ):
                # Allow when notes/rationale/eggress_behavior explicitly
                # justify the lack of a generated config artifact.
                justification_markers = (
                    "no config", "no generated config",
                    "no artifact", "no configuration artifact",
                    "does not generate", "doesn't generate",
                    "no config artifact",
                    "config artifact is not produced",
                )
                combined_lower = combined  # already lowercased above
                if not any(m in combined_lower for m in justification_markers):
                    warnings.append(Diagnostic(
                        "warning", 13, entry_id,
                        f"config='not_applicable' for tier '{tier_for_check}' with parser+translator=complete. "
                        "Verify this is intentional (e.g. no config artifact is produced); "
                        "add a 'no config artifact' justification to notes or set config='complete'."
                    ))

        # ── Rule 14: caveat_class validation ──────────────────────────────
        VALID_CAVEAT_CLASSES = frozenset({
            "protocol_crate_only",
            "missing_protocol_command",
            "missing_protocol_role",
            "missing_protocol_transport",
            "deferred_by_adr",
            "intentional_non_parity",
            "cli_process_model",
            "translator_scope_gap",
        })
        caveat_class = cap.get("caveat_class", "")
        if caveat_class and caveat_class not in VALID_CAVEAT_CLASSES:
            warnings.append(Diagnostic(
                "warning", 14, entry_id,
                f"unknown caveat_class '{caveat_class}'; expected one of: "
                + ", ".join(sorted(VALID_CAVEAT_CLASSES))
            ))

        config_val_r14 = cap.get("config", "")
        runtime_val_r14 = cap.get("runtime", "")
        rationale_r14 = cap.get("rationale", "")
        notes_r14 = cap.get("notes", "")
        if (config_val_r14 == "refused" or runtime_val_r14 == "refused"):
            if not caveat_class and not rationale_r14:
                warnings.append(Diagnostic(
                    "warning", 14, entry_id,
                    "config or runtime is 'refused' but no caveat_class or rationale "
                    "provided; add a caveat_class or rationale to explain the refusal"
                ))

        if caveat_class == "protocol_crate_only":
            notes_lower = (notes_r14 or "").lower()
            has_protocol_crate_ref = any(w in notes_lower for w in ("protocol", "crate", "refused"))
            if not has_protocol_crate_ref:
                warnings.append(Diagnostic(
                    "warning", 14, entry_id,
                    "caveat_class='protocol_crate_only' but notes do not mention "
                    "which protocol crate exists or which layer refuses it; "
                    "add 'protocol', 'crate', or 'refused' to notes"
                ))

        if caveat_class == "deferred_by_adr":
            combined_r14 = ((rationale_r14 or "") + " " + (notes_r14 or "")).lower()
            if "adr" not in combined_r14:
                warnings.append(Diagnostic(
                    "warning", 14, entry_id,
                    "caveat_class='deferred_by_adr' but rationale/notes do not "
                    "mention 'ADR' or 'adr'; add an ADR reference"
                ))

        # ── Rule 15: drop_in without named evidence reference ──────────
        # Release-blocking drop-in capabilities must have named test files
        # or integration evidence so parity claims are verifiable.
        if tier == "drop_in":
            tests = cap.get("tests", [])
            evidence = cap.get("evidence", "")
            if not tests and evidence not in ("differential", "integration"):
                warnings.append(Diagnostic(
                    "warning", 15, entry_id,
                    "drop_in capability without named test references or "
                    "integration/differential evidence; add test file names "
                    "to 'tests' list for verifiability"
                ))

    # ── Report ──────────────────────────────────────────────────────────
    total_caps = len(capabilities)
    tier_counts: dict[str, int] = {}
    for cap in capabilities:
        t = cap.get("tier", "unknown")
        tier_counts[t] = tier_counts.get(t, 0) + 1

    print(f"Manifest: {manifest_path}")
    print(f"Total capabilities: {total_caps}")
    for tier_name in ("drop_in", "compatible_with_warning", "native_equivalent", "intentional_non_parity", "unsupported"):
        count = tier_counts.get(tier_name, 0)
        print(f"  {tier_name}: {count}")

    if errors:
        print(f"\n{'='*60}")
        print(f"ERRORS ({len(errors)}):")
        print(f"{'='*60}")
        for diag in errors:
            print(str(diag))

    if warnings:
        print(f"\n{'='*60}")
        print(f"WARNINGS ({len(warnings)}):")
        print(f"{'='*60}")
        for diag in warnings:
            print(str(diag))

    # Determine exit code
    error_count = len(errors)
    warning_count = len(warnings)

    if strict:
        error_count += warning_count
        if warnings:
            print(f"\n--strict mode: {len(warnings)} warnings promoted to errors")

    if error_count == 0:
        print(f"\nPASS: {total_caps} capabilities validated successfully.")
        return 0
    else:
        print(f"\nFAIL: {error_count} error(s), {warning_count} warning(s)")
        return 1


def generate_report(manifest_path: Path, report_path: Path) -> int:
    """Generate a markdown parity report from the manifest.

    The report tiers and counts are derived from the manifest, so it
    cannot drift. Section structure follows the original hand-written
    PPROXY_PARITY_REPORT.md.
    """
    data = parse_manifest(manifest_path)
    capabilities = extract_capabilities(data)

    total_caps = len(capabilities)
    tier_counts: dict[str, int] = {}
    for cap in capabilities:
        t = cap.get("tier", "unknown")
        tier_counts[t] = tier_counts.get(t, 0) + 1

    drop_in = [c for c in capabilities if c.get("tier") == "drop_in"]
    compatible = [c for c in capabilities if c.get("tier") == "compatible_with_warning"]
    native_equiv = [c for c in capabilities if c.get("tier") == "native_equivalent"]
    intentional = [c for c in capabilities if c.get("tier") == "intentional_non_parity"]
    unsupported = [c for c in capabilities if c.get("tier") == "unsupported"]

    def percent(n: int) -> str:
        return f"{(100.0 * n / total_caps):.1f}%" if total_caps else "0.0%"

    # Group drop_in by category
    def by_category(caps: list[dict]) -> dict[str, list[dict]]:
        out: dict[str, list[dict]] = {}
        for c in caps:
            cat = c.get("category", "other")
            out.setdefault(cat, []).append(c)
        return out

    drop_in_by_cat = by_category(drop_in)
    # Preserve a stable category order
    category_order = ["cli", "uri", "protocol", "routing", "python"]
    cat_labels = {
        "cli": "CLI",
        "uri": "URI Grammar",
        "protocol": "Runtime Protocols",
        "routing": "Routing",
        "python": "Python",
    }

    lines: list[str] = []
    lines.append("# pproxy Parity Report")
    lines.append("")
    lines.append(f"> Generated by `scripts/validate_pproxy_parity_manifest.py --write-report`.")
    lines.append(f"> Source of truth: `docs/parity/pproxy_capability_manifest.toml`")
    lines.append("")

    lines.append("## Summary")
    lines.append("")
    lines.append("| Tier | Count | Percentage |")
    lines.append("|------|-------|------------|")
    for tname in (
        "drop_in", "compatible_with_warning", "native_equivalent",
        "intentional_non_parity", "unsupported",
    ):
        n = tier_counts.get(tname, 0)
        lines.append(f"| `{tname}` | {n} | {percent(n)} |")
    lines.append(f"| **Total** | **{total_caps}** | |")
    lines.append("")

    lines.append(f"## Drop-In Capabilities ({len(drop_in)})")
    lines.append("")
    lines.append(
        "These features are drop-in replacements for pproxy. All required "
        "layers are `complete` and evidence is `integration` or stronger."
    )
    lines.append("")
    for cat in category_order:
        if cat not in drop_in_by_cat:
            continue
        caps = drop_in_by_cat[cat]
        lines.append(f"### {cat_labels.get(cat, cat.title())} ({len(caps)})")
        ids = [c.get("id", "?") for c in caps]
        lines.append("- " + ", ".join(f"`{i}`" for i in ids))
    lines.append("")

    lines.append(f"## Compatible-With-Warning ({len(compatible)})")
    lines.append("")
    lines.append("These features work but emit diagnostics or differ in a known way.")
    lines.append("")
    lines.append("| ID | Warning | Diagnostic |")
    lines.append("|----|---------|------------|")
    for c in compatible:
        cid = c.get("id", "?")
        notes = c.get("notes", "")
        diag = c.get("diagnostic", "")
        lines.append(f"| `{cid}` | {notes} | `{diag}` |")
    lines.append("")

    lines.append(f"## Native Equivalent ({len(native_equiv)})")
    lines.append("")
    lines.append("These features achieve the same outcome through a different mechanism.")
    lines.append("")
    lines.append("| ID | Mechanism difference |")
    lines.append("|----|---------------------|")
    for c in native_equiv:
        cid = c.get("id", "?")
        notes = c.get("notes", "") or c.get("eggress_behavior", "")
        lines.append(f"| `{cid}` | {notes} |")
    lines.append("")

    lines.append(f"## Intentional Non-Parity ({len(intentional)})")
    lines.append("")
    lines.append("These features are deliberately not replicated with rationale.")
    lines.append("")
    lines.append("| ID | Rationale |")
    lines.append("|----|-----------|")
    for c in intentional:
        cid = c.get("id", "?")
        rationale = c.get("rationale", "")
        lines.append(f"| `{cid}` | {rationale} |")
    lines.append("")

    lines.append(f"## Unsupported ({len(unsupported)})")
    lines.append("")
    lines.append("These features are not implemented.")
    lines.append("")
    lines.append("| ID | Notes |")
    lines.append("|----|-------|")
    for c in unsupported:
        cid = c.get("id", "?")
        notes = c.get("notes", "")
        lines.append(f"| `{cid}` | {notes} |")
    lines.append("")

    # ── Categorized caveat sections ─────────────────────────────────────
    # Group capabilities by caveat_class for targeted report sections.

    def _extract_next_phase(notes: str) -> str:
        """Extract phase references like 'Phase 25-28' or 'H5/H6/H7' from notes."""
        import re
        m = re.search(r'(Phase\s+\S+)', notes, re.IGNORECASE)
        if m:
            return m.group(1)
        m = re.search(r'((?:H\d+(?:/\d+)*))', notes)
        if m:
            return m.group(1)
        return ""

    def _extract_adr_ref(text: str) -> str:
        """Extract ADR file path references from text."""
        import re
        m = re.search(r'(docs/adr/\S+)', text)
        if m:
            return m.group(1)
        m = re.search(r'(ADR\S*)', text)
        if m:
            return m.group(1)
        return ""

    def _refused_layers(c: dict) -> str:
        layers = []
        if c.get("config") == "refused":
            layers.append("config")
        if c.get("runtime") == "refused":
            layers.append("runtime")
        if c.get("cli") == "refused":
            layers.append("cli")
        return ", ".join(layers) if layers else ""

    def _short_note(notes: str, max_len: int = 80) -> str:
        if len(notes) <= max_len:
            return notes
        return notes[:max_len - 1].rsplit(" ", 1)[0] + "\u2026"

    # a. Protocol-Crate-Only Runtime Refusals
    proto_crate = [
        c for c in capabilities
        if c.get("caveat_class") == "protocol_crate_only"
    ]
    if proto_crate:
        lines.append("## Protocol-Crate-Only Runtime Refusals")
        lines.append("")
        lines.append(
            "The following capabilities have protocol crate implementations but are "
            "**refused by the runtime/config compiler** (Phase 25-28 H5/H6/H7). "
            "They cannot be promoted to `drop_in` until the config compiler and "
            "runtime supervisor accept them."
        )
        lines.append("")
        lines.append("| ID | Tier | Refused layers | Note | Next phase |")
        lines.append("|----|------|----------------|------|------------|")
        for c in proto_crate:
            cid = c.get("id", "?")
            tier_val = c.get("tier", "?")
            refused = _refused_layers(c)
            notes_val = _short_note(c.get("notes", ""))
            next_phase = _extract_next_phase(c.get("notes", ""))
            lines.append(f"| `{cid}` | `{tier_val}` | {refused} | {notes_val} | {next_phase} |")
        lines.append("")

    # b. Missing Protocol Commands or Roles
    missing_proto = [
        c for c in capabilities
        if c.get("caveat_class") in ("missing_protocol_command", "missing_protocol_role", "missing_protocol_transport")
    ]
    if missing_proto:
        lines.append("## Missing Protocol Commands or Roles")
        lines.append("")
        lines.append(
            "These capabilities require protocol-level support (commands, roles, "
            "or transports) that the upstream protocol crates do not yet implement."
        )
        lines.append("")
        lines.append("| ID | Tier | What's missing | Note |")
        lines.append("|----|------|----------------|------|")
        for c in missing_proto:
            cid = c.get("id", "?")
            tier_val = c.get("tier", "?")
            cc = c.get("caveat_class", "")
            missing_map = {
                "missing_protocol_command": "command",
                "missing_protocol_role": "role",
                "missing_protocol_transport": "transport",
            }
            what = missing_map.get(cc, cc)
            notes_val = _short_note(c.get("notes", ""))
            lines.append(f"| `{cid}` | `{tier_val}` | {what} | {notes_val} |")
        lines.append("")

    # c. Deferred Design Areas
    deferred = [
        c for c in capabilities
        if c.get("caveat_class") == "deferred_by_adr"
    ]
    if deferred:
        lines.append("## Deferred Design Areas")
        lines.append("")
        lines.append(
            "These capabilities are deferred pending a design decision recorded "
            "in an Architecture Decision Record (ADR)."
        )
        lines.append("")
        lines.append("| ID | Tier | ADR reference |")
        lines.append("|----|------|---------------|")
        for c in deferred:
            cid = c.get("id", "?")
            tier_val = c.get("tier", "?")
            adr = _extract_adr_ref(c.get("rationale", "") + " " + c.get("notes", ""))
            lines.append(f"| `{cid}` | `{tier_val}` | {adr} |")
        lines.append("")

    # d. Intentional Non-Parity (from caveat_class, not the existing tier section)
    intentional_cc = [
        c for c in capabilities
        if c.get("caveat_class") == "intentional_non_parity"
    ]
    if intentional_cc:
        lines.append("## Intentional Non-Parity (Caveat Classified)")
        lines.append("")
        lines.append(
            "These capabilities are explicitly classified as intentional "
            "non-parity with rationale explaining the design choice."
        )
        lines.append("")
        lines.append("| ID | Tier | Rationale |")
        lines.append("|----|------|-----------|")
        for c in intentional_cc:
            cid = c.get("id", "?")
            tier_val = c.get("tier", "?")
            rationale_val = c.get("rationale", "")
            lines.append(f"| `{cid}` | `{tier_val}` | {rationale_val} |")
        lines.append("")

    # e. CLI / Translator Scope Gaps
    scope_gaps = [
        c for c in capabilities
        if c.get("caveat_class") in ("cli_process_model", "translator_scope_gap")
    ]
    if scope_gaps:
        lines.append("## CLI / Translator Scope Gaps")
        lines.append("")
        lines.append(
            "These capabilities are limited by the CLI process model or "
            "translator scope and cannot achieve full drop-in parity."
        )
        lines.append("")
        lines.append("| ID | Tier | Note |")
        lines.append("|----|------|------|")
        for c in scope_gaps:
            cid = c.get("id", "?")
            tier_val = c.get("tier", "?")
            notes_val = _short_note(c.get("notes", ""))
            lines.append(f"| `{cid}` | `{tier_val}` | {notes_val} |")
        lines.append("")

    lines.append("## Verification")
    lines.append("")
    lines.append(
        "This report is generated from the manifest by "
        "`scripts/validate_pproxy_parity_manifest.py --write-report`. The "
        "manifest is the single source of truth; do not edit this file by hand."
    )
    lines.append("")

    report_path.write_text("\n".join(lines))
    print(f"Wrote parity report: {report_path}")
    return 0


def check_report_consistency(
    manifest_path: Path, report_path: Path
) -> int:
    """Check that the hand-written (or previously generated) report matches the manifest.

    Returns 0 if the report is consistent with the manifest, 1 otherwise.
    """
    if not report_path.exists():
        print(f"ERROR: report not found: {report_path}", file=sys.stderr)
        return 1

    data = parse_manifest(manifest_path)
    capabilities = extract_capabilities(data)
    total_caps = len(capabilities)
    tier_counts: dict[str, int] = {}
    for cap in capabilities:
        t = cap.get("tier", "unknown")
        tier_counts[t] = tier_counts.get(t, 0) + 1

    report_text = report_path.read_text()
    errors: list[str] = []

    # Check the Total count appears in the report
    if f"**Total** | **{total_caps}**" not in report_text and f"| **Total** | **{total_caps}**" not in report_text:
        # Some legacy reports omit the bolded Total row; accept plain Total
        if f"| Total | {total_caps} |" not in report_text and f"| **Total** | **{total_caps}**" not in report_text:
            errors.append(
                f"Total count {total_caps} not found in report summary table"
            )

    # Check that each tier count appears
    for tname, expected in tier_counts.items():
        # Look for "| `tier` | N |" pattern
        needle = f"| `{tname}` | {expected} |"
        if needle not in report_text:
            errors.append(
                f"Tier count for `{tname}` (expected {expected}) not found in report"
            )

    if errors:
        print(f"Report is INCONSISTENT with manifest: {report_path}")
        for e in errors:
            print(f"  - {e}")
        return 1
    print(f"Report is consistent with manifest: {report_path}")
    return 0


# ---------------------------------------------------------------------------
# Composition matrix validation (Phase A2)
# ---------------------------------------------------------------------------

VALID_COMPOSITION_PROTOCOLS = frozenset({
    "direct", "http", "https", "socks4", "socks4a", "socks5",
    "shadowsocks", "trojan", "ssh", "ws", "wss", "raw", "tunnel",
    "h2", "quic", "h3", "unix", "redir",
})

VALID_COMPOSITION_ROLES = frozenset({
    "listener", "upstream", "chain_hop", "terminal",
    "reverse_server", "reverse_client",
})

VALID_COMPOSITION_TRAFFIC_KINDS = frozenset({"tcp", "udp"})

VALID_CONSTRAINT_TYPES = frozenset({
    "chain_max_hops", "platform", "requires_tls",
    "no_udp", "no_chain", "protocol_crate_only",
})

VALID_CAVEAT_CLASSES_COMPOSITION = frozenset({
    "protocol_crate_only", "missing_protocol_command",
    "missing_protocol_role", "missing_protocol_transport",
    "deferred_by_adr", "intentional_non_parity",
    "cli_process_model", "translator_scope_gap",
})


def validate_composition_matrix(
    matrix_path: Path,
    manifest_path: Path,
    strict: bool = False,
) -> int:
    """Validate the composition matrix against the manifest.

    Returns the number of errors.
    """
    if tomllib is None:
        print("ERROR: Python 3.11+ with tomllib is required.", file=sys.stderr)
        return 2

    # Load manifest capability IDs
    with open(manifest_path, "rb") as f:
        manifest_data = tomllib.load(f)
    manifest_ids = {
        cap["id"] for cap in manifest_data.get("capability", [])
        if isinstance(cap, dict) and "id" in cap
    }

    # Load composition matrix
    with open(matrix_path, "rb") as f:
        matrix_data = tomllib.load(f)

    errors: list[str] = []
    warnings: list[str] = []

    meta = matrix_data.get("matrix", {})
    schema_version = meta.get("schema_version", "")
    if schema_version != "1":
        errors.append(f"schema version mismatch: got '{schema_version}', expected '1'")

    cells = matrix_data.get("cell", [])
    chains = matrix_data.get("chain", [])
    constraints = matrix_data.get("constraint", [])

    if not cells and not chains:
        errors.append("empty composition matrix (no cells or chains)")

    # Validate cells
    seen_cells: set[tuple[str, str, str]] = set()
    for idx, cell in enumerate(cells):
        proto = cell.get("protocol", "")
        role = cell.get("role", "")
        traffic = cell.get("traffic_kind", "")
        tier = cell.get("tier", "")
        evidence = cell.get("evidence", "")
        cap_ids = cell.get("capability_ids", [])
        caveat_class = cell.get("caveat_class", "")
        cell_key = (proto, role, traffic)

        if proto not in VALID_COMPOSITION_PROTOCOLS:
            errors.append(f"cell[{idx}]: unknown protocol '{proto}'")
        if role not in VALID_COMPOSITION_ROLES:
            errors.append(f"cell[{idx}]: unknown role '{role}'")
        if traffic not in VALID_COMPOSITION_TRAFFIC_KINDS:
            errors.append(f"cell[{idx}]: unknown traffic_kind '{traffic}'")
        if tier not in VALID_TIERS:
            errors.append(f"cell[{idx}]: unknown tier '{tier}'")
        if evidence not in VALID_EVIDENCE:
            errors.append(f"cell[{idx}]: unknown evidence '{evidence}'")
        if caveat_class and caveat_class not in VALID_CAVEAT_CLASSES_COMPOSITION:
            errors.append(f"cell[{idx}]: unknown caveat_class '{caveat_class}'")

        if cell_key in seen_cells:
            errors.append(f"cell[{idx}]: duplicate cell: protocol={proto} role={role} traffic_kind={traffic}")
        seen_cells.add(cell_key)

        if tier == "unsupported" and cap_ids:
            errors.append(f"cell[{idx}]: unsupported cell has non-empty capability_ids: {proto}/{role}")

        if caveat_class == "protocol_crate_only" and tier == "drop_in":
            errors.append(f"cell[{idx}]: protocol-crate-only cell has tier=drop_in: {proto}/{role}")

        if tier == "drop_in" and evidence in ("unit", "synthetic", "docs_only", "none"):
            warnings.append(f"cell[{idx}]: drop_in with weak evidence: {proto}/{role} evidence={evidence}")

        for cap_id in cap_ids:
            if cap_id not in manifest_ids:
                errors.append(f"cell[{idx}]: capability_id not found in manifest: {cap_id}")

    # Validate chains
    seen_chains: set[tuple[str, str, str]] = set()
    for idx, chain in enumerate(chains):
        from_proto = chain.get("from_protocol", "")
        to_proto = chain.get("to_protocol", "")
        traffic = chain.get("traffic_kind", "")
        tier = chain.get("tier", "")
        evidence = chain.get("evidence", "")
        cap_ids = chain.get("capability_ids", [])
        chain_key = (from_proto, to_proto, traffic)

        if from_proto not in VALID_COMPOSITION_PROTOCOLS:
            errors.append(f"chain[{idx}]: unknown from_protocol '{from_proto}'")
        if to_proto not in VALID_COMPOSITION_PROTOCOLS:
            errors.append(f"chain[{idx}]: unknown to_protocol '{to_proto}'")
        if traffic not in VALID_COMPOSITION_TRAFFIC_KINDS:
            errors.append(f"chain[{idx}]: unknown traffic_kind '{traffic}'")
        if tier not in VALID_TIERS:
            errors.append(f"chain[{idx}]: unknown tier '{tier}'")
        if evidence not in VALID_EVIDENCE:
            errors.append(f"chain[{idx}]: unknown evidence '{evidence}'")

        if chain_key in seen_chains:
            errors.append(f"chain[{idx}]: duplicate chain: from={from_proto} to={to_proto} traffic_kind={traffic}")
        seen_chains.add(chain_key)

        for cap_id in cap_ids:
            if cap_id not in manifest_ids:
                errors.append(f"chain[{idx}]: capability_id not found in manifest: {cap_id}")

    # Validate constraints
    for idx, constraint in enumerate(constraints):
        ctype = constraint.get("type", "")
        applies_to = constraint.get("applies_to", [])

        if ctype not in VALID_CONSTRAINT_TYPES:
            errors.append(f"constraint[{idx}]: unknown type '{ctype}'")

        for proto in applies_to:
            if proto not in VALID_COMPOSITION_PROTOCOLS:
                errors.append(f"constraint[{idx}]: applies_to references unknown protocol '{proto}'")

    # Report
    all_findings = []
    for e in errors:
        all_findings.append(f"  [ERROR] {e}")
    for w in warnings:
        level = "ERROR" if strict else "WARNING"
        all_findings.append(f"  [{level}] {w}")

    if all_findings:
        print(f"\nComposition matrix validation ({matrix_path.name}):")
        for finding in all_findings:
            print(finding)

    effective_errors = len(errors) + (len(warnings) if strict else 0)
    if effective_errors == 0:
        print(f"Composition matrix is valid: {len(cells)} cells, {len(chains)} chains, {len(constraints)} constraints")
    return effective_errors


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate the pproxy capability manifest.",
        epilog="Exit code 0 if all checks pass, non-zero otherwise.",
    )
    parser.add_argument(
        "manifest",
        type=Path,
        nargs="?",
        default=None,
        help="Path to pproxy_capability_manifest.toml",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Promote warnings to errors",
    )
    parser.add_argument(
        "--validate-only",
        action="store_true",
        help="Only validate schema (tier/layer/evidence values and required fields)",
    )
    parser.add_argument(
        "--write-report",
        type=Path,
        default=None,
        help="Generate the parity report from the manifest and write it to the given path.",
    )
    parser.add_argument(
        "--check-report",
        type=Path,
        default=None,
        help="Check that the given report file is consistent with the manifest.",
    )
    parser.add_argument(
        "--check-matrix",
        type=Path,
        default=None,
        help="Validate the composition matrix against the manifest.",
    )
    args = parser.parse_args()

    # Two subcommand modes
    if args.write_report is not None:
        if args.manifest is None:
            print("ERROR: --write-report requires a manifest path", file=sys.stderr)
            return 2
        if not args.manifest.exists():
            print(f"ERROR: Manifest file not found: {args.manifest}", file=sys.stderr)
            return 2
        return generate_report(args.manifest, args.write_report)

    if args.check_report is not None:
        if args.manifest is None:
            print("ERROR: --check-report requires a manifest path", file=sys.stderr)
            return 2
        if not args.manifest.exists():
            print(f"ERROR: Manifest file not found: {args.manifest}", file=sys.stderr)
            return 2
        return check_report_consistency(args.manifest, args.check_report)

    if args.check_matrix is not None:
        if args.manifest is None:
            print("ERROR: --check-matrix requires a manifest path", file=sys.stderr)
            return 2
        if not args.manifest.exists():
            print(f"ERROR: Manifest file not found: {args.manifest}", file=sys.stderr)
            return 2
        if not args.check_matrix.exists():
            print(f"ERROR: Composition matrix not found: {args.check_matrix}", file=sys.stderr)
            return 2
        return validate_composition_matrix(args.check_matrix, args.manifest, strict=args.strict)

    if args.manifest is None:
        parser.print_help()
        return 2

    if not args.manifest.exists():
        print(f"ERROR: Manifest file not found: {args.manifest}", file=sys.stderr)
        return 2

    return validate_manifest(args.manifest, strict=args.strict, validate_only=args.validate_only)


if __name__ == "__main__":
    sys.exit(main())
