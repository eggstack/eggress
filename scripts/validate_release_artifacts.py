#!/usr/bin/env python3
"""Validate the canonical eggress PyPI artifact set (wheels + sdist).

The expected set is the Tier A matrix: ten ordinary-CPython ``cp39-abi3``
wheel families plus one sdist. This table is the canonical target
definition; adding or removing an approved target means changing this set
together with the ``build-wheels`` matrix in
``.github/workflows/publish-python.yml``.

Wheel filenames are parsed with the ``packaging`` library (standards-aware),
not manual ``name.split("-")`` parsing. The validator is fail-closed: any
missing, duplicate, mislabeled, or unexpected family fails the release, as
does any version mismatch or debug artifact.
"""

from __future__ import annotations

import fnmatch
import re
import sys
from pathlib import Path

try:
    from packaging.utils import parse_wheel_filename
except ImportError:
    print(
        "ERROR: the 'packaging' library is required "
        "(python -m pip install 'packaging>=24')",
        file=sys.stderr,
    )
    raise SystemExit(2)

DIST_DIR = Path("dist")

# Canonical Tier A wheel families. Each must appear exactly once; no other
# family may appear in a production publish.
EXPECTED_FAMILIES = frozenset(
    {
        "linux-x86_64-gnu",
        "linux-aarch64-gnu",
        "linux-armv7l-gnu",
        "linux-x86_64-musl",
        "linux-aarch64-musl",
        "linux-armv7l-musl",
        "macos-x86_64",
        "macos-arm64",
        "windows-x86_64",
        "windows-arm64",
    }
)

# (glob over the platform tag, family). Order matters: first match wins.
PLATFORM_FAMILY_MAP = (
    ("manylinux*x86_64", "linux-x86_64-gnu"),
    ("manylinux*aarch64", "linux-aarch64-gnu"),
    ("manylinux*armv7l", "linux-armv7l-gnu"),
    ("musllinux*x86_64", "linux-x86_64-musl"),
    ("musllinux*aarch64", "linux-aarch64-musl"),
    ("musllinux*armv7l", "linux-armv7l-musl"),
    ("macosx*x86_64", "macos-x86_64"),
    ("macosx*arm64", "macos-arm64"),
    ("win_amd64", "windows-x86_64"),
    ("win_arm64", "windows-arm64"),
)

EXPECTED_DIST_NAME = "eggress"
EXPECTED_PYTHON_TAG = "cp39"
EXPECTED_ABI_TAG = "abi3"


def family_for_platform_tag(platform_tag: str) -> str | None:
    for pattern, family in PLATFORM_FAMILY_MAP:
        if fnmatch.fnmatchcase(platform_tag, pattern):
            return family
    return None


def validate(dist_dir: Path = DIST_DIR) -> dict[str, str]:
    wheels = sorted(dist_dir.glob("*.whl"))
    sdists = sorted(dist_dir.glob("*.tar.gz"))
    if len(wheels) != len(EXPECTED_FAMILIES):
        raise SystemExit(
            f"::error::expected {len(EXPECTED_FAMILIES)} wheels "
            f"(Tier A matrix), found {len(wheels)}"
        )
    if len(sdists) != 1:
        raise SystemExit(f"::error::expected exactly one sdist, found {len(sdists)}")

    version: str | None = None
    targets: dict[str, str] = {}
    for wheel in wheels:
        try:
            name, ver, _build, tags = parse_wheel_filename(wheel.name)
        except ValueError:
            raise SystemExit(f"::error::unexpected wheel filename: {wheel.name}")
        if name != EXPECTED_DIST_NAME:
            raise SystemExit(f"::error::unexpected distribution name: {wheel.name}")
        if version is None:
            version = str(ver)
        elif str(ver) != version:
            raise SystemExit(f"::error::wheel version mismatch: {wheel.name}")
        if len(tags) != 1:
            raise SystemExit(
                f"::error::abi3 wheel must carry exactly one tag: {wheel.name}"
            )
        tag = next(iter(tags))
        if tag.interpreter != EXPECTED_PYTHON_TAG or tag.abi != EXPECTED_ABI_TAG:
            raise SystemExit(f"::error::wheel is not cp39-abi3: {wheel.name}")
        family = family_for_platform_tag(tag.platform)
        if family is None:
            raise SystemExit(f"::error::unapproved wheel platform: {wheel.name}")
        if family in targets:
            raise SystemExit(f"::error::duplicate wheel target {family}: {wheel.name}")
        targets[family] = wheel.name

    if set(targets) != set(EXPECTED_FAMILIES):
        missing = set(EXPECTED_FAMILIES) - set(targets)
        extra = set(targets) - set(EXPECTED_FAMILIES)
        raise SystemExit(
            f"::error::wheel target set mismatch: missing={sorted(missing)} "
            f"extra={sorted(extra)}"
        )
    if version is None or sdists[0].name != f"eggress-{version}.tar.gz":
        raise SystemExit(
            f"::error::sdist version does not match wheels: {sdists[0].name}"
        )
    if not re.fullmatch(r"eggress-[0-9]+\.[0-9]+\.[0-9]+\.tar\.gz", sdists[0].name):
        raise SystemExit(f"::error::unexpected sdist filename: {sdists[0].name}")
    if any("debug" in path.name.lower() for path in [*wheels, *sdists]):
        raise SystemExit("::error::debug artifact found")

    return {"version": version, **targets, "sdist": sdists[0].name}


def main() -> int:
    result = validate()
    print("approved artifact set:")
    for family in sorted(EXPECTED_FAMILIES):
        print(f"  {family}: {result[family]}")
    print(f"  sdist: {result['sdist']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
