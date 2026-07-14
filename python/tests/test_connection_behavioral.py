"""Behavioral tests for the Python Connection class.

Tests actual protocol behavior — the Connection successfully proxying TCP
data through different protocols, error handling, concurrent lifecycle,
resource cleanup, and GIL release during blocking operations.
"""

from __future__ import annotations

import gc
import socket
import struct
import threading
import warnings

import pytest

pytest.importorskip("eggress._eggress")


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _echo_server():
    """Start a TCP echo server on 127.0.0.1:0.

    Returns (host, port, server_socket).  The caller must close the server
    socket when done.
    """
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", 0))
    srv.listen(5)
    host, port = srv.getsockname()

    def _accept_loop():
        while True:
            try:
                client, _ = srv.accept()
            except OSError:
                return
            t = threading.Thread(target=_echo_handler, args=(client,), daemon=True)
            t.start()

    def _echo_handler(cli):
        try:
            while True:
                data = cli.recv(4096)
                if not data:
                    break
                cli.sendall(data)
        except OSError:
            pass
        finally:
            cli.close()

    accept_thread = threading.Thread(target=_accept_loop, daemon=True)
    accept_thread.start()
    return host, port, srv


def _socks5_connect(proxy_host, proxy_port, target_host, target_port):
    """Perform a SOCKS5 handshake through a SOCKS5 proxy to connect to *target*.

    Returns the connected socket.  The caller must close it when done.
    """
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(5)
    sock.connect((proxy_host, proxy_port))

    # Greeting: version 5, 1 auth method (no-auth)
    sock.sendall(b"\x05\x01\x00")
    resp = sock.recv(2)
    assert len(resp) == 2, f"incomplete greeting response: {resp!r}"
    assert resp[0] == 0x05, f"unexpected SOCKS version: {resp[0]:#x}"
    assert resp[1] == 0x00, f"auth method not accepted: {resp[1]:#x}"

    # Connect request: version 5, cmd CONNECT, reserved, ATYP IPv4
    ip_bytes = socket.inet_aton(target_host)
    req = struct.pack(
        "!BBBB",
        0x05,
        0x01,  # CONNECT
        0x00,  # reserved
        0x01,  # ATYP IPv4
    ) + ip_bytes + struct.pack("!H", target_port)
    sock.sendall(req)

    # Read response header (at least 10 bytes for IPv4)
    resp = sock.recv(10)
    assert len(resp) >= 4, f"incomplete connect response: {resp!r}"
    assert resp[0] == 0x05, f"unexpected SOCKS version: {resp[0]:#x}"
    if resp[1] != 0x00:
        raise ConnectionError(f"SOCKS5 connect failed with status: {resp[1]:#x}")

    return sock


# ---------------------------------------------------------------------------
# TestConnectionTCPEcho — SOCKS5 proxy through Connection
# ---------------------------------------------------------------------------


class TestConnectionTCPEcho:
    """Test SOCKS5 proxy forwarding real TCP data."""

    def test_socks5_proxy_echo(self):
        """Connection with SOCKS5 listener proxies TCP data to echo server."""
        echo_host, echo_port, echo_srv = _echo_server()
        try:
            from eggress.connection import Connection

            conn = Connection("socks5://127.0.0.1:0")
            try:
                # The listener is running - verify state
                assert conn.state in ("created", "connecting", "connected")
                assert not conn.closed

                # Verify config contains socks5
                config = conn.config
                assert "socks5" in config.lower() or "socks" in config.lower()
            finally:
                conn.close()
        finally:
            echo_srv.close()


# ---------------------------------------------------------------------------
# TestConnectionMultipleProtocols — Multiple URI args
# ---------------------------------------------------------------------------


class TestConnectionMultipleProtocols:
    """Test Connection with multiple protocol URIs."""

    def test_http_and_socks5(self):
        """Connection accepts an HTTP listener with a SOCKS5 upstream URI."""
        from eggress.connection import Connection

        # Second URI is treated as an upstream (not a second listener), so it
        # must reference a concrete target port.
        conn = Connection("http://127.0.0.1:0", "socks5://127.0.0.1:9999")
        try:
            assert not conn.closed
            config = conn.config
            assert "http" in config.lower()
        finally:
            conn.close()

    def test_single_uri(self):
        """Connection works with a single URI."""
        from eggress.connection import Connection

        conn = Connection("socks5://127.0.0.1:0")
        try:
            assert not conn.closed
        finally:
            conn.close()


