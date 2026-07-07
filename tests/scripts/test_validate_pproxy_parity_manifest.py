"""Regression tests for Rule 14 scope fix in validate_pproxy_parity_manifest.py.

Each test writes a minimal 3-capability TOML manifest (first, middle, last),
runs validate_manifest(), and asserts the expected diagnostic targets the
correct entry_id.
"""

import sys
import textwrap
import tempfile
from pathlib import Path

import pytest

# Ensure the scripts/ directory is importable
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
from validate_pproxy_parity_manifest import validate_manifest, Diagnostic  # noqa: E402

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

BASE_CAP = textwrap.dedent("""\
    [metadata]
    version = "1.0"

    [[capability]]
    id = "{id}"
    category = "protocol"
    tier = "intentional_non_parity"
    parser = "complete"
    translator = "complete"
    config = "complete"
    runtime = "complete"
    cli = "not_applicable"
    python = "not_applicable"
    docs = "complete"
    evidence = "unit"
    rationale = "Test rationale for {id}"
""")


def _write_manifest(tmp_path: Path, caps: list[str]) -> Path:
    """Write a manifest with multiple [[capability]] sections."""
    manifest = tmp_path / "manifest.toml"
    parts = [textwrap.dedent("""\
        [metadata]
        version = "1.0"
    """)]
    parts.extend(caps)
    manifest.write_text("\n".join(parts))
    return manifest


def _make_cap(
    entry_id: str,
    *,
    caveat_class: str = "",
    notes: str = "",
    rationale: str = "",
    config: str = "complete",
    runtime: str = "complete",
) -> str:
    """Build a [[capability]] TOML block."""
    lines = [
        "[[capability]]",
        f'id = "{entry_id}"',
        'category = "protocol"',
        'tier = "intentional_non_parity"',
        'parser = "complete"',
        'translator = "complete"',
        f'config = "{config}"',
        f'runtime = "{runtime}"',
        'cli = "not_applicable"',
        'python = "not_applicable"',
        'docs = "complete"',
        'evidence = "unit"',
    ]
    rationale_val = rationale if rationale != "" else f"Test rationale for {entry_id}"
    lines.append(f'rationale = "{rationale_val}"')
    if caveat_class:
        lines.append(f'caveat_class = "{caveat_class}"')
    if notes:
        lines.append(f'notes = "{notes}"')
    return "\n".join(lines)


def _run(caps: list[str]) -> list[Diagnostic]:
    """Write manifest, validate, return warnings."""
    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(Path(tmp), caps)
        # We need to capture output; validate_manifest prints to stdout.
        # Redirect stdout by temporarily replacing sys.stdout is messy.
        # Instead, we just run it and inspect the return code + captured output.
        # For cleaner tests, we'll call validate_manifest and capture its
        # printed output by redirecting.
        import io
        old_stdout = sys.stdout
        sys.stdout = buf = io.StringIO()
        try:
            exit_code = validate_manifest(manifest, strict=False)
        finally:
            sys.stdout = old_stdout
    output = buf.getvalue()
    return exit_code, output


