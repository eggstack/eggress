import asyncio
import socket
import struct
import threading
import time

import pytest
from eggress.config import EggressConfig
from eggress.pproxy import AlreadyStartedError, Server, UnsupportedFeatureError


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _echo_server():
    """Start a TCP echo server on 127.0.0.1:0, return (host, port, server_socket)."""
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", 0))
    srv.listen(5)
    host, port = srv.getsockname()

    def _accept():
        while True:
            try:
                conn, _ = srv.accept()
            except OSError:
                break
            try:
                while True:
                    data = conn.recv(4096)
                    if not data:
                        break
                    conn.sendall(data)
            finally:
                conn.close()

    t = threading.Thread(target=_accept, daemon=True)
    t.start()
    return host, port, srv


def _echo_conn(host, port, payload=b"hello"):
    """Connect to echo server, send payload, return response."""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(3.0)
    s.connect((host, port))
    s.sendall(payload)
    resp = s.recv(4096)
    s.close()
    return resp


def _socks5_connect(proxy_host, proxy_port, target_host, target_port):
    """Perform a SOCKS5 CONNECT handshake and return the connected socket."""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(5.0)
    s.connect((proxy_host, proxy_port))
    # Greeting: version 5, 1 auth method (no auth)
    s.sendall(b"\x05\x01\x00")
    resp = s.recv(2)
    assert resp[0] == 0x05, f"SOCKS5 greeting version mismatch: {resp!r}"
    # CONNECT request using IPv4 literal (ATYP=0x01) so the runtime treats
    # the target as a literal IP and bypasses DNS rebinding protection.
    octets = [int(x) for x in target_host.split(".")]
    req = (
        b"\x05\x01\x00\x01"
        + bytes(octets)
        + struct.pack("!H", target_port)
    )
    s.sendall(req)
    resp = s.recv(32)
    assert resp[1] == 0x00, f"SOCKS5 connect failed: {resp!r}"
    return s


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


def test_import_server():
    """Importing Server and AlreadyStartedError works."""
    from eggress.pproxy import Server, AlreadyStartedError  # noqa: F401

    assert Server is not None
    assert AlreadyStartedError is not None


def test_construct_with_listen_remote():
    """Server(listen=[...]) does not raise."""
    srv = Server(listen=["http://127.0.0.1:0"])
    assert srv is not None
    srv.close()


VALID_TOML = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""


def test_construct_with_config():
    """Server(config=eggress_config) does not raise."""
    cfg = EggressConfig.from_toml(VALID_TOML)
    srv = Server(config=cfg)
    assert srv is not None
    srv.close()


def test_construct_no_args_raises():
    """Server() raises ValueError."""
    with pytest.raises(ValueError, match="listen/remote or config"):
        Server()


def test_construct_conflicting_args_raises():
    """Server(listen=[...], config=...) raises ValueError."""
    cfg = EggressConfig.from_toml(VALID_TOML)
    with pytest.raises(ValueError, match="mutually exclusive"):
        Server(listen=["http://127.0.0.1:0"], config=cfg)


def test_addresses_empty_before_start():
    """addresses returns empty dict before start()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        assert srv.addresses == {}
    finally:
        srv.close()


def test_start_and_stop():
    """Start with HTTP listener on port 0, verify addresses non-empty, stop."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        srv.start()
        time.sleep(0.1)
        addrs = srv.addresses
        assert len(addrs) > 0
        for key, addr in addrs.items():
            assert addr != ""
    finally:
        srv.close()


def test_start_returns_self():
    """start() returns self for chaining."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        result = srv.start()
        assert result is srv
    finally:
        srv.close()


def test_addresses_after_stop():
    """After close(), addresses returns empty dict."""
    srv = Server(listen=["http://127.0.0.1:0"])
    srv.start()
    time.sleep(0.1)
    srv.close()
    assert srv.addresses == {}


def test_double_start_raises():
    """Starting an already-running server raises AlreadyStartedError."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        srv.start()
        time.sleep(0.1)
        with pytest.raises(AlreadyStartedError):
            srv.start()
    finally:
        srv.close()


def test_close_is_idempotent():
    """Closing twice does not raise."""
    srv = Server(listen=["http://127.0.0.1:0"])
    srv.start()
    time.sleep(0.1)
    srv.close()
    srv.close()  # should not raise


