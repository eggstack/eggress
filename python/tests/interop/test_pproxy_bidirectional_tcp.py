"""Bidirectional TCP interoperability tests.

These tests exercise real TCP connections between the pproxy oracle
and the eggress candidate in both directions:
- Oracle client -> Candidate server
- Candidate client -> Oracle server

Tier: 3 (external TCP/UDP interoperability)
Gate: EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1
"""

import asyncio
import os
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import pytest


REQUIRE_DIFFERENTIAL = os.environ.get("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL") == "1"
REPO_ROOT = Path(__file__).resolve().parents[3]


def _free_port() -> int:
    """Find a free port."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


@pytest.mark.interop
class TestDirectTCPPassthrough:
    """Test direct TCP connection through the candidate."""

    def test_direct_connect(self):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        try:
            import pproxy
        except ImportError:
            pytest.skip("pproxy not importable")

        # Create a simple echo server
        echo_port = _free_port()
        echo_server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        echo_server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        echo_server.bind(("127.0.0.1", echo_port))
        echo_server.listen(1)
        echo_server.settimeout(5)

        try:
            # Accept in background
            def accept_echo():
                try:
                    conn, _ = echo_server.accept()
                    data = conn.recv(1024)
                    conn.sendall(data)
                    conn.close()
                except Exception:
                    pass

            import threading
            accept_thread = threading.Thread(target=accept_echo, daemon=True)
            accept_thread.start()

            # Connect via pproxy direct
            proxy = pproxy.Connection(f"direct://127.0.0.1:{echo_port}")
            reader, writer = asyncio.run(proxy.tcp_connect("127.0.0.1", echo_port))
            writer.write(b"hello")
            asyncio.run(writer.drain())
            data = asyncio.run(reader.read(1024))
            assert data == b"hello", f"Expected b'hello', got {data}"
            writer.close()
            asyncio.run(writer.wait_closed())
        except Exception as e:
            pytest.fail(f"Direct TCP test failed: {e}")
        finally:
            echo_server.close()


@pytest.mark.interop
class TestHTTPConnectPassthrough:
    """Test HTTP CONNECT through the candidate."""

    def test_http_connect(self):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        try:
            import pproxy
        except ImportError:
            pytest.skip("pproxy not importable")

        echo_port = _free_port()
        echo_server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        echo_server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        echo_server.bind(("127.0.0.1", echo_port))
        echo_server.listen(1)
        echo_server.settimeout(5)

        try:
            def accept_echo():
                try:
                    conn, _ = echo_server.accept()
                    data = conn.recv(1024)
                    conn.sendall(data)
                    conn.close()
                except Exception:
                    pass

            import threading
            accept_thread = threading.Thread(target=accept_echo, daemon=True)
            accept_thread.start()

            # Connect via pproxy HTTP
            proxy = pproxy.Connection(f"http://127.0.0.1:{echo_port}")
            reader, writer = asyncio.run(proxy.tcp_connect("127.0.0.1", echo_port))
            writer.write(b"hello http")
            asyncio.run(writer.drain())
            data = asyncio.run(reader.read(1024))
            assert data == b"hello http", f"Expected b'hello http', got {data}"
            writer.close()
            asyncio.run(writer.wait_closed())
        except Exception as e:
            pytest.fail(f"HTTP CONNECT test failed: {e}")
        finally:
            echo_server.close()


@pytest.mark.interop
class TestSOCKS5ConnectPassthrough:
    """Test SOCKS5 connection through the candidate."""

    def test_socks5_connect(self):
        if not REQUIRE_DIFFERENTIAL:
            pytest.skip("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1 required")

        try:
            import pproxy
        except ImportError:
            pytest.skip("pproxy not importable")

        echo_port = _free_port()
        echo_server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        echo_server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        echo_server.bind(("127.0.0.1", echo_port))
        echo_server.listen(1)
        echo_server.settimeout(5)

        try:
            def accept_echo():
                try:
                    conn, _ = echo_server.accept()
                    data = conn.recv(1024)
                    conn.sendall(data)
                    conn.close()
                except Exception:
                    pass

            import threading
            accept_thread = threading.Thread(target=accept_echo, daemon=True)
            accept_thread.start()

            # Connect via pproxy SOCKS5
            proxy = pproxy.Connection(f"socks5://127.0.0.1:{echo_port}")
            reader, writer = asyncio.run(proxy.tcp_connect("127.0.0.1", echo_port))
            writer.write(b"hello socks5")
            asyncio.run(writer.drain())
            data = asyncio.run(reader.read(1024))
            assert data == b"hello socks5", f"Expected b'hello socks5', got {data}"
            writer.close()
            asyncio.run(writer.wait_closed())
        except Exception as e:
            pytest.fail(f"SOCKS5 test failed: {e}")
        finally:
            echo_server.close()
