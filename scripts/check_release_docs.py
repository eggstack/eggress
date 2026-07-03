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


def main() -> int:
    print("Release-doc consistency check")
    print("=" * 40)
    check_baseline_index()
    check_report_compatible_count()
    check_hosted_ci_caveat()
    check_json_artifact_policy()
    check_release_notes_count()

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