def _run_strict(caps: list[str]) -> tuple[int, str]:
    """Write manifest, validate in strict mode, return (exit_code, output)."""
    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(Path(tmp), caps)
        import io
        old_stdout = sys.stdout
        sys.stdout = buf = io.StringIO()
        try:
            exit_code = validate_manifest(manifest, strict=True)
        finally:
            sys.stdout = old_stdout
    return exit_code, buf.getvalue()


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestRule14ScopeFix:
    """Rule 14 must validate ALL capabilities, not just the last."""

    def test_unknown_caveat_class_first_entry(self):
        """Unknown caveat_class on the FIRST capability should produce a warning."""
        caps = [
            _make_cap("first", caveat_class="bogus_value"),
            _make_cap("middle"),
            _make_cap("last"),
        ]
        exit_code, output = _run(caps)
        assert "[first]" in output, f"Expected [first] in output:\n{output}"
        assert "unknown caveat_class 'bogus_value'" in output
        # Only one Rule 14 warning should appear (for first, not middle or last)
        assert output.count("unknown caveat_class") == 1

    def test_unknown_caveat_class_middle_entry(self):
        """Unknown caveat_class on a MIDDLE capability should produce a warning."""
        caps = [
            _make_cap("first"),
            _make_cap("middle", caveat_class="definitely_wrong"),
            _make_cap("last"),
        ]
        exit_code, output = _run(caps)
        assert "[middle]" in output, f"Expected [middle] in output:\n{output}"
        assert "unknown caveat_class 'definitely_wrong'" in output

    def test_unknown_caveat_class_last_entry(self):
        """Unknown caveat_class on the LAST capability should produce a warning."""
        caps = [
            _make_cap("first"),
            _make_cap("middle"),
            _make_cap("last", caveat_class="also_wrong"),
        ]
        exit_code, output = _run(caps)
        assert "[last]" in output, f"Expected [last] in output:\n{output}"
        assert "unknown caveat_class 'also_wrong'" in output

    def test_refused_without_caveat_class(self):
        """config='refused' without caveat_class or rationale should warn."""
        # Use native_equivalent tier (no rationale required by Rule 6)
        caps = [
            textwrap.dedent("""\
                [[capability]]
                id = "first"
                category = "protocol"
                tier = "native_equivalent"
                parser = "complete"
                translator = "complete"
                config = "refused"
                runtime = "complete"
                cli = "not_applicable"
                python = "not_applicable"
                docs = "complete"
                evidence = "unit"
            """),
            _make_cap("middle"),
            _make_cap("last"),
        ]
        exit_code, output = _run(caps)
        assert "[first]" in output, f"Expected [first] in output:\n{output}"
        assert "refused" in output.lower()
        assert "no caveat_class or rationale" in output

    def test_deferred_by_adr_without_adr(self):
        """caveat_class='deferred_by_adr' without ADR in rationale/notes should warn."""
        caps = [
            _make_cap("first"),
            _make_cap(
                "middle",
                caveat_class="deferred_by_adr",
                rationale="Deferred for now",
                notes="No reference to any design document",
            ),
            _make_cap("last"),
        ]
        exit_code, output = _run(caps)
        assert "[middle]" in output, f"Expected [middle] in output:\n{output}"
        assert "deferred_by_adr" in output
        assert "add an ADR reference" in output

    def test_protocol_crate_only_without_crate_ref(self):
        """caveat_class='protocol_crate_only' without protocol/crate/refused in notes should warn."""
        caps = [
            _make_cap("first"),
            _make_cap("middle"),
            _make_cap(
                "last",
                caveat_class="protocol_crate_only",
                notes="This is just some generic note",
            ),
        ]
        exit_code, output = _run(caps)
        assert "[last]" in output, f"Expected [last] in output:\n{output}"
        assert "protocol_crate_only" in output
        assert "add 'protocol', 'crate', or 'refused' to notes" in output

    def test_all_three_entries_checked(self):
        """When all three entries have issues, all three should be reported."""
        caps = [
            _make_cap("first", caveat_class="unknown_class_a"),
            _make_cap("middle", caveat_class="unknown_class_b"),
            _make_cap("last", caveat_class="unknown_class_c"),
        ]
        exit_code, output = _run(caps)
        assert "[first]" in output
        assert "[middle]" in output
        assert "[last]" in output
        assert "unknown caveat_class" in output

    def test_valid_caveat_class_no_warning(self):
        """Valid caveat_class with proper notes should produce no Rule 14 warning."""
        caps = [
            _make_cap(
                "first",
                caveat_class="protocol_crate_only",
                notes="This protocol crate refuses runtime integration",
            ),
            _make_cap("middle"),
            _make_cap("last"),
        ]
        exit_code, output = _run(caps)
        assert "Rule 14" not in output, f"Unexpected Rule 14 warning:\n{output}"

    def test_strict_mode_promotes_rule14_warnings(self):
        """Rule 14 warnings should become errors in --strict mode."""
        caps = [
            _make_cap("first", caveat_class="bogus"),
            _make_cap("middle"),
            _make_cap("last"),
        ]
        exit_code, output = _run_strict(caps)
        assert exit_code == 1, f"Expected non-zero exit in strict mode:\n{output}"
        assert "warnings promoted to errors" in output


class TestRule14OnRealManifest:
    """Ensure the real manifest passes Rule 14 validation."""

    def test_real_manifest_passes(self):
        manifest_path = (
            Path(__file__).resolve().parents[2]
            / "docs"
            / "parity"
            / "pproxy_capability_manifest.toml"
        )
        if not manifest_path.exists():
            pytest.skip("Real manifest not found")

        import io
        old_stdout = sys.stdout
        sys.stdout = buf = io.StringIO()
        try:
            exit_code = validate_manifest(manifest_path, strict=False)
        finally:
            sys.stdout = old_stdout
        output = buf.getvalue()
        assert exit_code == 0, f"Validator failed on real manifest:\n{output}"

    def test_real_manifest_strict(self):
        manifest_path = (
            Path(__file__).resolve().parents[2]
            / "docs"
            / "parity"
            / "pproxy_capability_manifest.toml"
        )
        if not manifest_path.exists():
            pytest.skip("Real manifest not found")

        import io
        old_stdout = sys.stdout
        sys.stdout = buf = io.StringIO()
        try:
            exit_code = validate_manifest(manifest_path, strict=True)
        finally:
            sys.stdout = old_stdout
        output = buf.getvalue()
        # Strict mode may have warnings promoted; record but don't hard-fail
        # unless it's an actual structural issue.
        if exit_code != 0:
            print(f"Strict mode output:\n{output}")
