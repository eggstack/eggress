"""Concurrency and multi-service behavior tests for pproxy compat layer."""

import threading

from eggress import EggressService


VALID_TOML = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""


def test_two_services_on_port_0():
    """Start two services on port 0, verify independent bound addresses."""
    svc1 = EggressService.from_toml(VALID_TOML)
    svc2 = EggressService.from_toml(VALID_TOML)

    h1 = svc1.start()
    h2 = svc2.start()

    try:
        addrs1 = h1.bound_addresses
        addrs2 = h2.bound_addresses

        assert "socks" in addrs1
        assert "socks" in addrs2
        # Different ports
        assert addrs1["socks"] != addrs2["socks"]
    finally:
        h1.shutdown()
        h2.shutdown()


def test_one_service_shutdown_does_not_kill_other():
    """Shutting down one service should not affect the other."""
    svc1 = EggressService.from_toml(VALID_TOML)
    svc2 = EggressService.from_toml(VALID_TOML)

    h1 = svc1.start()
    h2 = svc2.start()

    try:
        h1.shutdown()
        # h2 should still be alive
        assert h2.status()["readiness"] is True
    finally:
        h2.shutdown()


def test_concurrent_metrics_text_calls():
    """Multiple threads calling metrics_text() should not deadlock."""
    handle = EggressService.from_toml(VALID_TOML).start()
    errors = []

    def read_metrics():
        try:
            m = handle.metrics_text()
            assert "eggress_connections_total" in m
        except Exception as e:
            errors.append(e)

    threads = [threading.Thread(target=read_metrics) for _ in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=5)

    handle.shutdown()
    assert not errors, f"Concurrent metrics calls raised: {errors}"


def test_concurrent_status_calls():
    """Multiple threads calling status() should not deadlock."""
    handle = EggressService.from_toml(VALID_TOML).start()
    errors = []

    def read_status():
        try:
            s = handle.status()
            assert s["readiness"] is True
        except Exception as e:
            errors.append(e)

    threads = [threading.Thread(target=read_status) for _ in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=5)

    handle.shutdown()
    assert not errors, f"Concurrent status calls raised: {errors}"


def test_concurrent_reload_and_metrics():
    """Concurrent reload and metrics calls should not deadlock."""
    handle = EggressService.from_toml(VALID_TOML).start()
    errors = []

    def read_metrics():
        try:
            handle.metrics_text()
        except Exception as e:
            errors.append(e)

    def do_reload():
        try:
            handle.reload_toml(VALID_TOML)
        except Exception as e:
            errors.append(e)

    threads = [
        threading.Thread(target=read_metrics),
        threading.Thread(target=do_reload),
        threading.Thread(target=read_metrics),
        threading.Thread(target=do_reload),
    ]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=5)

    handle.shutdown()
    assert not errors, f"Concurrent reload/metrics raised: {errors}"


def test_thread_start_shutdown_smoke():
    """Start and shutdown from a non-main thread should work."""
    handle_holder = [None]
    error_holder = [None]

    def worker():
        try:
            svc = EggressService.from_toml(VALID_TOML)
            handle = svc.start()
            handle_holder[0] = handle
            assert handle.status()["readiness"] is True
            handle.shutdown()
        except Exception as e:
            error_holder[0] = e

    t = threading.Thread(target=worker)
    t.start()
    t.join(timeout=10)

    assert error_holder[0] is None, f"Thread worker failed: {error_holder[0]}"
    assert handle_holder[0] is not None
