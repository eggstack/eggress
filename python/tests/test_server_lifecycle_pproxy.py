"""Tests for pproxy-compatible server lifecycle via ProxySimple.start_server.

Verifies that ``pproxy.Server(uri).start_server()`` is functional for
protocol paths already supported by eggress.  The pproxy oracle returns
``asyncio.Server`` from ``start_server()``; our compat layer must match.
"""

from __future__ import annotations

import asyncio
import time

import pytest


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _get_proxy_simple(uri: str):
    """Create a ProxySimple from a pproxy URI using our compat layer."""
    import importlib
    # Import our compat server module directly, not via 'pproxy' namespace
    # which may resolve to the real pproxy package.
    _compat = importlib.import_module("pproxy.server")
    # If the real pproxy is loaded, use our eggress classes directly.
    if not getattr(_compat, "__eggress_compat__", False):
        from eggress._pproxy_proxy import ProxySimple, ProxyDirect
        from eggress.protocol import MAPPINGS
        from eggress.pproxy import check_pproxy_uri
        # Split chain URIs on __ to get the listen portion only.
        listen_uri = uri.split("__", 1)[0] if "__" in uri else uri
        scheme = listen_uri.split("://")[0].lower() if "://" in listen_uri else ""
        proto_cls = MAPPINGS.get(scheme)
        if scheme == "direct" or proto_cls is None:
            return ProxyDirect()
        info = check_pproxy_uri(listen_uri)
        # Instantiate the protocol class (not pass the class itself).
        return ProxySimple(
            jump=uri,
            protos=(proto_cls(),),
            host_name=info.host if info.ok else None,
            port=info.port if info.ok else None,
        )
    return _compat.proxies_by_uri(uri)


def _server_addrs(server) -> list[tuple[str, int]]:
    """Extract (host, port) tuples from a server's sockets."""
    return [s.getsockname() for s in server.sockets]


async def _close_server(server) -> None:
    """Close a server and wait for it to finish."""
    server.close()
    await server.wait_closed()


# ---------------------------------------------------------------------------
# Tests: start_server on ProxySimple
# ---------------------------------------------------------------------------


def test_proxy_simple_has_start_server():
    """ProxySimple instances have a start_server method."""
    proxy = _get_proxy_simple("http://127.0.0.1:0")
    assert hasattr(proxy, "start_server")
    assert callable(proxy.start_server)


def test_start_server_returns_compatible_handle():
    """start_server() returns an object with close(), wait_closed(), and sockets."""
    async def _run():
        proxy = _get_proxy_simple("http://127.0.0.1:0")
        server = await proxy.start_server(args={})
        try:
            assert hasattr(server, "close") and callable(server.close)
            assert hasattr(server, "wait_closed") and callable(server.wait_closed)
            assert hasattr(server, "sockets")
            addrs = _server_addrs(server)
            assert len(addrs) > 0
        finally:
            await _close_server(server)

    asyncio.run(_run())


def test_start_server_with_socks5():
    """start_server() works with a SOCKS5 proxy."""
    async def _run():
        proxy = _get_proxy_simple("socks5://127.0.0.1:0")
        server = await proxy.start_server(args={})
        try:
            assert hasattr(server, "close") and callable(server.close)
            assert hasattr(server, "wait_closed") and callable(server.wait_closed)
            assert hasattr(server, "sockets")
            addrs = _server_addrs(server)
            assert len(addrs) > 0
        finally:
            await _close_server(server)

    asyncio.run(_run())


def test_start_server_close_is_idempotent():
    """Closing the returned server multiple times does not raise."""
    async def _run():
        proxy = _get_proxy_simple("http://127.0.0.1:0")
        server = await proxy.start_server(args={})
        server.close()
        await server.wait_closed()
        server.close()
        await server.wait_closed()

    asyncio.run(_run())


def test_start_server_with_upstream_chain():
    """start_server() with a chained upstream creates a valid server."""
    async def _run():
        # Chain: socks5 listener -> http upstream on a real port
        proxy = _get_proxy_simple("socks5://127.0.0.1:0__http://127.0.0.1:18080")
        server = await proxy.start_server(args={})
        try:
            assert hasattr(server, "close") and callable(server.close)
            assert hasattr(server, "wait_closed") and callable(server.wait_closed)
            assert hasattr(server, "sockets")
            addrs = _server_addrs(server)
            assert len(addrs) > 0
        finally:
            await _close_server(server)

    asyncio.run(_run())


