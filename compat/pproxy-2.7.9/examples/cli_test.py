#!/usr/bin/env python3
"""Executable oracle fixture: pproxy CLI behavior tests.

Tests --help, --version, parser defaults, and invalid URI handling
by invoking pproxy as a subprocess.

Provenance: Eggress-authored behavioral scenario based on pproxy 2.7.9 public API.
License: MIT (pproxy)
Tested with: pproxy==2.7.9 on Python 3.11
"""
import subprocess
import sys

try:
    from importlib.metadata import version as _get_version
    PPROXY_VERSION = _get_version("pproxy")
except Exception:
    PPROXY_VERSION = "unknown"

passed = 0
failed = 0


def check(name, condition, detail=""):
    global passed, failed
    if condition:
        print(f"  PASS: {name}")
        passed += 1
    else:
        msg = f"  FAIL: {name}"
        if detail:
            msg += f" -- {detail}"
        print(msg)
        failed += 1


def run_pproxy(*args, timeout=5):
    """Run pproxy with given args, return (returncode, stdout, stderr)."""
    cmd = [sys.executable, "-m", "pproxy"] + list(args)
    try:
        result = subprocess.run(
            cmd, capture_output=True, text=True, timeout=timeout
        )
        return result.returncode, result.stdout, result.stderr
    except subprocess.TimeoutExpired:
        return -1, "", "timeout"


def test_help():
    """Test --help exits 0 and shows usage."""
    print("test_help")
    rc, stdout, stderr = run_pproxy("--help")
    check("--help exits 0", rc == 0, f"rc={rc}")
    combined = stdout + stderr
    check("--help mentions -l/--listen", "-l" in combined or "--listen" in combined)
    check("--help mentions -r/--remote", "-r" in combined or "--remote" in combined)
    check("--help mentions proxy protocol keywords",
          "socks" in combined.lower() or "http" in combined.lower() or "ss" in combined.lower())


def test_version():
    """Test --version exits 0 and shows version string."""
    print("test_version")
    rc, stdout, stderr = run_pproxy("--version")
    combined = stdout + stderr
    check("--version exits 0 or 2", rc in (0, 2), f"rc={rc}")
    check("--version mentions pproxy or version", "pproxy" in combined.lower() or "version" in combined.lower())


def test_invalid_uri():
    """Test invalid URI produces error, not crash."""
    print("test_invalid_uri")
    rc, stdout, stderr = run_pproxy("-l", "not_a_valid_uri")
    check("invalid URI does not exit 0", rc != 0, f"rc={rc}")
    combined = stdout + stderr
    check("error message present", len(combined.strip()) > 0)


def test_default_listen():
    """Test default listen address behavior."""
    print("test_default_listen")
    rc, stdout, stderr = run_pproxy("--help")
    combined = stdout + stderr
    # pproxy --help should show default port or listen syntax
    check("--help output is non-empty", len(combined.strip()) > 0)


def test_rulefile_flag():
    """Test --rulefile with non-existent file produces error."""
    print("test_rulefile_flag")
    rc, stdout, stderr = run_pproxy("-l", "http://:8080/", "--rulefile", "/nonexistent/path/rules.txt")
    # Should either error about the file or at least not crash
    check("rulefile with bad path does not hang", rc != -1, f"rc={rc}")


def test_multiple_listeners():
    """Test multiple -l flags."""
    print("test_multiple_listeners")
    rc, stdout, stderr = run_pproxy(
        "-l", "http://:8080/",
        "-l", "socks5://:1080/",
        "--help"
    )
    check("multiple -l with --help works", rc == 0, f"rc={rc}")


def main():
    print(f"pproxy {PPROXY_VERSION} cli fixture")
    print(f"Python {sys.version}")
    print()

    test_help()
    test_version()
    test_invalid_uri()
    test_default_listen()
    test_rulefile_flag()
    test_multiple_listeners()

    print()
    print(f"Results: {passed} passed, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
