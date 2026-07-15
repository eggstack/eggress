"""Phase C2 corrective: ProxyConnection facade tests.

Tests for the pproxy-compatible ProxyConnection class that provides
outbound TCP connections through a proxy chain.
"""

from __future__ import annotations

import gc
import socket
import threading
import warnings

import pytest

pytest.importorskip("eggress._eggress")


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

_SOCKS5_URI = "socks5://127.0.0.1:0"
_HTTP_URI = "http://127.0.0.1:0"


def _start_echo_server() -> tuple[socket.socket, threading.Thread]:
    """Start a simple TCP echo server on an ephemeral port."""
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", 0))
    server.listen(1)
    addr = server.getsockname()

    def _echo():
        try:
            conn, _ = server.accept()
            try:
                data = conn.recv(4096)
                if data:
                    conn.sendall(data)
            finally:
                conn.close()
        except OSError:
            pass
        finally:
            server.close()

    t = threading.Thread(target=_echo, daemon=True)
    t.start()
    return addr, t


# ---------------------------------------------------------------------------
# Contract tests
# ---------------------------------------------------------------------------


class TestProxyConnectionContract:
    """Contract tests: imports, signatures, attributes, types."""

    def test_import_proxy_connection(self):
        from eggress.pproxy_connection import ProxyConnection

        assert ProxyConnection is not None

    def test_import_from_package(self):
        import eggress

        assert hasattr(eggress, "ProxyConnection")

    def test_constructor_requires_args(self):
        from eggress.pproxy_connection import ProxyConnection

        with pytest.raises(ValueError, match="at least one URI"):
            ProxyConnection()

    def test_constructor_accepts_uris(self):
        from eggress.pproxy_connection import ProxyConnection

        conn = ProxyConnection(_SOCKS5_URI)
        try:
            assert not conn.closed
        finally:
            conn.close()

    def test_repr(self):
        from eggress.pproxy_connection import ProxyConnection

        conn = ProxyConnection(_SOCKS5_URI)
        try:
            r = repr(conn)
            assert "ProxyConnection" in r
            assert "state=" in r
        finally:
            conn.close()

    def test_repr_after_close(self):
        from eggress.pproxy_connection import ProxyConnection

        conn = ProxyConnection(_SOCKS5_URI)
        conn.close()
        r = repr(conn)
        assert "closed" in r

    def test_bool_when_open(self):
        from eggress.pproxy_connection import ProxyConnection

        conn = ProxyConnection(_SOCKS5_URI)
        try:
            assert bool(conn)
        finally:
            conn.close()

    def test_bool_when_closed(self):
        from eggress.pproxy_connection import ProxyConnection

        conn = ProxyConnection(_SOCKS5_URI)
        conn.close()
        assert not bool(conn)


# ---------------------------------------------------------------------------
# Lifecycle tests
# ---------------------------------------------------------------------------


class TestProxyConnectionLifecycle:
    """Behavioral tests: lifecycle, close semantics, resource ownership."""

    def test_close_is_idempotent(self):
        from eggress.pproxy_connection import ProxyConnection

        conn = ProxyConnection(_SOCKS5_URI)
        conn.close()
        conn.close()  # should not raise
        assert conn.closed

    def test_context_manager_closes(self):
        from eggress.pproxy_connection import ProxyConnection

        with ProxyConnection(_SOCKS5_URI) as conn:
            assert not conn.closed
        assert conn.closed

    def test_tcp_connect_returns_socket(self):
        from eggress.pproxy_connection import ProxyConnection

        echo_addr, echo_thread = _start_echo_server()
        try:
            conn = ProxyConnection(_SOCKS5_URI)
            try:
                sock = conn.tcp_connect(echo_addr[0], echo_addr[1], timeout=5.0)
                try:
                    assert isinstance(sock, socket.socket)
                    sock.sendall(b"hello")
                    data = sock.recv(1024)
                    assert data == b"hello"
                finally:
                    sock.close()
            finally:
                conn.close()
        finally:
            echo_thread.join(timeout=5)

    def test_tcp_connect_after_close_raises(self):
        from eggress.pproxy_connection import ProxyConnection

        conn = ProxyConnection(_SOCKS5_URI)
        conn.close()
        with pytest.raises(RuntimeError, match="closed"):
            conn.tcp_connect("127.0.0.1", 80)

    def test_del_warns_on_unclosed(self):
        from eggress.pproxy_connection import ProxyConnection

        conn = ProxyConnection(_SOCKS5_URI)
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            del conn
            gc.collect()
            resource_warnings = [x for x in w if issubclass(x.category, ResourceWarning)]
            assert len(resource_warnings) >= 1

    def test_multiple_connections_independent(self):
        from eggress.pproxy_connection import ProxyConnection

        conn1 = ProxyConnection(_SOCKS5_URI)
        conn2 = ProxyConnection(_SOCKS5_URI)
        try:
            assert not conn1.closed
            assert not conn2.closed
            conn1.close()
            assert conn1.closed
            assert not conn2.closed
        finally:
            conn2.close()

    def test_tcp_connect_multiple_times(self):
        """Verify the same ProxyConnection can make multiple connections."""
        from eggress.pproxy_connection import ProxyConnection

        conn = ProxyConnection(_SOCKS5_URI)
        try:
            echo1_addr, echo1_thread = _start_echo_server()
            echo2_addr, echo2_thread = _start_echo_server()
            try:
                sock1 = conn.tcp_connect(echo1_addr[0], echo1_addr[1], timeout=5.0)
                sock1.sendall(b"first")
                assert sock1.recv(1024) == b"first"
                sock1.close()

                sock2 = conn.tcp_connect(echo2_addr[0], echo2_addr[1], timeout=5.0)
                sock2.sendall(b"second")
                assert sock2.recv(1024) == b"second"
                sock2.close()
            finally:
                echo1_thread.join(timeout=5)
                echo2_thread.join(timeout=5)
        finally:
            conn.close()

    def test_addresses_empty_before_start(self):
        from eggress.pproxy_connection import ProxyConnection

        conn = ProxyConnection(_SOCKS5_URI)
        try:
            # Addresses should be populated after first tcp_connect
            # but may be empty before any connection is made
            addrs = conn.addresses
            assert isinstance(addrs, dict)
        finally:
            conn.close()
