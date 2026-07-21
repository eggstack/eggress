"""AC5 corrective: Proxy object behavioral tests.

Tests for ProxyDirect.bind, chain topology (nested .jump), and
tcp_connect/open_connection on ProxyDirect.
"""

from __future__ import annotations

import asyncio

import pytest


# ---------------------------------------------------------------------------
# ProxyDirect.bind
# ---------------------------------------------------------------------------


class TestProxyDirectBind:
    def test_bind_returns_direct_string(self):
        from eggress._pproxy_proxy import ProxyDirect

        obj = ProxyDirect()
        assert obj.bind == "DIRECT"

    def test_singleton_bind(self):
        from eggress._pproxy_proxy import DIRECT

        assert DIRECT.bind == "DIRECT"


# ---------------------------------------------------------------------------
# Chain topology (nested .jump)
# ---------------------------------------------------------------------------


class TestChainTopology:
    def test_two_hop_chain_has_jump_attribute(self):
        from pproxy.server import proxies_by_uri

        result = proxies_by_uri("socks5://proxy:1080/__direct://")
        assert hasattr(result, "jump")

    def test_two_hop_chain_jump_is_not_list(self):
        from pproxy.server import proxies_by_uri

        result = proxies_by_uri("socks5://proxy:1080/__direct://")
        assert not isinstance(result.jump, list)

    def test_three_hop_chain_nested_jumps(self):
        from pproxy.server import proxies_by_uri

        result = proxies_by_uri(
            "socks5://proxy1:1080/__socks5://proxy2:1080/__direct://"
        )
        assert hasattr(result, "jump")
        assert not isinstance(result.jump, list)
        assert hasattr(result.jump, "jump")
        assert not isinstance(result.jump.jump, list)

    def test_direct_uri_returns_proxy_direct(self):
        from pproxy.server import proxies_by_uri

        result = proxies_by_uri("direct://")
        assert getattr(result, "direct", None) is True
        assert getattr(result, "bind", None) == "DIRECT"

    def test_single_uri_returns_proxy_simple(self):
        from pproxy.server import proxies_by_uri

        result = proxies_by_uri("socks5://proxy:1080")
        assert getattr(result, "direct", None) is False
        assert hasattr(result, "jump")

    def test_connection_alias_returns_jump_object(self):
        from pproxy import Connection

        result = Connection("http://proxy:8080/__direct://")
        assert hasattr(result, "jump")
        assert not isinstance(result.jump, list)


# ---------------------------------------------------------------------------
# ProxyDirect.tcp_connect and open_connection
# ---------------------------------------------------------------------------


class TestProxyDirectTcpConnect:
    def test_tcp_connect_returns_reader_writer(self):
        import socket
        import threading
        from eggress._pproxy_proxy import ProxyDirect

        # Start a simple echo server
        server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server.bind(("127.0.0.1", 0))
        server.listen(1)
        addr = server.getsockname()

        def _echo():
            try:
                conn, _ = server.accept()
                data = conn.recv(4096)
                if data:
                    conn.sendall(data)
                conn.close()
            except OSError:
                pass
            finally:
                server.close()

        t = threading.Thread(target=_echo, daemon=True)
        t.start()

        async def _test():
            proxy = ProxyDirect()
            reader, writer = await proxy.tcp_connect(addr[0], addr[1])
            writer.write(b"hello")
            await writer.drain()
            data = await reader.read(1024)
            writer.close()
            await writer.wait_closed()
            return data

        try:
            result = asyncio.run(_test())
            assert result == b"hello"
        finally:
            t.join(timeout=5)

    def test_open_connection_delegates_to_tcp_connect(self):
        import socket
        import threading
        from eggress._pproxy_proxy import ProxyDirect

        server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server.bind(("127.0.0.1", 0))
        server.listen(1)
        addr = server.getsockname()

        def _echo():
            try:
                conn, _ = server.accept()
                data = conn.recv(4096)
                if data:
                    conn.sendall(data)
                conn.close()
            except OSError:
                pass
            finally:
                server.close()

        t = threading.Thread(target=_echo, daemon=True)
        t.start()

        async def _test():
            proxy = ProxyDirect()
            reader, writer = await proxy.open_connection(
                addr[0], addr[1], local_addr=None, lbind=None
            )
            writer.write(b"open")
            await writer.drain()
            data = await reader.read(1024)
            writer.close()
            await writer.wait_closed()
            return data

        try:
            result = asyncio.run(_test())
            assert result == b"open"
        finally:
            t.join(timeout=5)
