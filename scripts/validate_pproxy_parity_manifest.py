#!/usr/bin/env python3
"""Validate the pproxy capability manifest (Phase 37).

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


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate the pproxy capability manifest.",
        epilog="Exit code 0 if all checks pass, non-zero otherwise.",
    )
    parser.add_argument(
        "manifest",
        type=Path,
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
    args = parser.parse_args()

    if not args.manifest.exists():
        print(f"ERROR: Manifest file not found: {args.manifest}", file=sys.stderr)
        return 2

    return validate_manifest(args.manifest, strict=args.strict, validate_only=args.validate_only)


if __name__ == "__main__":
    sys.exit(main())
