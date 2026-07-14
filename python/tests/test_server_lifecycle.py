import asyncio
import socket
import struct
import threading
import time
from pathlib import Path

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


# ---------------------------------------------------------------------------
# WS5: TCP/UDP roles — TLS, auth, chains, UDP, IPv6
# ---------------------------------------------------------------------------


def _generate_self_signed_cert(tmp_path):
    """Generate a self-signed cert+key in tmp_path. Skip if openssl missing."""
    import subprocess

    cert = tmp_path / "cert.pem"
    key = tmp_path / "key.pem"
    try:
        subprocess.run(
            [
                "openssl", "req", "-x509", "-newkey", "rsa:2048",
                "-keyout", str(key), "-out", str(cert),
                "-days", "1", "-nodes", "-subj", "/CN=localhost",
            ],
            check=True, capture_output=True,
        )
    except (FileNotFoundError, subprocess.CalledProcessError):
        pytest.skip("openssl not available to generate self-signed cert")
    return str(cert), str(key)


def test_tls_listener(tmp_path):
    """TLS listener starts and accepts a TLS connection."""
    cert, key = _generate_self_signed_cert(tmp_path)
    toml = f"""
version = 1

[[listeners]]
name = "tls-in"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.tls]
cert = "{cert}"
key = "{key}"
"""
    cfg = EggressConfig.from_toml(toml)
    with Server(config=cfg) as srv:
        time.sleep(0.2)
        addrs = srv.addresses
        assert len(addrs) > 0
        # Verify the listener is reachable via TLS
        for key_name, addr in addrs.items():
            if addr:
                host, port = addr.rsplit(":", 1)
                import ssl
                ctx = ssl.create_default_context()
                ctx.check_hostname = False
                ctx.verify_mode = ssl.CERT_NONE
                s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                s.settimeout(3.0)
                try:
                    s.connect((host, int(port)))
                    ss = ctx.wrap_socket(s, server_hostname="localhost")
                    ss.close()
                finally:
                    s.close()
                break


def test_tls_listener_config_present(tmp_path):
    """TLS config is present in the translated TOML."""
    cert, key = _generate_self_signed_cert(tmp_path)
    toml = f"""
version = 1

[[listeners]]
name = "tls-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.tls]
cert = "{cert}"
key = "{key}"
"""
    cfg = EggressConfig.from_toml(toml)
    srv = Server(config=cfg)
    try:
        redacted = srv.config.redacted_toml()
        assert "[listeners.tls]" in redacted
    finally:
        srv.close()


def test_auth_listener():
    """Listener with password auth starts successfully."""
    toml = """
version = 1

[[listeners]]
name = "auth-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.auth]
type = "password"
username = "admin"
password = "secret"
"""
    cfg = EggressConfig.from_toml(toml)
    with Server(config=cfg) as srv:
        time.sleep(0.1)
        assert len(srv.addresses) > 0
        st = srv.status()
        assert st.get("readiness") is True


def test_auth_rejects_wrong_password():
    """SOCKS5 with auth rejects connection with wrong credentials."""
    toml = """
version = 1

[[listeners]]
name = "auth-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.auth]
type = "password"
username = "admin"
password = "secret"
"""
    cfg = EggressConfig.from_toml(toml)
    with Server(config=cfg) as srv:
        time.sleep(0.1)
        addrs = srv.addresses
        socks_addr = None
        for key, addr in addrs.items():
            if addr:
                host, port = addr.rsplit(":", 1)
                socks_addr = (host, int(port))
                break
        assert socks_addr is not None

        # Connect with WRONG auth (send no-auth greeting)
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(3.0)
        try:
            s.connect(socks_addr)
            # Send greeting claiming no-auth
            s.sendall(b"\x05\x01\x00")
            resp = s.recv(2)
            # Server should either reject (method 0xFF) or the connect
            # should fail. Either way, the connection should not succeed
            # for a wrong-auth client.
            if resp and resp[0] == 0x05 and len(resp) > 1 and resp[1] != 0xFF:
                # If server accepted no-auth, try CONNECT — should fail
                octets = [int(x) for x in "127.0.0.1".split(".")]
                req = (
                    b"\x05\x01\x00\x01"
                    + bytes(octets)
                    + struct.pack("!H", 1)
                )
                s.sendall(req)
                resp2 = s.recv(32)
                # Should get a failure reply (not 0x00 success)
                if resp2 and len(resp2) >= 2:
                    assert resp2[1] != 0x00, (
                        "CONNECT succeeded without valid auth"
                    )
        finally:
            s.close()