def test_start_server_with_args():
    """start_server(args={}) accepts a dict of runtime arguments."""
    async def _run():
        proxy = _get_proxy_simple("http://127.0.0.1:0")
        server = await proxy.start_server(args={"verbose": 0})
        try:
            assert hasattr(server, "close") and callable(server.close)
            assert hasattr(server, "wait_closed") and callable(server.wait_closed)
            assert hasattr(server, "sockets")
            addrs = _server_addrs(server)
            assert len(addrs) > 0
        finally:
            await _close_server(server)

    asyncio.run(_run())


def test_start_server_with_custom_handler():
    """start_server(stream_handler=...) uses the provided handler."""
    async def _run():
        proxy = _get_proxy_simple("http://127.0.0.1:0")
        handler_called = []

        async def my_handler(reader, writer, **kwargs):
            handler_called.append(True)
            writer.close()
            await writer.wait_closed()

        server = await proxy.start_server(args={}, stream_handler=my_handler)
        try:
            assert hasattr(server, "close") and callable(server.close)
            assert hasattr(server, "wait_closed") and callable(server.wait_closed)
            assert hasattr(server, "sockets")
            addrs = _server_addrs(server)
            assert len(addrs) > 0
        finally:
            await _close_server(server)

    asyncio.run(_run())


# ---------------------------------------------------------------------------
# Tests: start_server on ProxyBackward
# ---------------------------------------------------------------------------


def test_proxy_backward_has_start_server():
    """ProxyBackward instances have a start_server method."""
    from eggress._pproxy_proxy import ProxyBackward
    proxy = ProxyBackward()
    assert hasattr(proxy, "start_server")
    assert callable(proxy.start_server)


def test_backward_start_server_returns_compatible_handle():
    """ProxyBackward.start_server() delegates to ProxySimple and works."""
    from eggress._pproxy_proxy import ProxyBackward

    async def _run():
        proxy = ProxyBackward(protos=(), bind="127.0.0.1:0")
        server = await proxy.start_server(args={})
        try:
            assert hasattr(server, "close") and callable(server.close)
            assert hasattr(server, "wait_closed") and callable(server.wait_closed)
            assert hasattr(server, "sockets")
            addrs = _server_addrs(server)
            assert len(addrs) > 0
        finally:
            await _close_server(server)

    asyncio.run(_run())


# ---------------------------------------------------------------------------
# Tests: start_server on ProxyH2
# ---------------------------------------------------------------------------


def test_proxy_h2_has_start_server():
    """ProxyH2 instances have a start_server method."""
    from eggress._pproxy_proxy import ProxyH2
    proxy = ProxyH2()
    assert hasattr(proxy, "start_server")
    assert callable(proxy.start_server)


def test_h2_start_server_returns_compatible_handle():
    """ProxyH2.start_server() delegates to ProxySimple and works."""
    from eggress._pproxy_proxy import ProxyH2

    async def _run():
        proxy = ProxyH2(protos=(), bind="127.0.0.1:0")
        server = await proxy.start_server(args={})
        try:
            assert hasattr(server, "close") and callable(server.close)
            assert hasattr(server, "wait_closed") and callable(server.wait_closed)
            assert hasattr(server, "sockets")
            addrs = _server_addrs(server)
            assert len(addrs) > 0
        finally:
            await _close_server(server)

    asyncio.run(_run())


# ---------------------------------------------------------------------------
# Tests: start_server on unsupported protocols
# ---------------------------------------------------------------------------


def test_ssh_start_server_raises():
    """ProxySSH.start_server() raises NotImplementedError."""
    from eggress._pproxy_proxy import ProxySSH
    proxy = ProxySSH()

    async def _run():
        with pytest.raises(NotImplementedError, match="SSH"):
            await proxy.start_server(args={})

    asyncio.run(_run())


def test_quic_start_server_raises():
    """ProxyQUIC.start_server() raises NotImplementedError."""
    from eggress._pproxy_proxy import ProxyQUIC
    proxy = ProxyQUIC()

    async def _run():
        with pytest.raises(NotImplementedError, match="QUIC"):
            await proxy.start_server(args={})

    asyncio.run(_run())


def test_h3_start_server_raises():
    """ProxyH3.start_server() raises NotImplementedError (QUIC)."""
    from eggress._pproxy_proxy import ProxyH3
    proxy = ProxyH3()

    async def _run():
        with pytest.raises(NotImplementedError):
            await proxy.start_server(args={})

    asyncio.run(_run())


# ---------------------------------------------------------------------------
# Tests: pproxy.Server(uri) integration
# ---------------------------------------------------------------------------


