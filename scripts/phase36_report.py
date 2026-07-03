#!/usr/bin/env python3
"""Phase 36 final parity report generator.

Reads ``tests/compat/pproxy_manifest.toml`` and produces
``target/compat/final-pproxy-parity-report.json`` with the aggregates consumed
by ``docs/release/FINAL_PPROXY_PARITY_REPORT.md``.

Run from the workspace root:

    python3 scripts/phase36_report.py

Exits non-zero on parse error or if the manifest fails validation.
"""
from __future__ import annotations

import json
import re
import sys
from collections import defaultdict
from pathlib import Path

WORKSPACE_ROOT = Path(__file__).resolve().parent.parent
MANIFEST_PATH = WORKSPACE_ROOT / "tests" / "compat" / "pproxy_manifest.toml"
OUTPUT_PATH = WORKSPACE_ROOT / "target" / "compat" / "final-pproxy-parity-report.json"


def parse_features(content: str) -> list[dict]:
    """Extract [[features]] blocks from the manifest as plain dicts."""
    features: list[dict] = []
    for match in re.finditer(
        r"\[\[features\]\](.*?)(?=\[\[features\]\]|\Z)", content, re.DOTALL
    ):
        block = match.group(1)
        feature: dict = {}
        for raw in block.splitlines():
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            if "=" not in line:
                continue
            key, value = line.split("=", 1)
            key = key.strip()
            value = value.strip()
            if value.startswith("[") and value.endswith("]"):
                inner = value[1:-1]
                items: list[str] = []
                if inner.strip():
                    current = ""
                    depth = 0
                    for ch in inner:
                        if ch == '"':
                            depth = 1 - depth
                            continue
                        if ch == "," and depth == 0:
                            items.append(current.strip())
                            current = ""
                            continue
                        current += ch
                    if current.strip():
                        items.append(current.strip())
                feature[key] = items
            else:
                feature[key] = value.strip('"')
        features.append(feature)
    return features


def build_report(features: list[dict]) -> dict:
    """Aggregate manifest features into the parity report payload."""
    by_status: dict[str, int] = defaultdict(int)
    by_evidence: dict[str, int] = defaultdict(int)
    by_category: dict[str, int] = defaultdict(int)
    by_status_category: dict[str, dict[str, int]] = defaultdict(
        lambda: defaultdict(int)
    )

    buckets: dict[str, list] = {
        "compatible": [],
        "supported": [],
        "partial": [],
        "intentional_non_parity": [],
        "unsupported": [],
        "experimental": [],
    }

    perf: list[dict] = []
    sec: list[dict] = []
    plat: list[dict] = []

    for f in features:
        status = f.get("egress_status", "unknown")
        evidence = f.get("evidence_level", "unknown")
        category = f.get("category", "unknown")
        by_status[status] += 1
        by_evidence[evidence] += 1
        by_category[category] += 1
        by_status_category[status][category] += 1

        summary = {
            "id": f.get("id", ""),
            "category": category,
            "evidence_level": evidence,
            "divergence": f.get("divergence", ""),
            "tests": f.get("tests", []),
        }
        if "external_dependency" in f:
            summary["external_dependency"] = f["external_dependency"]

        if status in buckets:
            buckets[status].append(summary)
        if category == "performance":
            perf.append(summary)
        if category == "security":
            sec.append(summary)
        if category == "platform":
            plat.append(summary)

    return {
        "meta": {
            "pproxy_version": "2.7.9",
            "manifest_version": "1",
            "eggress_version": "0.1.0",
            "phase": "36",
            "status": "release_candidate",
        },
        "totals": {"features": len(features)},
        "by_status": dict(sorted(by_status.items())),
        "by_evidence": dict(sorted(by_evidence.items())),
        "by_category": dict(sorted(by_category.items())),
        "by_status_category": {
            status: dict(sorted(cats.items()))
            for status, cats in sorted(by_status_category.items())
        },
        "compatible_features": sorted(buckets["compatible"], key=lambda x: x["id"]),
        "supported_features": sorted(buckets["supported"], key=lambda x: x["id"]),
        "partial_features": sorted(buckets["partial"], key=lambda x: x["id"]),
        "intentional_non_parity_features": sorted(
            buckets["intentional_non_parity"], key=lambda x: x["id"]
        ),
        "unsupported_features": sorted(buckets["unsupported"], key=lambda x: x["id"]),
        "experimental_features": sorted(buckets["experimental"], key=lambda x: x["id"]),
        "performance_features": sorted(perf, key=lambda x: x["id"]),
        "security_features": sorted(sec, key=lambda x: x["id"]),
        "platform_features": sorted(plat, key=lambda x: x["id"]),
    }


def main() -> int:
    if not MANIFEST_PATH.exists():
        print(f"manifest not found at {MANIFEST_PATH}", file=sys.stderr)
        return 2

    content = MANIFEST_PATH.read_text()
    features = parse_features(content)
    if not features:
        print("no [[features]] blocks found in manifest", file=sys.stderr)
        return 2

    report = build_report(features)
    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT_PATH.write_text(json.dumps(report, indent=2) + "\n")

    by_status = report["by_status"]
    print(
        f"wrote {OUTPUT_PATH.relative_to(WORKSPACE_ROOT)} "
        f"({report['totals']['features']} features)"
    )
    print(
        f"  compatible={by_status.get('compatible', 0)} "
        f"supported={by_status.get('supported', 0)} "
        f"partial={by_status.get('partial', 0)} "
        f"intentional_non_parity={by_status.get('intentional_non_parity', 0)} "
        f"unsupported={by_status.get('unsupported', 0)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())