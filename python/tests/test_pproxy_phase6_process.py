"""Strict pproxy executable/process contract checks for Phase 6."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def run_pproxy(*args: str) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["PYTHONPATH"] = str(ROOT / "python")
    return subprocess.run(
        [sys.executable, "-m", "pproxy", *args],
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
        timeout=15,
    )


def test_false_gap_options_fail_before_python_service_start():
    for args, expected in (
        (("--log", "ignored"), "--log"),
        (("--rulefile", "ignored"), "--rulefile"),
        (("--listen", "http://:0"), "--listen"),
        (("-s", "invalid"), "invalid choice"),
    ):
        result = run_pproxy(*args)
        assert result.returncode == 2, result.stderr
        assert expected in result.stderr
        assert "started" not in result.stderr


def test_python_test_mode_uses_native_bridge_without_listener_startup():
    result = run_pproxy(
        "-l",
        "http://:0",
        "-r",
        "socks5://127.0.0.1:1",
        "--test",
        "http://example.com/phase6",
    )
    assert result.returncode == 1, result.stderr
    assert "pproxy-upstream-0" in result.stdout
    assert "started" not in result.stderr
    assert "listen:" not in result.stderr