def test_stop_alias():
    """stop() works the same as close()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    srv.start()
    time.sleep(0.1)
    srv.stop()
    assert srv.addresses == {}


def test_sync_context_manager():
    """with Server(...) as srv: starts and stops."""
    with Server(listen=["http://127.0.0.1:0"]) as srv:
        time.sleep(0.1)
        assert len(srv.addresses) > 0
    assert srv.addresses == {}


def test_async_context_manager():
    """async with Server(...) as srv: starts and stops."""
    async def _run():
        async with Server(listen=["http://127.0.0.1:0"]) as srv:
            await asyncio.sleep(0.1)
            assert len(srv.addresses) > 0
        assert srv.addresses == {}

    asyncio.run(_run())


def test_unsupported_uri_raises():
    """Server(listen=["ssh://..."]) raises UnsupportedFeatureError."""
    with pytest.raises(UnsupportedFeatureError):
        Server(listen=["ssh://127.0.0.1:22"])


def test_allow_partial_with_unsupported():
    """Server(listen=["ssh://..."], allow_partial=True) does not raise."""
    srv = Server(listen=["ssh://127.0.0.1:22"], allow_partial=True)
    assert srv is not None
    srv.close()


def test_repr():
    """repr(server) shows 'Server(stopped)' or 'Server(running)'."""
    srv = Server(listen=["http://127.0.0.1:0"])
    assert repr(srv) == "Server(stopped)"
    srv.start()
    time.sleep(0.1)
    try:
        assert repr(srv) == "Server(running)"
    finally:
        srv.close()
    assert repr(srv) == "Server(stopped)"


def test_connect_through_socks5():
    """SOCKS5 listener (direct mode) relays data to an echo server."""
    echo_host, echo_port, echo_srv = _echo_server()
    try:
        with Server(listen=["socks5://127.0.0.1:0"]) as srv:
            time.sleep(0.1)
            addrs = srv.addresses
            assert len(addrs) > 0
            # Find the socks5 listener address
            socks_addr = None
            for key, addr in addrs.items():
                if addr:
                    host, port = addr.rsplit(":", 1)
                    socks_addr = (host, int(port))
                    break
            assert socks_addr is not None, f"No listener address found: {addrs}"

            s = _socks5_connect(socks_addr[0], socks_addr[1], echo_host, echo_port)
            try:
                s.sendall(b"ping")
                resp = s.recv(4096)
                assert resp == b"ping"
            finally:
                s.close()
    finally:
        echo_srv.close()


def test_multiple_listeners():
    """Two listeners on port 0 both show in addresses."""
    with Server(
        listen=[
            "socks5://127.0.0.1:0",
            "http://127.0.0.1:0",
        ],
    ) as srv:
        time.sleep(0.1)
        addrs = srv.addresses
        assert len(addrs) == 2
        for key, addr in addrs.items():
            assert addr != ""


def test_config_property():
    """Pre-built config works with Server."""
    cfg = EggressConfig.from_toml(VALID_TOML)
    with Server(config=cfg) as srv:
        time.sleep(0.1)
        assert len(srv.addresses) > 0


def test_server_config_property_from_config():
    """Server(config=cfg).config returns the same EggressConfig instance."""
    cfg = EggressConfig.from_toml(VALID_TOML)
    srv = Server(config=cfg)
    try:
        assert srv.config is cfg
    finally:
        srv.close()


def test_server_config_property_before_start():
    """Server.config is available before start()."""
    cfg = EggressConfig.from_toml(VALID_TOML)
    srv = Server(config=cfg)
    assert srv.config is cfg
    srv.close()


def test_server_config_property_from_listen_remote():
    """Server(listen=[...]).config returns the translated EggressConfig."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        cfg = srv.config
        assert isinstance(cfg, EggressConfig)
        redacted = cfg.redacted_toml()
        assert "version = 1" in redacted
        assert "[[listeners]]" in redacted
    finally:
        srv.close()


def test_server_config_property_after_start():
    """Server.config remains accessible after start()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        cfg_before = srv.config
        srv.start()
        time.sleep(0.1)
        cfg_after = srv.config
        assert cfg_before is cfg_after
    finally:
        srv.close()


def test_server_config_property_after_close():
    """Server.config remains accessible after close()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    srv.start()
    time.sleep(0.1)
    srv.close()
    assert isinstance(srv.config, EggressConfig)


def test_wait_closed():
    """wait_closed() returns once the server is shut down."""
    async def _run():
        srv = Server(listen=["http://127.0.0.1:0"])
        srv.start()
        await asyncio.sleep(0.1)
        assert len(srv.addresses) > 0
        # Close in background so wait_closed can observe the transition
        async def _close_later():
            await asyncio.sleep(0.05)
            await srv.aclose()
        task = asyncio.create_task(_close_later())
        await srv.wait_closed()
        assert srv.addresses == {}
        await task

    asyncio.run(_run())


def test_wait_closed_already_stopped():
    """wait_closed() returns immediately if server never started."""
    async def _run():
        srv = Server(listen=["http://127.0.0.1:0"])
        await srv.wait_closed()
        assert srv.addresses == {}

    asyncio.run(_run())


