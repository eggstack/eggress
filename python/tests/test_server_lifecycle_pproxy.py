"""Tests for pproxy-compatible server lifecycle via ProxySimple.start_server.

Verifies that ``pproxy.Server(uri).start_server()`` is functional for
protocol paths already supported by eggress.
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
        scheme = uri.split("://")[0].lower() if "://" in uri else ""
        proto_cls = MAPPINGS.get(scheme)
        if scheme == "direct" or proto_cls is None:
            return ProxyDirect()
        info = check_pproxy_uri(uri)
        return ProxySimple(
            jump=uri,
            protos=(proto_cls,),
            host_name=info.host if info.ok else None,
            port=info.port if info.ok else None,
        )
    return _compat.proxies_by_uri(uri)


# ---------------------------------------------------------------------------
# Tests: start_server on ProxySimple
# ---------------------------------------------------------------------------


def test_proxy_simple_has_start_server():
    """ProxySimple instances have a start_server method."""
    proxy = _get_proxy_simple("http://127.0.0.1:0")
    assert hasattr(proxy, "start_server")
    assert callable(proxy.start_server)


def test_start_server_returns_running_server():
    """start_server() returns a running eggress Server with addresses."""
    from eggress.pproxy import Server as EggressServer

    async def _run():
        proxy = _get_proxy_simple("http://127.0.0.1:0")
        server = await proxy.start_server()
        try:
            assert isinstance(server, EggressServer)
            assert len(server.addresses) > 0
        finally:
            await server.aclose()

    asyncio.run(_run())


def test_start_server_with_socks5():
    """start_server() works with a SOCKS5 proxy."""
    from eggress.pproxy import Server as EggressServer

    async def _run():
        proxy = _get_proxy_simple("socks5://127.0.0.1:0")
        server = await proxy.start_server()
        try:
            assert isinstance(server, EggressServer)
            assert len(server.addresses) > 0
        finally:
            await server.aclose()

    asyncio.run(_run())


def test_start_server_close_is_idempotent():
    """Closing the returned server multiple times does not raise."""
    async def _run():
        proxy = _get_proxy_simple("http://127.0.0.1:0")
        server = await proxy.start_server()
        await server.aclose()
        await server.aclose()  # idempotent

    asyncio.run(_run())


def test_start_server_with_upstream_chain():
    """start_server() with a chained upstream creates a valid server."""
    from eggress.pproxy import Server as EggressServer

    async def _run():
        # Chain: socks5 listener -> http upstream on a real port
        proxy = _get_proxy_simple("socks5://127.0.0.1:0__http://127.0.0.1:18080")
        server = await proxy.start_server()
        try:
            assert isinstance(server, EggressServer)
            assert len(server.addresses) > 0
            # Verify upstream config is in the translated TOML
            redacted = server.config.redacted_toml()
            assert "[[upstreams]]" in redacted
        finally:
            await server.aclose()

    asyncio.run(_run())


def test_start_server_with_args_ignored():
    """start_server(args=...) does not crash (args are ignored)."""
    async def _run():
        proxy = _get_proxy_simple("http://127.0.0.1:0")
        server = await proxy.start_server(args=["--some-flag"])
        try:
            assert len(server.addresses) > 0
        finally:
            await server.aclose()

    asyncio.run(_run())


def test_start_server_with_stream_handler_ignored():
    """start_server(stream_handler=...) does not crash (handler is ignored)."""
    async def _run():
        proxy = _get_proxy_simple("http://127.0.0.1:0")
        handler_called = []

        def my_handler(*args, **kwargs):
            handler_called.append(True)

        server = await proxy.start_server(stream_handler=my_handler)
        try:
            assert len(server.addresses) > 0
        finally:
            await server.aclose()

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


def test_backward_start_server_returns_running_server():
    """ProxyBackward.start_server() delegates to ProxySimple and works."""
    from eggress.pproxy import Server as EggressServer
    from eggress._pproxy_proxy import ProxyBackward

    async def _run():
        proxy = ProxyBackward(protos=(), bind="127.0.0.1:0")
        server = await proxy.start_server()
        try:
            assert isinstance(server, EggressServer)
            assert len(server.addresses) > 0
        finally:
            await server.aclose()

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


def test_h2_start_server_returns_running_server():
    """ProxyH2.start_server() delegates to ProxySimple and works."""
    from eggress.pproxy import Server as EggressServer
    from eggress._pproxy_proxy import ProxyH2

    async def _run():
        proxy = ProxyH2(protos=(), bind="127.0.0.1:0")
        server = await proxy.start_server()
        try:
            assert isinstance(server, EggressServer)
            assert len(server.addresses) > 0
        finally:
            await server.aclose()

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
            await proxy.start_server()

    asyncio.run(_run())


def test_quic_start_server_raises():
    """ProxyQUIC.start_server() raises NotImplementedError."""
    from eggress._pproxy_proxy import ProxyQUIC
    proxy = ProxyQUIC()

    async def _run():
        with pytest.raises(NotImplementedError, match="QUIC"):
            await proxy.start_server()

    asyncio.run(_run())


def test_h3_start_server_raises():
    """ProxyH3.start_server() raises NotImplementedError (QUIC)."""
    from eggress._pproxy_proxy import ProxyH3
    proxy = ProxyH3()

    async def _run():
        with pytest.raises(NotImplementedError):
            await proxy.start_server()

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
        server = await proxy.start_server()
        try:
            assert len(server.addresses) > 0
            st = server.status()
            assert st.get("readiness") is True
        finally:
            await server.aclose()
        assert server.addresses == {}

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
        server = await proxy.start_server()
        try:
            assert len(server.addresses) > 0
        finally:
            await server.aclose()

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


def test_build_listen_uri_custom_bind():
    """_build_listen_uri uses stored bind address."""
    from eggress._pproxy_proxy import ProxySimple
    from eggress.protocol import HTTP
    proxy = ProxySimple(protos=(HTTP,), bind="0.0.0.0:8080")
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
    """Perform a SOCKS5 CONNECT handshake and return the connected socket."""
    import socket
    import struct

    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(5.0)
    s.connect((proxy_host, proxy_port))
    s.sendall(b"\x05\x01\x00")
    resp = s.recv(2)
    assert resp[0] == 0x05, f"SOCKS5 greeting version mismatch: {resp!r}"
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


def test_start_server_socks5_relay():
    """SOCKS5 started via start_server relays data to an echo server."""
    echo_host, echo_port, echo_srv = _echo_server()
    try:
        async def _run():
            proxy = _get_proxy_simple("socks5://127.0.0.1:0")
            server = await proxy.start_server()
            try:
                addrs = server.addresses
                assert len(addrs) > 0
                # Find the SOCKS5 listener
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
                    s.sendall(b"ping")
                    resp = s.recv(4096)
                    assert resp == b"ping"
                finally:
                    s.close()
            finally:
                await server.aclose()

        asyncio.run(_run())
    finally:
        echo_srv.close()