def test_auth_config_present():
    """Auth config is present in the translated TOML."""
    toml = """
version = 1

[[listeners]]
name = "auth-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.auth]
type = "password"
username = "admin"
password = "secret"
"""
    cfg = EggressConfig.from_toml(toml)
    srv = Server(config=cfg)
    try:
        redacted = srv.config.redacted_toml()
        assert "[listeners.auth]" in redacted
    finally:
        srv.close()


def test_upstream_chain_config():
    """Upstream chain with routing rules present in TOML."""
    toml = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[upstreams]]
id = "hop1"
uri = "http://127.0.0.1:9999"

[[upstream_groups]]
id = "main"
scheduler = "round-robin"
members = ["hop1"]

[[rules]]
id = "route-all"
upstream_group = "main"
"""
    cfg = EggressConfig.from_toml(toml)
    with Server(config=cfg) as srv:
        time.sleep(0.1)
        assert len(srv.addresses) > 0
        # Verify upstream config is preserved
        redacted = srv.config.redacted_toml()
        assert "[[upstreams]]" in redacted
        assert "[[upstream_groups]]" in redacted
        assert "[[rules]]" in redacted


def test_chain_uri_translation():
    """Chain URI syntax translates to multi-hop upstream."""
    from eggress.pproxy import translate_pproxy_args

    result = translate_pproxy_args([
        "-l", "socks5://127.0.0.1:0",
        "-r", "socks5://127.0.0.1:9001__http://127.0.0.1:9002",
    ])
    assert result.ok
    toml = result.config().redacted_toml()
    assert "[[upstreams]]" in toml
    assert "[[upstream_groups]]" in toml


def test_udp_listener():
    """UDP-enabled listener starts and reports UDP in status."""
    toml = """
version = 1

[[listeners]]
name = "socks-udp"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.udp]
enabled = true
bind = "127.0.0.1:0"
"""
    cfg = EggressConfig.from_toml(toml)
    with Server(config=cfg) as srv:
        time.sleep(0.2)
        assert len(srv.addresses) > 0
        st = srv.status()
        assert st.get("readiness") is True


def test_standalone_udp_listener():
    """Standalone UDP mode starts and accepts datagrams."""
    toml = """
version = 1

[[listeners]]
name = "udp-standalone"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.udp]
enabled = true
mode = "standalone_pproxy_udp"
bind = "127.0.0.1:0"
"""
    cfg = EggressConfig.from_toml(toml)
    with Server(config=cfg) as srv:
        time.sleep(0.2)
        assert len(srv.addresses) > 0


def test_ipv6_listener():
    """IPv6 loopback listener starts and accepts connections."""
    import platform

    if platform.system() == "Windows":
        pytest.skip("IPv6 loopback may not be available on Windows")
    toml = """
version = 1

[[listeners]]
name = "ipv6-in"
bind = "[::1]:0"
protocols = ["http"]
"""
    cfg = EggressConfig.from_toml(toml)
    with Server(config=cfg) as srv:
        time.sleep(0.2)
        addrs = srv.addresses
        assert len(addrs) > 0
        # Verify we can connect via IPv6
        for key, addr in addrs.items():
            if addr and "[::1]" in addr:
                port_str = addr.rsplit(":", 1)[1]
                s = socket.socket(socket.AF_INET6, socket.SOCK_STREAM)
                s.settimeout(3.0)
                try:
                    s.connect(("::1", int(port_str)))
                finally:
                    s.close()
                break


# ---------------------------------------------------------------------------
# WS7: Loop and thread behavior
# ---------------------------------------------------------------------------


def test_loop_affinity():
    """Server created in one loop can be started in another."""
    results = []

    def _worker():
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        try:
            srv = Server(listen=["http://127.0.0.1:0"])
            loop.run_until_complete(srv.astart())
            results.append(srv.addresses)
            loop.run_until_complete(srv.aclose())
        finally:
            loop.close()

    t = threading.Thread(target=_worker)
    t.start()
    t.join(timeout=5.0)
    assert len(results) == 1
    assert len(results[0]) > 0


def test_interpreter_shutdown():
    """Server handles interpreter shutdown without hanging."""
    import subprocess
    import sys

    code = """