# ---------------------------------------------------------------------------
# Phase C3: Enhanced observability and lifecycle tests
# ---------------------------------------------------------------------------


def test_status_before_start():
    """status() returns empty dict before start()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        assert srv.status() == {}
    finally:
        srv.close()


def test_status_after_start():
    """status() returns a dict with readiness after start()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        srv.start()
        time.sleep(0.1)
        st = srv.status()
        assert isinstance(st, dict)
        assert "readiness" in st
        assert st["readiness"] is True
    finally:
        srv.close()


def test_status_after_close():
    """status() returns empty dict after close()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    srv.start()
    time.sleep(0.1)
    srv.close()
    assert srv.status() == {}


def test_sessions_before_start():
    """sessions returns 0 before start()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        assert srv.sessions == 0
    finally:
        srv.close()


def test_sessions_after_start():
    """sessions returns a non-negative integer after start()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        srv.start()
        time.sleep(0.1)
        assert isinstance(srv.sessions, int)
        assert srv.sessions >= 0
    finally:
        srv.close()


def test_sessions_after_close():
    """sessions returns 0 after close()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    srv.start()
    time.sleep(0.1)
    srv.close()
    assert srv.sessions == 0


def test_last_error_none_initially():
    """last_error is None when server is freshly constructed."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        assert srv.last_error is None
    finally:
        srv.close()


def test_last_error_none_after_success():
    """last_error remains None after successful start and close."""
    srv = Server(listen=["http://127.0.0.1:0"])
    srv.start()
    time.sleep(0.1)
    srv.close()
    assert srv.last_error is None


def test_last_error_on_start_failure():
    """last_error is set when start() fails."""
    from eggress._eggress import ConfigError

    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        # Force an error by trying to start with a bad config
        # We can simulate this by directly setting a bad service
        original_service = srv._service

        class _FailingService:
            def start(self):
                raise ConfigError("test error")

        srv._service = _FailingService()
        with pytest.raises(ConfigError):
            srv.start()
        assert srv.last_error is not None
        assert "test error" in str(srv.last_error)
    finally:
        srv._service = original_service
        srv.close()


def test_reload_before_start_raises():
    """reload() raises RuntimeError if server is not running."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        with pytest.raises(RuntimeError, match="not running"):
            srv.reload("version = 1\n")
    finally:
        srv.close()


def test_reload_after_start():
    """reload() succeeds with valid TOML while server is running."""
    cfg = EggressConfig.from_toml(VALID_TOML)
    with Server(config=cfg) as srv:
        time.sleep(0.1)
        # Reload with same config (should succeed since listener names match)
        result = srv.reload(VALID_TOML)
        assert isinstance(result, dict)


def test_is_ready_after_start():
    """is_ready returns True after server starts successfully."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        srv.start()
        time.sleep(0.1)
        assert srv.is_ready is True
    finally:
        srv.close()


def test_is_ready_before_start():
    """is_ready returns False before start()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        assert srv.is_ready is False
    finally:
        srv.close()


def test_is_ready_after_close():
    """is_ready returns False after close()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    srv.start()
    time.sleep(0.1)
    srv.close()
    assert srv.is_ready is False


def test_listener_info_before_start():
    """listener_info returns empty list before start()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        assert srv.listener_info == []
    finally:
        srv.close()


def test_listener_info_after_start():
    """listener_info returns a non-empty list after start()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        srv.start()
        time.sleep(0.1)
        info = srv.listener_info
        assert isinstance(info, list)
        assert len(info) > 0
    finally:
        srv.close()


def test_metrics_text_before_start():
    """metrics_text returns empty string before start()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        assert srv.metrics_text == ""
    finally:
        srv.close()


