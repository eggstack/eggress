#!/usr/bin/env python3
"""Release-document consistency checker.

Validates that release-facing docs are internally consistent and that
known stale patterns do not creep back in.  Run from the workspace root:

    python3 scripts/check_release_docs.py

Exits 0 if all checks pass, 1 if any check fails.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

WORKSPACE_ROOT = Path(__file__).resolve().parent.parent

PASS = 0
FAILS: list[str] = []


def check(condition: bool, msg: str) -> None:
    if condition:
        print(f"  ✓ {msg}")
    else:
        FAILS.append(msg)
        print(f"  ✗ {msg}")


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def check_baseline_index() -> None:
    print("\n[R1] Baseline index exists and links resolve")
    base = WORKSPACE_ROOT / "docs" / "performance"
    check((base / "BASELINE.md").exists(), "docs/performance/BASELINE.md exists")
    check(
        (base / "BASELINE_2026_07_03.md").exists(),
        "docs/performance/BASELINE_2026_07_03.md exists",
    )
    content = read(base / "BASELINE.md")
    check(
        "BASELINE_2026_07_03.md" in content,
        "BASELINE.md links to BASELINE_2026_07_03.md",
    )


def check_report_compatible_count() -> None:
    print("\n[R2] Final parity report compatible count matches manifest")
    manifest = WORKSPACE_ROOT / "tests" / "compat" / "pproxy_manifest.toml"
    content = read(manifest)
    compatible_ids = []
    for m in re.finditer(
        r"\[\[features\]\](.*?)(?=\[\[features\]\]|\Z)", content, re.DOTALL
    ):
        block = m.group(1)
        fid = re.search(r'id\s*=\s*"([^"]+)"', block)
        status = re.search(r'egress_status\s*=\s*"([^"]+)"', block)
        if fid and status and status.group(1) == "compatible":
            compatible_ids.append(fid.group(1))
    manifest_count = len(compatible_ids)

    report = WORKSPACE_ROOT / "docs" / "release" / "FINAL_PPROXY_PARITY_REPORT.md"
    report_content = read(report)
    m = re.search(r"Compatible features \((\d+)\)", report_content)
    if m:
        report_count = int(m.group(1))
        check(
            report_count == manifest_count,
            f"Report heading says {report_count} compatible; manifest has {manifest_count}",
        )
    else:
        FAILS.append("Could not find compatible count in FINAL_PPROXY_PARITY_REPORT.md")
        print("  ✗ Could not find compatible count heading")

    # Check that each subsection count adds up
    subsection_counts = re.findall(
        r"### \w[^\(]+\((\d+)\)", report_content
    )
    total_from_subsections = sum(int(c) for c in subsection_counts)
    check(
        total_from_subsections == manifest_count,
        f"Subsection counts sum to {total_from_subsections}; manifest has {manifest_count}",
    )


def check_hosted_ci_caveat() -> None:
    print("\n[R3] Hosted-CI caveat present in release docs")
    docs_to_check = [
        ("docs/release/PARITY_RELEASE_GO_NO_GO.md", ["No hosted CI", "local verification"]),
        ("docs/release/FINAL_PPROXY_PARITY_REPORT.md", ["No hosted CI", "local verification"]),
        ("docs/release/RELEASE_NOTES_PARITY_RC.md", ["Hosted CI", "local verification"]),
        ("docs/CI_STATUS.md", ["non-functional", "local verification"]),
    ]
    for rel_path, required_phrases in docs_to_check:
        path = WORKSPACE_ROOT / rel_path
        if not path.exists():
            FAILS.append(f"{rel_path} does not exist")
            print(f"  ✗ {rel_path} does not exist")
            continue
        # Normalize whitespace for cross-line phrase matching
        content = " ".join(read(path).lower().split())
        for phrase in required_phrases:
            normalized_phrase = " ".join(phrase.lower().split())
            check(
                normalized_phrase in content,
                f"{rel_path} contains '{phrase}'",
            )


def check_json_artifact_policy() -> None:
    print("\n[R4] JSON artifact policy explicit")
    report = WORKSPACE_ROOT / "docs" / "release" / "FINAL_PPROXY_PARITY_REPORT.md"
    content = read(report)
    check(
        "not committed" in content.lower() or "generated with" in content.lower(),
        "FINAL_PPROXY_PARITY_REPORT.md clarifies JSON is generated/not committed",
    )
    gonogo = WORKSPACE_ROOT / "docs" / "release" / "PARITY_RELEASE_GO_NO_GO.md"
    gng_content = read(gonogo)
    check(
        "not committed" in gng_content.lower() or "generated" in gng_content.lower(),
        "PARITY_RELEASE_GO_NO_GO.md clarifies JSON artifact policy",
    )


def check_release_notes_count() -> None:
    print("\n[R4b] Release notes compatible count is 26")
    notes = WORKSPACE_ROOT / "docs" / "release" / "RELEASE_NOTES_PARITY_RC.md"
    content = read(notes)
    m = re.search(r"\*\*(\d+) compatible features\*\*", content)
    if m:
        count = int(m.group(1))
        check(count == 26, f"Release notes highlights say {count} compatible; should be 26")
    else:
        FAILS.append("Could not find compatible count in RELEASE_NOTES_PARITY_RC.md")
        print("  ✗ Could not find compatible count")


def check_milestone_plan_status() -> None:
    """[R5] Milestone A-C plans must not be CLOSED while corrective pass is in progress."""
    print("\n[R5] Milestone plan status consistency")
    corrective_pass = WORKSPACE_ROOT / "plans" / "MILESTONES_A_C_CORRECTIVE_PASS.md"
    cp_content = read(corrective_pass)
    cp_in_progress = "Ready for implementation" in cp_content or "corrective pass in progress" in cp_content.lower()

    plans = [
        "plans/MILESTONE_A_HONEST_CONTRACT.md",
        "plans/MILESTONE_B_PYTHON_SOURCE_COMPATIBILITY.md",
        "plans/MILESTONE_C_FUNCTIONAL_INTERNAL_API.md",
    ]
    for rel_path in plans:
        path = WORKSPACE_ROOT / rel_path
        if not path.exists():
            FAILS.append(f"{rel_path} does not exist")
            print(f"  ✗ {rel_path} does not exist")
            continue
        content = read(path)
        is_closed = "CLOSED" in content.split("##")[0] if "##" in content else False
        if cp_in_progress and is_closed:
            FAILS.append(
                f"{rel_path} says CLOSED but corrective pass is in progress"
            )
            print(f"  ✗ {rel_path} says CLOSED but corrective pass is in progress")
        elif not is_closed:
            print(f"  ✓ {rel_path} status is not CLOSED (correct)")
        else:
            print(f"  ✓ {rel_path} status consistent")


def check_strict_report_consistency() -> None:
    """[R6] Strict report gap count must match manifest gap records."""
    print("\n[R6] Strict report vs manifest gap consistency")
    manifest = WORKSPACE_ROOT / "docs" / "parity" / "pproxy_2_7_9_strict_manifest.toml"
    report = WORKSPACE_ROOT / "docs" / "parity" / "PPROXY_2_7_9_STRICT_REPORT.md"
    if not manifest.exists():
        FAILS.append("Strict manifest not found")
        print("  ✗ Strict manifest not found")
        return
    if not report.exists():
        FAILS.append("Strict report not found")
        print("  ✗ Strict report not found")
        return

    manifest_content = read(manifest)
    report_content = read(report)

    gap_count = manifest_content.count('status = "gap"')
    report_gap_match = re.search(r"\|\s*Gap.*?\|\s*(\d+)", report_content)
    if report_gap_match:
        report_gaps = int(report_gap_match.group(1))
        check(
            gap_count == report_gaps,
            f"Manifest has {gap_count} gap records; report says {report_gaps}",
        )
    else:
        FAILS.append("Could not find gap count in strict report")
        print("  ✗ Could not find gap count in strict report")


def check_readme_no_false_completion_claims() -> None:
    """[R7] README must not claim full A-C completion while corrective pass is active."""
    print("\n[R7] README completion claim consistency")
    readme = WORKSPACE_ROOT / "README.md"
    if not readme.exists():
        FAILS.append("README.md not found")
        print("  ✗ README.md not found")
        return
    content = read(readme)
    false_phrases = [
        "milestone a.*complete",
        "milestone b.*complete",
        "milestone c.*complete",
        "corrective pass complete",
        "corrective pass has completed",
        "full pproxy parity.*achieved",
        "all.*acceptance criteria.*satisfied",
    ]
    # Use line-by-line matching to avoid cross-line false positives
    lines = content.split("\n")
    for phrase in false_phrases:
        found = False
        for line in lines:
            if re.search(phrase, line, re.IGNORECASE):
                found = True
                break
        if found:
            FAILS.append(f"README contains possibly-stale claim matching: {phrase}")
            print(f"  ✗ README contains possibly-stale claim matching: {phrase}")
        else:
            print(f"  ✓ README does not contain stale claim: {phrase}")


def main() -> int:
    print("Release-doc consistency check")
    print("=" * 40)
    check_baseline_index()
    check_report_compatible_count()
    check_hosted_ci_caveat()
    check_json_artifact_policy()
    check_release_notes_count()
    check_milestone_plan_status()
    check_strict_report_consistency()
    check_readme_no_false_completion_claims()

    print("\n" + "=" * 40)
    if FAILS:
        print(f"FAILED: {len(FAILS)} check(s) failed")
        for f in FAILS:
            print(f"  - {f}")
        return 1
    else:
        print("ALL CHECKS PASSED")
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