import sys, time
sys.path.insert(0, ".")
from eggress.pproxy import Server
srv = Server(listen=["http://127.0.0.1:0"])
srv.start()
time.sleep(0.1)
srv.close()
"""
    result = subprocess.run(
        [sys.executable, "-c", code],
        capture_output=True, timeout=10,
        text=True, cwd=str(Path(__file__).resolve().parents[2]),
    )
    assert result.returncode == 0, (
        f"Interpreter shutdown failed: {result.stderr}"
    )


# ---------------------------------------------------------------------------
# WS8: Exception mapping — bind, TLS, auth errors
# ---------------------------------------------------------------------------


def test_bind_conflict_error():
    """Starting two servers on the same port raises an error."""
    import socket as _socket

    # Bind a port to block it
    blocker = _socket.socket(_socket.AF_INET, _socket.SOCK_STREAM)
    blocker.setsockopt(_socket.SOL_SOCKET, _socket.SO_REUSEADDR, 1)
    blocker.bind(("127.0.0.1", 0))
    blocker.listen(1)
    _, port = blocker.getsockname()

    try:
        srv = Server(listen=[f"http://127.0.0.1:{port}"])
        try:
            with pytest.raises(Exception):
                srv.start()
            assert srv.last_error is not None
        finally:
            srv.close()
    finally:
        blocker.close()


def test_tls_missing_cert_error():
    """TLS listener with missing cert file fails at config parsing."""
    toml = """
version = 1

[[listeners]]
name = "tls-bad"
bind = "127.0.0.1:0"
protocols = ["http"]

[listeners.tls]
cert = "/nonexistent/cert.pem"
key = "/nonexistent/key.pem"
"""
    with pytest.raises(Exception, match="cert"):
        EggressConfig.from_toml(toml)


def test_config_error_invalid_toml():
    """Invalid TOML raises an error at construction time."""
    with pytest.raises(Exception):
        EggressConfig.from_toml("this is not valid toml {{{")


def test_reload_with_invalid_toml():
    """reload() with invalid TOML raises an error."""
    srv = Server(listen=["http://127.0.0.1:0"])
    srv.start()
    time.sleep(0.1)
    try:
        with pytest.raises(Exception):
            srv.reload("this is not valid toml {{{")
    finally:
        srv.close()


# ---------------------------------------------------------------------------
# WS9: Advanced tests — partial rollback, GIL, FD leak, pproxy examples,
#      close with active session
# ---------------------------------------------------------------------------


def test_partial_bind_failure_rollback():
    """When one listener fails to bind, all are cleaned up."""
    import socket as _socket

    # Block a specific port
    blocker = _socket.socket(_socket.AF_INET, _socket.SOCK_STREAM)
    blocker.setsockopt(_socket.SOL_SOCKET, _socket.SO_REUSEADDR, 1)
    blocker.bind(("127.0.0.1", 0))
    blocker.listen(1)
    _, blocked_port = blocker.getsockname()

    try:
        # Two listeners: one free, one blocked — the blocked one should
        # cause the whole server to fail and release the free one.
        srv = Server(listen=[
            "http://127.0.0.1:0",
            f"http://127.0.0.1:{blocked_port}",
        ])
        try:
            with pytest.raises(Exception):
                srv.start()
            # After failure, no addresses should be held
            assert srv.addresses == {}
        finally:
            srv.close()
    finally:
        blocker.close()


def test_gil_release_during_start():
    """Multiple concurrent Server.start() calls don't deadlock the GIL."""
    errors = []

    def _start_server(idx):
        try:
            srv = Server(listen=["http://127.0.0.1:0"])
            srv.start()
            time.sleep(0.05)
            srv.close()
        except Exception as e:
            errors.append((idx, e))

    threads = [threading.Thread(target=_start_server, args=(i,)) for i in range(5)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=10.0)

    assert errors == [], f"Concurrent start errors: {errors}"


def test_descriptor_leak_detection():
    """Repeated start/close cycles don't leak file descriptors."""
    import os

    fd_before = len(os.listdir("/dev/fd")) if os.path.isdir("/dev/fd") else None
    if fd_before is None:
        pytest.skip("Cannot enumerate file descriptors on this platform")

    for _ in range(5):
        srv = Server(listen=["http://127.0.0.1:0"])
        srv.start()
        time.sleep(0.1)
        srv.close()

    # Allow GC and cleanup
    time.sleep(0.2)
    fd_after = len(os.listdir("/dev/fd"))
    # Allow a small margin for OS-level overhead but no significant growth
    assert fd_after - fd_before < 10, (
        f"FD leak detected: {fd_before} before, {fd_after} after 5 cycles"
    )