def test_metrics_text_after_start():
    """metrics_text returns non-empty string after start()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        srv.start()
        time.sleep(0.1)
        metrics = srv.metrics_text
        assert isinstance(metrics, str)
        assert len(metrics) > 0
    finally:
        srv.close()


def test_metrics_text_after_close():
    """metrics_text returns empty string after close()."""
    srv = Server(listen=["http://127.0.0.1:0"])
    srv.start()
    time.sleep(0.1)
    srv.close()
    assert srv.metrics_text == ""


def test_del_warns_if_not_closed():
    """__del__ issues ResourceWarning if server was not properly closed."""
    import gc
    import warnings

    srv = Server(listen=["http://127.0.0.1:0"])
    srv.start()
    time.sleep(0.1)
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always")
        del srv
        gc.collect()
        resource_warnings = [x for x in w if issubclass(x.category, ResourceWarning)]
        assert len(resource_warnings) >= 1
        assert "not properly closed" in str(resource_warnings[0].message)


def test_del_no_warning_if_closed():
    """__del__ does not issue ResourceWarning if server was properly closed."""
    import gc
    import warnings

    srv = Server(listen=["http://127.0.0.1:0"])
    srv.start()
    time.sleep(0.1)
    srv.close()
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always")
        del srv
        gc.collect()
        resource_warnings = [x for x in w if issubclass(x.category, ResourceWarning)]
        assert len(resource_warnings) == 0


def test_concurrent_sessions_through_socks5():
    """Multiple concurrent SOCKS5 connections through the server."""
    echo_host, echo_port, echo_srv = _echo_server()
    try:
        with Server(listen=["socks5://127.0.0.1:0"]) as srv:
            time.sleep(0.1)
            addrs = srv.addresses
            socks_addr = None
            for key, addr in addrs.items():
                if addr:
                    host, port = addr.rsplit(":", 1)
                    socks_addr = (host, int(port))
                    break
            assert socks_addr is not None

            # Establish 3 connections and verify each works independently
            for i in range(3):
                s = _socks5_connect(
                    socks_addr[0], socks_addr[1], echo_host, echo_port
                )
                try:
                    payload = f"msg{i}".encode()
                    s.sendall(payload)
                    resp = s.recv(4096)
                    assert resp == payload
                finally:
                    s.close()
    finally:
        echo_srv.close()


def test_server_coexists_with_pproxyservice():
    """Server and PPProxyService can coexist in the same process."""
    from eggress.pproxy import PPProxyService

    with Server(listen=["http://127.0.0.1:0"]) as srv:
        time.sleep(0.1)
        assert len(srv.addresses) > 0
        with PPProxyService(listen=["socks5://127.0.0.1:0"]) as handle:
            time.sleep(0.1)
            assert len(handle.bound_addresses) > 0


def test_multiple_independent_servers():
    """Multiple Server instances can run independently."""
    with Server(listen=["http://127.0.0.1:0"]) as srv1:
        time.sleep(0.1)
        with Server(listen=["socks5://127.0.0.1:0"]) as srv2:
            time.sleep(0.1)
            assert len(srv1.addresses) > 0
            assert len(srv2.addresses) > 0
            # They should have different addresses
            addrs1 = set(srv1.addresses.values())
            addrs2 = set(srv2.addresses.values())
            assert addrs1 != addrs2


def test_run_from_main_thread():
    """run() can be called from the main thread (starts and blocks)."""
    import signal

    srv = Server(listen=["http://127.0.0.1:0"])
    started = threading.Event()

    def _run_in_thread():
        # Signal main thread to send SIGINT
        time.sleep(0.2)
        # Send SIGINT to self (main thread)
        import os
        os.kill(os.getpid(), signal.SIGINT)

    t = threading.Thread(target=_run_in_thread, daemon=True)
    t.start()

    # run() should block until SIGINT
    srv.run()
    # After run returns, server should be closed
    assert srv.addresses == {}


def test_run_from_non_main_thread_raises():
    """run() raises RuntimeError when called from non-main thread."""
    srv = Server(listen=["http://127.0.0.1:0"])
    error = []

    def _try_run():
        try:
            srv.run()
        except RuntimeError as e:
            error.append(e)

    t = threading.Thread(target=_try_run)
    t.start()
    t.join(timeout=2.0)
    srv.close()
    assert len(error) == 1
    assert "main thread" in str(error[0])


def test_construct_bad_config_type_raises():
    """Server(config=not_an_eggress_config) raises TypeError."""
    with pytest.raises(TypeError, match="EggressConfig"):
        Server(config="not a config")


def test_multiple_listeners_different_protocols():
    """Multiple listeners with different protocols work correctly."""
    echo_host, echo_port, echo_srv = _echo_server()
    try:
        with Server(
            listen=[
                "socks5://127.0.0.1:0",
                "http://127.0.0.1:0",
            ],
        ) as srv:
            time.sleep(0.1)
            addrs = srv.addresses
            assert len(addrs) == 2

            # Both should be reachable
            socks_addr = None
            http_addr = None
            for key, addr in addrs.items():
                if addr:
                    host, port = addr.rsplit(":", 1)
                    if socks_addr is None:
                        socks_addr = (host, int(port))
                    else:
                        http_addr = (host, int(port))

            if socks_addr:
                s = _socks5_connect(
                    socks_addr[0], socks_addr[1], echo_host, echo_port
                )
                s.sendall(b"test")
                resp = s.recv(4096)
                assert resp == b"test"
                s.close()
    finally:
        echo_srv.close()


def test_chaining_start_returns_self():
    """start() returns self enabling method chaining."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        result = srv.start().addresses
        assert isinstance(result, dict)
        assert len(result) > 0
    finally:
        srv.close()