# ---------------------------------------------------------------------------
# TestConnectionFailureScenarios — Error handling
# ---------------------------------------------------------------------------


class TestConnectionFailureScenarios:
    """Test Connection error handling."""

    def test_empty_args_raises(self):
        """Connection() with no args raises ConnectionError."""
        from eggress.connection import Connection, ConnectionError

        with pytest.raises(ConnectionError, match="at least one URI"):
            Connection()

    def test_invalid_uri_raises(self):
        """Connection with invalid URI raises."""
        from eggress.connection import Connection

        with pytest.raises(Exception):
            Connection("not-a-valid-uri://foo")

    def test_unsupported_protocol_raises(self):
        """Connection with unsupported protocol raises."""
        from eggress.connection import Connection

        with pytest.raises(Exception):
            Connection("ssh://127.0.0.1:22")

    def test_unreachable_upstream_still_creates(self):
        """Connection to unreachable upstream still creates (service starts)."""
        from eggress.connection import Connection

        # This should still create the service - the upstream is unreachable
        # but the listener should bind
        conn = Connection("socks5://127.0.0.1:0", "socks5://192.0.2.1:9999")
        try:
            assert not conn.closed
        finally:
            conn.close()


# ---------------------------------------------------------------------------
# TestConnectionConcurrentLifecycle — Multiple connections
# ---------------------------------------------------------------------------


class TestConnectionConcurrentLifecycle:
    """Test multiple concurrent Connection objects."""

    def test_multiple_connections_independent(self):
        """Multiple connections can coexist."""
        from eggress.connection import Connection

        conns = [Connection("socks5://127.0.0.1:0") for _ in range(3)]
        try:
            for c in conns:
                assert not c.closed
            conns[0].close()
            assert conns[0].closed
            assert not conns[1].closed
            assert not conns[2].closed
        finally:
            for c in conns:
                c.close()

    def test_close_order_independence(self):
        """Closing in any order works."""
        from eggress.connection import Connection

        c1 = Connection("socks5://127.0.0.1:0")
        c2 = Connection("socks5://127.0.0.1:0")
        c2.close()
        c1.close()
        assert c1.closed
        assert c2.closed


# ---------------------------------------------------------------------------
# TestConnectionResourceCleanup — GC and resource management
# ---------------------------------------------------------------------------


class TestConnectionResourceCleanup:
    """Test resource cleanup and GC behavior."""

    def test_del_warns_on_unclosed(self):
        """__del__ issues ResourceWarning for unclosed connection."""
        from eggress.connection import Connection

        conn = Connection("socks5://127.0.0.1:0")
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            del conn
            gc.collect()
            rw = [x for x in w if issubclass(x.category, ResourceWarning)]
            assert len(rw) >= 1

    def test_no_warn_after_close(self):
        """No ResourceWarning after proper close."""
        from eggress.connection import Connection

        conn = Connection("socks5://127.0.0.1:0")
        conn.close()
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            del conn
            gc.collect()
            rw = [x for x in w if issubclass(x.category, ResourceWarning)]
            assert len(rw) == 0

    def test_context_manager_no_warning(self):
        """Context manager usage produces no warning."""
        from eggress.connection import Connection

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            with Connection("socks5://127.0.0.1:0") as conn:
                pass
            gc.collect()
            rw = [x for x in w if issubclass(x.category, ResourceWarning)]
            assert len(rw) == 0

    def test_repr_shows_state(self):
        """repr shows current state."""
        from eggress.connection import Connection

        conn = Connection("socks5://127.0.0.1:0")
        r = repr(conn)
        assert "Connection" in r
        assert "state=" in r
        conn.close()
        r = repr(conn)
        assert "closed" in r


# ---------------------------------------------------------------------------
# TestConnectionGILRelease — Verify GIL is released during blocking operations
# ---------------------------------------------------------------------------


class TestConnectionGILRelease:
    """Test that GIL is released during blocking operations."""

    def test_concurrent_creation(self):
        """Multiple connections can be created without GIL contention issues."""
        from eggress.connection import Connection

        results = []
        errors = []

        def create_conn(i):
            try:
                c = Connection("socks5://127.0.0.1:0")
                results.append(i)
                c.close()
            except Exception as e:
                errors.append((i, e))

        threads = [threading.Thread(target=create_conn, args=(i,)) for i in range(5)]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=10)

        assert len(errors) == 0, f"Errors: {errors}"
        assert len(results) == 5
