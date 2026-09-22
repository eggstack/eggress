"""Unit tests for scripts/validate_release_artifacts.py (no network)."""

import importlib.util
import sys
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parents[2] / "scripts" / "validate_release_artifacts.py"


def load_validator():
    spec = importlib.util.spec_from_file_location("validate_release_artifacts", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    sys.modules["validate_release_artifacts"] = module
    spec.loader.exec_module(module)
    return module


va = load_validator()
VERSION = "1.0.7"

WHEELS = {
    "linux-x86_64-gnu": f"eggress-{VERSION}-cp39-abi3-manylinux_2_17_x86_64.whl",
    "linux-aarch64-gnu": f"eggress-{VERSION}-cp39-abi3-manylinux_2_17_aarch64.whl",
    "linux-armv7l-gnu": f"eggress-{VERSION}-cp39-abi3-manylinux_2_31_armv7l.whl",
    "linux-x86_64-musl": f"eggress-{VERSION}-cp39-abi3-musllinux_1_2_x86_64.whl",
    "linux-aarch64-musl": f"eggress-{VERSION}-cp39-abi3-musllinux_1_2_aarch64.whl",
    "linux-armv7l-musl": f"eggress-{VERSION}-cp39-abi3-musllinux_1_2_armv7l.whl",
    "macos-x86_64": f"eggress-{VERSION}-cp39-abi3-macosx_11_0_x86_64.whl",
    "macos-arm64": f"eggress-{VERSION}-cp39-abi3-macosx_11_0_arm64.whl",
    "windows-x86_64": f"eggress-{VERSION}-cp39-abi3-win_amd64.whl",
    "windows-arm64": f"eggress-{VERSION}-cp39-abi3-win_arm64.whl",
}


def _stage(tmp_path, wheels=None, sdist=True):
    dist = tmp_path / "dist"
    dist.mkdir()
    for filename in (wheels if wheels is not None else list(WHEELS.values())):
        (dist / filename).touch()
    if sdist:
        (dist / f"eggress-{VERSION}.tar.gz").touch()
    return dist


def test_valid_tier_a_set(tmp_path):
    result = va.validate(_stage(tmp_path))
    assert result["version"] == VERSION
    for family, filename in WHEELS.items():
        assert result[family] == filename


def test_wheel_count_is_matrix_driven(tmp_path):
    dist = _stage(tmp_path, wheels=list(WHEELS.values())[:5])
    with pytest.raises(SystemExit, match="expected 10 wheels"):
        va.validate(dist)


def test_missing_family_fails_closed(tmp_path):
    wheels = [f for fam, f in WHEELS.items() if fam != "windows-arm64"]
    # A second x86_64 GNU wheel with a different floor keeps the count at ten
    # but duplicates the family while windows-arm64 is missing.
    wheels.append(f"eggress-{VERSION}-cp39-abi3-manylinux_2_28_x86_64.whl")
    dist = _stage(tmp_path, wheels=wheels)
    with pytest.raises(SystemExit, match="duplicate wheel target"):
        va.validate(dist)


def test_unapproved_platform_rejected(tmp_path):
    wheels = [f for fam, f in WHEELS.items() if fam != "windows-arm64"]
    wheels.append(f"eggress-{VERSION}-cp39-abi3-linux_x86_64.whl")
    dist = _stage(tmp_path, wheels=wheels)
    with pytest.raises(SystemExit, match="unapproved wheel platform"):
        va.validate(dist)


def test_non_abi3_rejected(tmp_path):
    wheels = [
        f.replace("cp39-abi3", "cp312-cp312") if fam == "windows-x86_64" else f
        for fam, f in WHEELS.items()
    ]
    with pytest.raises(SystemExit, match="not cp39-abi3"):
        va.validate(_stage(tmp_path, wheels=wheels))


def test_version_mismatch_rejected(tmp_path):
    wheels = [
        f.replace(VERSION, "9.9.9") if fam == "macos-arm64" else f
        for fam, f in WHEELS.items()
    ]
    with pytest.raises(SystemExit, match="version mismatch"):
        va.validate(_stage(tmp_path, wheels=wheels))


def test_sdist_mismatch_rejected(tmp_path):
    dist = _stage(tmp_path)
    (dist / f"eggress-{VERSION}.tar.gz").unlink()
    (dist / "eggress-9.9.9.tar.gz").touch()
    with pytest.raises(SystemExit, match="sdist version does not match"):
        va.validate(dist)


def test_debug_artifact_rejected(tmp_path):
    debug_wheel = f"eggress-{VERSION}-1debug-cp39-abi3-win_amd64.whl"
    wheels = [
        debug_wheel if fam == "windows-x86_64" else f
        for fam, f in WHEELS.items()
    ]
    with pytest.raises(SystemExit, match="debug artifact"):
        va.validate(_stage(tmp_path, wheels=wheels))


def test_gnu_musl_armv7_families_distinct():
    assert va.family_for_platform_tag("manylinux_2_17_x86_64") == "linux-x86_64-gnu"
    assert va.family_for_platform_tag("manylinux_2_17_aarch64") == "linux-aarch64-gnu"
    assert va.family_for_platform_tag("manylinux_2_31_armv7l") == "linux-armv7l-gnu"
    assert va.family_for_platform_tag("musllinux_1_2_x86_64") == "linux-x86_64-musl"
    assert va.family_for_platform_tag("musllinux_1_2_aarch64") == "linux-aarch64-musl"
    assert va.family_for_platform_tag("musllinux_1_2_armv7l") == "linux-armv7l-musl"
    assert va.family_for_platform_tag("macosx_11_0_arm64") == "macos-arm64"
    assert va.family_for_platform_tag("win_arm64") == "windows-arm64"
    assert va.family_for_platform_tag("linux_x86_64") is None
