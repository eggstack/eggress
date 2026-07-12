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
    s.settimeout(3.0)
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
