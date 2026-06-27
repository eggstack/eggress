import threading
import time

from eggress import EggressService

VALID_TOML = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""


def test_gil_released_during_start():
    """Verify GIL is released during start() by running a counter in another thread."""
    counter = 0
    done = threading.Event()

    def increment():
        nonlocal counter
        while not done.is_set():
            counter += 1

    t = threading.Thread(target=increment)
    t.start()

    with EggressService.from_toml(VALID_TOML).start() as handle:
        done.set()
        t.join(timeout=2)
        assert counter > 0, "GIL was not released: counter did not advance during start()"
        assert handle.status()["readiness"] is True


def test_concurrent_operations():
    """Verify multiple threads can call handle methods without deadlock."""
    results = {}
    errors = []

    def read_status(handle, key):
        try:
            results[key] = handle.status()
        except Exception as e:
            errors.append(e)

    def read_metrics(handle, key):
        try:
            results[key] = handle.metrics_text()
        except Exception as e:
            errors.append(e)

    with EggressService.from_toml(VALID_TOML).start() as handle:
        threads = [
            threading.Thread(target=read_status, args=(handle, "status1")),
            threading.Thread(target=read_metrics, args=(handle, "metrics1")),
            threading.Thread(target=read_status, args=(handle, "status2")),
            threading.Thread(target=read_metrics, args=(handle, "metrics2")),
        ]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=5)

        assert not errors, f"Concurrent calls raised errors: {errors}"
        assert "status1" in results
        assert "metrics1" in results
        assert results["status1"]["readiness"] is True
        assert "eggress_connections_total" in results["metrics1"]


def test_shutdown_returns_promptly():
    """Verify shutdown() returns within a reasonable time."""
    handle = EggressService.from_toml(VALID_TOML).start()
    # Ensure service is ready
    assert handle.status()["readiness"] is True

    start = time.monotonic()
    handle.shutdown()
    elapsed = time.monotonic() - start

    assert elapsed < 5.0, f"shutdown() took {elapsed:.2f}s, expected < 5s"