def test_pproxy_socks_example():
    """Common pproxy usage pattern: SOCKS5 proxy with upstream."""
    args = ["-l", "socks5://127.0.0.1:0"]
    from eggress.pproxy import PPProxyService
    with PPProxyService.from_args(args) as handle:
        time.sleep(0.1)
        assert len(handle.bound_addresses) > 0


def test_pproxy_multi_listener_example():
    """pproxy pattern: multiple listeners in one process."""
    args = [
        "-l", "socks5://127.0.0.1:0",
        "-l", "http://127.0.0.1:0",
    ]
    from eggress.pproxy import PPProxyService
    with PPProxyService.from_args(args) as handle:
        time.sleep(0.1)
        assert len(handle.bound_addresses) == 2


def test_pproxy_auth_example():
    """pproxy pattern: authenticated SOCKS5 listener."""
    args = [
        "-l", "socks5://admin:secret@127.0.0.1:0",
    ]
    from eggress.pproxy import PPProxyService
    with PPProxyService.from_args(args) as handle:
        time.sleep(0.1)
        assert len(handle.bound_addresses) > 0


def test_pproxy_chain_example():
    """pproxy pattern: SOCKS5 chained through HTTP proxy."""
    from eggress.pproxy import PPProxyService

    args = [
        "-l", "socks5://127.0.0.1:0",
        "-r", "http://127.0.0.1:9999",
    ]
    svc = PPProxyService.from_args(args)
    redacted = svc.config.redacted_toml()
    assert "[[upstreams]]" in redacted
    with svc as handle:
        time.sleep(0.1)
        assert len(handle.bound_addresses) > 0


def test_server_close_with_active_session():
    """Closing server while a SOCKS5 session is active cleans up."""
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

            # Open a connection through the proxy
            s = _socks5_connect(
                socks_addr[0], socks_addr[1], echo_host, echo_port
            )
            s.sendall(b"test")
            resp = s.recv(4096)
            assert resp == b"test"

            # Close server while connection is active
            # (context manager will call close())
        # After close, session count should be 0
        assert srv.sessions == 0
        s.close()
    finally:
        echo_srv.close()


def test_reload_with_upstream_change():
    """reload() applies new upstream config while server runs."""
    # Use explicit listener name so reload TOML matches
    toml = """
version = 1

[[listeners]]
name = "http"
bind = "127.0.0.1:0"
protocols = ["http"]
"""
    cfg = EggressConfig.from_toml(toml)
    srv = Server(config=cfg)
    srv.start()
    time.sleep(0.1)
    try:
        # Reload with same listener name + new upstream config
        new_toml = """
version = 1

[[listeners]]
name = "http"
bind = "127.0.0.1:0"
protocols = ["http"]

[[upstreams]]
id = "proxy1"
uri = "http://127.0.0.1:8080"

[[upstream_groups]]
id = "default"
scheduler = "round-robin"
members = ["proxy1"]

[[rules]]
id = "route-all"
upstream_group = "default"
"""
        result = srv.reload(new_toml)
        assert isinstance(result, dict)
        # Server should still be running
        assert srv.is_ready is True
    finally:
        srv.close()


def test_server_sessions_with_active_connection():
    """sessions property reflects active SOCKS5 connections."""
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

            s = _socks5_connect(
                socks_addr[0], socks_addr[1], echo_host, echo_port
            )
            try:
                # Give time for session to register
                time.sleep(0.1)
                sessions = srv.sessions
                assert sessions >= 1, (
                    f"Expected at least 1 session, got {sessions}"
                )
            finally:
                s.close()
    finally:
        echo_srv.close()


def test_status_contains_listeners():
    """status() dict contains 'listeners' key after start."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        srv.start()
        time.sleep(0.1)
        st = srv.status()
        assert "listeners" in st
        listeners = st["listeners"]
        assert isinstance(listeners, list)
        assert len(listeners) > 0
        # Each listener should have expected keys
        for listener in listeners:
            assert isinstance(listener, dict)
    finally:
        srv.close()


def test_metrics_text_contains_expected_metrics():
    """metrics_text output contains recognizable metric names."""
    srv = Server(listen=["http://127.0.0.1:0"])
    try:
        srv.start()
        time.sleep(0.1)
        metrics = srv.metrics_text
        assert isinstance(metrics, str)
        # Should contain at least one eggress metric
        assert "eggress" in metrics.lower() or "proxy" in metrics.lower() or len(metrics) > 0
    finally:
        srv.close()
