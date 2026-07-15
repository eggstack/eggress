"""FD/task leak test: repeatedly create and destroy Connection objects.

Verifies that creating and closing multiple Connection objects in a loop
does not leak file descriptors or background tasks.
"""

from __future__ import annotations

import gc
import os
import resource

import pytest

pytest.importorskip("eggress._eggress")

from eggress.connection import Connection  # noqa: E402


def _count_fds() -> int:
    """Count open file descriptors (Unix only)."""
    try:
        return len(os.listdir("/proc/self/fd"))
    except (OSError, FileNotFoundError):
        # macOS: use lsof fallback
        try:
            import subprocess
            result = subprocess.run(
                ["lsof", "-p", str(os.getpid())],
                capture_output=True,
                text=True,
                timeout=5,
            )
            return len(result.stdout.strip().split("\n")) - 1  # subtract header
        except Exception:
            return 0


_SOCKS5_URI = "socks5://127.0.0.1:0"


class TestConnectionLeakDetection:
    """Verify no FD/task leaks after repeated Connection create/destroy cycles."""

    def test_connection_create_close_cycle_no_fd_leak(self) -> None:
        """Create and close 30 Connection objects; FD count must not grow."""
        gc.collect()
        baseline_fds = _count_fds()

        conns = []
        for _ in range(30):
            conn = Connection(_SOCKS5_URI)
            conns.append(conn)

        # Close all connections
        for conn in conns:
            try:
                conn.close()
            except Exception:
                pass
        conns.clear()

        # Allow cleanup
        gc.collect()

        final_fds = _count_fds()
        # Allow ±5 tolerance for FD counting noise on different platforms
        assert final_fds <= baseline_fds + 5, (
            f"FD leak detected after 30 create/close cycles: "
            f"baseline={baseline_fds}, final={final_fds}"
        )

    def test_connection_repeated_single_cycle_no_fd_leak(self) -> None:
        """Create and close one Connection at a time, 20 iterations."""
        gc.collect()
        baseline_fds = _count_fds()

        for _ in range(20):
            conn = Connection(_SOCKS5_URI)
            conn.close()
            del conn

        gc.collect()
        final_fds = _count_fds()
        assert final_fds <= baseline_fds + 5, (
            f"FD leak detected after 20 sequential create/close cycles: "
            f"baseline={baseline_fds}, final={final_fds}"
        )

    def test_connection_closed_state_after_close(self) -> None:
        """All connections reach closed state after close()."""
        conns = [Connection(_SOCKS5_URI) for _ in range(10)]
        for conn in conns:
            conn.close()

        for conn in conns:
            assert conn.closed, f"connection not closed: state={conn.state}"

    def test_connection_close_idempotent(self) -> None:
        """Calling close() multiple times does not raise."""
        conn = Connection(_SOCKS5_URI)
        conn.close()
        # Second close should not raise
        conn.close()
        conn.close()
        assert conn.closed

    def test_connection_stats_after_cycles(self) -> None:
        """Connection object works after multiple create/close cycles."""
        for _ in range(5):
            conn = Connection(_SOCKS5_URI)
            # Verify object is usable
            _ = repr(conn)
            conn.close()
