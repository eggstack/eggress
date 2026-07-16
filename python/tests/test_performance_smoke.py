"""Python binding performance and overhead smoke tests (Phase 34).

These tests verify that the Python bindings meet reasonable latency and
throughput thresholds. They are *smoke tests*, not microbenchmarks —
thresholds are generous to accommodate CI variance.
"""

import time
import threading

import pytest


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

MULTI_LISTENER_TOML = """\
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[listeners]]
name = "http"
bind = "127.0.0.1:0"
protocols = ["http"]

[[listeners]]
name = "socks4"
bind = "127.0.0.1:0"
protocols = ["socks4"]
"""

MINIMAL_TOML = """\
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""


# ---------------------------------------------------------------------------
# 1. Import cost
# ---------------------------------------------------------------------------


def test_import_cost():
    """Verify eggress imports in reasonable time.

    Cold-start import of the native module + Python wrappers should
    complete well under 500 ms on any modern machine.

    Uses a subprocess to measure cold-import without polluting the
    current process's sys.modules (which would break subsequent tests
    that captured stale references to wrapper classes like
    ``EggressConfig``).
    """
    import subprocess
    import sys

    code = (
        "import time;"
        "t0=time.perf_counter();"
        "import eggress;"
        "print(int((time.perf_counter()-t0)*1000))"
    )
    result = subprocess.run(
        [sys.executable, "-c", code],
        capture_output=True,
        text=True,
        timeout=10,
    )
    if result.returncode != 0 and "incompatible architecture" in result.stderr:
        pytest.skip("native extension architecture mismatch in subprocess")
    assert result.returncode == 0, (
        f"subprocess import failed: {result.stderr!r}"
    )
    elapsed_ms = int(result.stdout.strip())
    assert elapsed_ms < 500, (
        f"import eggress took {elapsed_ms} ms (threshold: 500 ms)"
    )


# ---------------------------------------------------------------------------
# 2. URI translation overhead
# ---------------------------------------------------------------------------


def test_check_pproxy_uri_cost():
    """Measure average latency of check_pproxy_uri calls.

    URI parsing is a pure-native operation. 100 invocations of a simple
    HTTP URI should average under 10 ms each.
    """
    import eggress

    uri = "http://user:pass@example.com:8080"
    n = 100

    t0 = time.perf_counter()
    for _ in range(n):
        info = eggress.check_pproxy_uri(uri)
    elapsed = time.perf_counter() - t0

    avg_us = (elapsed / n) * 1_000_000
    assert avg_us < 10_000, (
        f"check_pproxy_uri averaged {avg_us:.0f} us/call (threshold: 10 000 us)"
    )


# ---------------------------------------------------------------------------
# 3. Config parse cost
# ---------------------------------------------------------------------------


def test_config_parse_cost():
    """Measure time to parse and validate a multi-listener TOML config.

    Config compilation is expected to be fast (< 500 ms) since it involves
    TOML deserialization and validation only — no network operations.
    """
    from eggress import EggressConfig

    t0 = time.perf_counter()
    for _ in range(10):
        EggressConfig.from_toml(MULTI_LISTENER_TOML)
    elapsed = time.perf_counter() - t0

    assert elapsed < 0.5, (
        f"10× config parse took {elapsed:.3f}s (threshold: 0.5s)"
    )


# ---------------------------------------------------------------------------
# 4. Service start/stop cost
# ---------------------------------------------------------------------------


def test_service_start_stop_cost():
    """Measure service start and shutdown overhead.

    Start should bind listeners and become ready within 1 s.
    Shutdown should complete within 2 s.
    """
    import eggress

    svc = eggress.EggressService.from_toml(MINIMAL_TOML)

    t0 = time.perf_counter()
    handle = svc.start()
    start_elapsed = time.perf_counter() - t0

    assert handle.status()["readiness"] is True
    assert start_elapsed < 1.0, (
        f"service start took {start_elapsed:.3f}s (threshold: 1.0s)"
    )

    t0 = time.perf_counter()
    handle.shutdown()
    stop_elapsed = time.perf_counter() - t0

    assert stop_elapsed < 2.0, (
        f"service shutdown took {stop_elapsed:.3f}s (threshold: 2.0s)"
    )


# ---------------------------------------------------------------------------
# 5. Status query cost
# ---------------------------------------------------------------------------


def test_status_query_cost():
    """Measure average latency of status() calls on a running service.

    Status queries hit an in-memory snapshot and should be very fast
    (< 5 ms each on average).
    """
    import eggress

    svc = eggress.EggressService.from_toml(MINIMAL_TOML)
    handle = svc.start()
    try:
        n = 100
        t0 = time.perf_counter()
        for _ in range(n):
            status = handle.status()
        elapsed = time.perf_counter() - t0

        assert status["readiness"] is True
        avg_us = (elapsed / n) * 1_000_000
        assert avg_us < 5_000, (
            f"status() averaged {avg_us:.0f} us/call (threshold: 5 000 us)"
        )
    finally:
        handle.shutdown()


# ---------------------------------------------------------------------------
# 6. Concurrent GIL release
# ---------------------------------------------------------------------------


def test_concurrent_gil_release():
    """Verify GIL is released during concurrent native operations.

    Multiple threads performing URI translations concurrently must
    complete without deadlock. This is a correctness smoke test — we
    just verify all threads finish and produce expected results.
    """
    import eggress

    uris = [
        "socks5://127.0.0.1:1080",
        "http://proxy.example.com:8080",
        "socks4://10.0.0.1:1080",
        "ss://aes-256-gcm:secret@10.0.0.1:8388",
    ]
    results = [None] * len(uris)
    errors = [None] * len(uris)

    def _check_uri(idx, uri):
        try:
            info = eggress.check_pproxy_uri(uri)
            results[idx] = info.ok
        except Exception as exc:
            errors[idx] = exc

    threads = [
        threading.Thread(target=_check_uri, args=(i, uri))
        for i, uri in enumerate(uris)
    ]

    t0 = time.perf_counter()
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=5.0)
    elapsed = time.perf_counter() - t0

    # All threads must have completed (no deadlocks)
    assert all(t.is_alive() is False for t in threads), (
        "some threads did not finish within timeout — possible deadlock"
    )
    # No errors from worker threads
    for i, err in enumerate(errors):
        assert err is None, f"thread {i} raised: {err}"
    # All URIs parsed successfully
    assert all(results), f"not all URIs parsed ok: {results}"
    # Reasonable total time (generous — just a smoke check)
    assert elapsed < 5.0, (
        f"concurrent GIL-release test took {elapsed:.3f}s (threshold: 5.0s)"
    )