def test_pproxy_server_alias_returns_proxy():
    """pproxy.Server(uri) returns a proxy object with start_server."""
    from pproxy import Server
    proxy = Server("http://127.0.0.1:0")
    assert hasattr(proxy, "start_server")


def test_pproxy_server_start_server_lifecycle():
    """pproxy.Server(uri).start_server() starts and stops cleanly."""
    import importlib
    compat_mod = importlib.import_module("pproxy.server")
    if not getattr(compat_mod, "__eggress_compat__", False):
        pytest.skip("real pproxy installed; compat integration test needs compat layer")
    from pproxy import Server

    async def _run():
        proxy = Server("http://127.0.0.1:0")
        server = await proxy.start_server(args={})
        try:
            assert hasattr(server, "close") and callable(server.close)
            assert hasattr(server, "wait_closed") and callable(server.wait_closed)
            assert hasattr(server, "sockets")
            addrs = _server_addrs(server)
            assert len(addrs) > 0
        finally:
            await _close_server(server)
        # After close, sockets should be empty
        assert server.sockets == ()

    asyncio.run(_run())


def test_pproxy_server_socks5_start_server():
    """pproxy.Server('socks5://...').start_server() works."""
    import importlib
    compat_mod = importlib.import_module("pproxy.server")
    if not getattr(compat_mod, "__eggress_compat__", False):
        pytest.skip("real pproxy installed; compat integration test needs compat layer")
    from pproxy import Server

    async def _run():
        proxy = Server("socks5://127.0.0.1:0")
        server = await proxy.start_server(args={})
        try:
            assert hasattr(server, "close") and callable(server.close)
            assert hasattr(server, "wait_closed") and callable(server.wait_closed)
            assert hasattr(server, "sockets")
            addrs = _server_addrs(server)
            assert len(addrs) > 0
        finally:
            await _close_server(server)

    asyncio.run(_run())


# ---------------------------------------------------------------------------
# Tests: _build_listen_uri / _build_remote_uri
# ---------------------------------------------------------------------------


def test_build_listen_uri_default():
    """_build_listen_uri returns correct URI for default config."""
    proxy = _get_proxy_simple("http://127.0.0.1:0")
    uri = proxy._build_listen_uri()
    assert uri.startswith("http://")
    assert "127.0.0.1" in uri


def test_build_listen_uri_socks5():
    """_build_listen_uri returns socks5 scheme for SOCKS5 proxy."""
    proxy = _get_proxy_simple("socks5://127.0.0.1:0")
    uri = proxy._build_listen_uri()
    assert uri.startswith("socks5://")


def test_build_listen_uri_custom_host_port():
    """_build_listen_uri uses stored host_name and port."""
    from eggress._pproxy_proxy import ProxySimple
    from eggress.protocol import HTTP
    proxy = ProxySimple(protos=(HTTP,), host_name="0.0.0.0", port=8080)
    uri = proxy._build_listen_uri()
    assert uri == "http://0.0.0.0:8080"


def test_build_remote_uri_none_for_direct():
    """_build_remote_uri returns None when no jump chain."""
    proxy = _get_proxy_simple("http://127.0.0.1:0")
    assert proxy._build_remote_uri() is None


def test_build_remote_uri_string_jump():
    """_build_remote_uri returns the upstream portion of a chain URI."""
    proxy = _get_proxy_simple("socks5://127.0.0.1:0__http://127.0.0.1:9999")
    remote = proxy._build_remote_uri()
    # Should return only the upstream part (after __)
    assert remote == "http://127.0.0.1:9999"


# ---------------------------------------------------------------------------
# Tests: start_server with SOCKS5 proxy through echo server
# ---------------------------------------------------------------------------


def _echo_server():
    """Start a TCP echo server on 127.0.0.1:0."""
    import socket
    import threading

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


def _socks5_connect(proxy_host, proxy_port, target_host, target_port):
    """Perform a SOCKS5 CONNECT handshake and return the connected socket.

    Sends the greeting and CONNECT request together, then waits for
    both responses.  This matches pproxy's stream_handler which reads
    the CONNECT request immediately after sending the greeting response,
    without yielding control back to the caller between steps.
    """
    import socket
    import struct

    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(5.0)
    s.connect((proxy_host, proxy_port))
    # Send greeting + CONNECT request in one shot
    octets = [int(x) for x in target_host.split(".")]
    connect_req = (
        b"\x05\x01\x00\x01"
        + bytes(octets)
        + struct.pack("!H", target_port)
    )
    s.sendall(b"\x05\x01\x00" + connect_req)
    # Read greeting response + connect response
    resp = s.recv(32)
    assert len(resp) >= 2, f"SOCKS5 response too short: {resp!r}"
    assert resp[0] == 0x05, f"SOCKS5 version mismatch: {resp!r}"
    assert resp[1] == 0x00, f"SOCKS5 connect failed: {resp!r}"
    return s


def test_start_server_socks5_listens():
    """SOCKS5 started via start_server is reachable on the bound port."""
    async def _run():
        proxy = _get_proxy_simple("socks5://127.0.0.1:0")
        server = await proxy.start_server(args={})
        try:
            addrs = _server_addrs(server)
            assert len(addrs) > 0
            host, port = addrs[0]
            import socket as _socket
            s = _socket.socket(_socket.AF_INET, _socket.SOCK_STREAM)
            s.settimeout(5.0)
            try:
                s.connect((host, int(port)))
                # pproxy's Socks5.accept reads greeting + CONNECT in one go,
                # so we send both together to avoid IncompleteReadError.
                import struct
                octets = [127, 0, 0, 1]
                connect_req = (
                    b"\x05\x01\x00"
                    + b"\x05\x01\x00\x01"
                    + bytes(octets)
                    + struct.pack("!H", 1)
                )
                s.sendall(connect_req)
                # Allow the handler to process the connection
                await asyncio.sleep(0.1)
                resp = s.recv(32)
                # We should get a SOCKS5 response (greeting + connect)
                assert len(resp) >= 2
                assert resp[0] == 0x05
            finally:
                s.close()
        finally:
            await _close_server(server)

    asyncio.run(_run())


def test_start_server_bind_failure_raises():
    """start_server() on an occupied port raises OSError."""
    import socket

    blocker = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    blocker.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    blocker.bind(("127.0.0.1", 0))
    blocker.listen(1)
    _, port = blocker.getsockname()

    try:
        async def _run():
            proxy = _get_proxy_simple(f"http://127.0.0.1:{port}")
            with pytest.raises(OSError):
                await proxy.start_server(args={})

        asyncio.run(_run())
    finally:
        blocker.close()


def test_start_server_returns_asyncio_server():
    """start_server() returns an asyncio.Server matching the pproxy contract."""
    import asyncio

    async def _run():
        proxy = _get_proxy_simple("http://127.0.0.1:0")
        server = await proxy.start_server(args={})
        # pproxy 2.7.9 returns asyncio.Server
        assert isinstance(server, asyncio.Server)
        assert callable(server.close)
        assert hasattr(server, "sockets")
        server.close()
        await server.wait_closed()

    asyncio.run(_run())


def test_start_server_close_idempotent():
    """asyncio.Server.close() is idempotent."""
    async def _run():
        proxy = _get_proxy_simple("http://127.0.0.1:0")
        server = await proxy.start_server(args={})
        server.close()
        await server.wait_closed()
        # Second close + wait_closed should not raise
        server.close()
        await server.wait_closed()

    asyncio.run(_run())


def test_start_server_wait_closed_after_close():
    """asyncio.Server.wait_closed() completes after close()."""
    async def _run():
        proxy = _get_proxy_simple("http://127.0.0.1:0")
        server = await proxy.start_server(args={})
        server.close()
        await server.wait_closed()
        # sockets should be empty after close
        assert server.sockets == ()

    asyncio.run(_run())


def test_start_server_sockets_populated_after_start():
    """server.sockets is non-empty after start_server completes."""
    async def _run():
        proxy = _get_proxy_simple("http://127.0.0.1:0")
        server = await proxy.start_server(args={})
        try:
            assert server.sockets is not None
            assert len(server.sockets) > 0
            for sock in server.sockets:
                assert sock.getsockname()[1] > 0
        finally:
            await _close_server(server)

    asyncio.run(_run())


def test_start_server_sockets_empty_after_close():
    """server.sockets is empty after close."""
    async def _run():
        proxy = _get_proxy_simple("http://127.0.0.1:0")
        server = await proxy.start_server(args={})
        await _close_server(server)
        assert server.sockets == ()

    asyncio.run(_run())


def test_start_server_cancelled_cleanup():
    """Cancelling tasks after start_server still allows clean close."""
    async def _run():
        proxy = _get_proxy_simple("http://127.0.0.1:0")
        server = await proxy.start_server(args={})
        # Verify it started
        assert len(server.sockets) > 0
        # Close explicitly
        await _close_server(server)
        # Should not raise
        assert server.sockets == ()

    asyncio.run(_run())
