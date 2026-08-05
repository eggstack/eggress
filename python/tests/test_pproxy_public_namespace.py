"""Focused Phase 4 tests for the installed top-level pproxy namespace.

Includes Phase 2 behavioral honesty tests: no-silent-no-op guard,
unsupported class fallback prevention, and exception hierarchy validation.
"""

from __future__ import annotations

import asyncio
import inspect
import socket
import threading

import pytest

import pproxy


def test_public_namespace_and_aliases():
    from pproxy import cipher, proto, server

    assert pproxy.Connection is server.proxies_by_uri
    assert pproxy.Server is server.proxies_by_uri
    assert pproxy.Rule is server.compile_rule
    assert pproxy.DIRECT is server.DIRECT
    assert proto.Socks5 and cipher.AES_256_GCM_Cipher


def test_direct_tcp_connect_reader_writer_contract():
    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    address = listener.getsockname()

    def echo():
        conn, _ = listener.accept()
        with conn:
            payload = conn.recv(64)
            conn.sendall(payload)
        listener.close()

    thread = threading.Thread(target=echo, daemon=True)
    thread.start()

    async def exercise():
        reader, writer = await pproxy.Connection("direct://").tcp_connect(*address)
        writer.write(b"phase4")
        await writer.drain()
        assert await reader.readexactly(6) == b"phase4"
        writer.close()
        await writer.wait_closed()

    asyncio.run(exercise())
    thread.join(timeout=2)


def test_direct_udp_callback_contract():
    async def exercise():
        received = asyncio.get_running_loop().create_future()

        class Echo(asyncio.DatagramProtocol):
            def datagram_received(self, data, addr):
                self.transport.sendto(data, addr)

            def connection_made(self, transport):
                self.transport = transport

        transport, _ = await asyncio.get_running_loop().create_datagram_endpoint(
            Echo, local_addr=("127.0.0.1", 0)
        )
        target = transport.get_extra_info("sockname")
        proxy = pproxy.Connection("direct://")

        def callback(data):
            if not received.done():
                received.set_result(data)

        await proxy.udp_sendto(target[0], target[1], b"udp", callback)
        # The callback is scheduled by the datagram protocol.
        assert await asyncio.wait_for(received, 2) == b"udp"
        transport.close()

    asyncio.run(exercise())


# ---------------------------------------------------------------------------
# Phase 2: Behavioral honesty tests
# ---------------------------------------------------------------------------


class TestExceptionHierarchy:
    """The stable compatibility exception hierarchy is correct."""

    def test_pcompatibility_error_inherits_runtime_error(self):
        from eggress.pproxy import PProxyCompatibilityError
        assert issubclass(PProxyCompatibilityError, RuntimeError)

    def test_unsupported_feature_inherits_pcompatibility_error(self):
        from eggress.pproxy import PProxyCompatibilityError, UnsupportedPProxyFeature
        assert issubclass(UnsupportedPProxyFeature, PProxyCompatibilityError)

    def test_unsupported_feature_has_feature_and_alternative(self):
        from eggress.pproxy import UnsupportedPProxyFeature
        exc = UnsupportedPProxyFeature("test_feature", alternative="use X instead")
        assert exc.feature == "test_feature"
        assert exc.alternative == "use X instead"
        assert "test_feature" in str(exc)
        assert "use X instead" in str(exc)

    def test_unsupported_feature_alternative_optional(self):
        from eggress.pproxy import UnsupportedPProxyFeature
        exc = UnsupportedPProxyFeature("test_feature")
        assert exc.alternative is None

    def test_exceptions_exported_from_eggress_init(self):
        from eggress import PProxyCompatibilityError, UnsupportedPProxyFeature
        assert PProxyCompatibilityError is not None
        assert UnsupportedPProxyFeature is not None


class TestNoSilentNoOpGuard:
    """Operational methods must not silently succeed with None/empty."""

    @pytest.mark.parametrize("method_name,args,kwargs", [
        ("check_server_alive", (1, [], None), {}),
        ("stream_handler", (None, None, None, None, (), [], None, None), {}),
        ("datagram_handler", (None, b"", None, (), [], None, None, "fa"), {}),
        ("test_url", (), {}),
    ])
    def test_server_module_methods_raise(self, method_name, args, kwargs):
        """pproxy.server operational methods raise UnsupportedPProxyFeature."""
        from eggress.pproxy import UnsupportedPProxyFeature
        from pproxy import server
        method = getattr(server, method_name)
        with pytest.raises(UnsupportedPProxyFeature):
            if asyncio.iscoroutinefunction(method):
                asyncio.run(method(*args, **kwargs))
            else:
                method(*args, **kwargs)

    def test_check_server_alive_is_coroutine(self):
        """check_server_alive is an async function."""
        from pproxy.server import check_server_alive
        assert asyncio.iscoroutinefunction(check_server_alive)

    def test_stream_handler_is_coroutine(self):
        from pproxy.server import stream_handler
        assert asyncio.iscoroutinefunction(stream_handler)

    def test_datagram_handler_is_coroutine(self):
        from pproxy.server import datagram_handler
        assert asyncio.iscoroutinefunction(datagram_handler)


class TestUnsupportedClassFallback:
    """Unsupported proxy classes cannot silently execute as supported protocols."""

    def test_proxy_ssh_start_server_raises(self):
        from eggress._pproxy_proxy import ProxySSH
        from eggress.pproxy import UnsupportedPProxyFeature
        proxy = ProxySSH()
        with pytest.raises(UnsupportedPProxyFeature, match="SSH"):
            asyncio.run(proxy.start_server(args={}))

    def test_proxy_ssh_tcp_connect_raises(self):
        from eggress._pproxy_proxy import ProxySSH
        from eggress.pproxy import UnsupportedPProxyFeature
        proxy = ProxySSH()
        with pytest.raises(UnsupportedPProxyFeature, match="SSH"):
            asyncio.run(proxy.tcp_connect("host", 22))

    def test_proxy_quic_start_server_raises(self):
        from eggress._pproxy_proxy import ProxyQUIC
        from eggress.pproxy import UnsupportedPProxyFeature
        proxy = ProxyQUIC()
        with pytest.raises(UnsupportedPProxyFeature, match="QUIC"):
            asyncio.run(proxy.start_server(args={}))

    def test_proxy_quic_tcp_connect_raises(self):
        from eggress._pproxy_proxy import ProxyQUIC
        from eggress.pproxy import UnsupportedPProxyFeature
        proxy = ProxyQUIC()
        with pytest.raises(UnsupportedPProxyFeature, match="QUIC"):
            asyncio.run(proxy.tcp_connect("host", 443))

    def test_proxy_h3_start_server_raises(self):
        from eggress._pproxy_proxy import ProxyH3
        from eggress.pproxy import UnsupportedPProxyFeature
        proxy = ProxyH3()
        with pytest.raises(UnsupportedPProxyFeature):
            asyncio.run(proxy.start_server(args={}))

    def test_proxy_backward_close_raises(self):
        from eggress._pproxy_proxy import ProxyBackward
        from eggress.pproxy import UnsupportedPProxyFeature
        proxy = ProxyBackward()
        with pytest.raises(UnsupportedPProxyFeature, match="close"):
            proxy.close()

    def test_proxy_backward_start_backward_client_raises(self):
        from eggress._pproxy_proxy import ProxyBackward
        from eggress.pproxy import UnsupportedPProxyFeature
        proxy = ProxyBackward()
        with pytest.raises(UnsupportedPProxyFeature, match="start_backward_client"):
            proxy.start_backward_client(args={})

    def test_plugin_get_plugin_raises(self):
        from pproxy.plugin import get_plugin
        from eggress.pproxy import UnsupportedPProxyFeature
        with pytest.raises(UnsupportedPProxyFeature, match="plugin"):
            get_plugin("test_plugin")

    def test_proto_sslwrap_raises(self):
        from pproxy.proto import sslwrap
        from eggress.pproxy import UnsupportedPProxyFeature
        with pytest.raises(UnsupportedPProxyFeature, match="sslwrap"):
            sslwrap(None, None)


class TestProxySSHStructuralOnly:
    """ProxySSH is structural-only; construction works but methods fail."""

    def test_constructible(self):
        from eggress._pproxy_proxy import ProxySSH
        proxy = ProxySSH()
        assert proxy is not None

    def test_direct_is_false(self):
        from eggress._pproxy_proxy import ProxySSH
        proxy = ProxySSH()
        assert proxy.direct is False

    def test_protos_accessible(self):
        from eggress._pproxy_proxy import ProxySSH
        proxy = ProxySSH()
        assert proxy.protos == ()

    def test_patch_stream_is_structural(self):
        from eggress._pproxy_proxy import ProxySSH
        proxy = ProxySSH()
        # patch_stream is structural; does not raise
        proxy.patch_stream(None, None, "host", 22)


class TestProxyQUICStructuralOnly:
    """ProxyQUIC is structural-only; construction works but methods fail."""

    def test_constructible(self):
        from eggress._pproxy_proxy import ProxyQUIC
        proxy = ProxyQUIC()
        assert proxy is not None

    def test_patch_writer_returns_argument(self):
        from eggress._pproxy_proxy import ProxyQUIC
        proxy = ProxyQUIC()
        sentinel = object()
        assert proxy.patch_writer(sentinel) is sentinel


class TestProxyH3StructuralOnly:
    """ProxyH3 is structural-only; construction works but methods fail."""

    def test_constructible(self):
        from eggress._pproxy_proxy import ProxyH3
        proxy = ProxyH3()
        assert proxy is not None

    def test_get_protocol_returns_none(self):
        from eggress._pproxy_proxy import ProxyH3
        proxy = ProxyH3()
        assert proxy.get_protocol() is None

    def test_get_stream_returns_none(self):
        from eggress._pproxy_proxy import ProxyH3
        proxy = ProxyH3()
        assert proxy.get_stream(None, 0) is None


class TestPrintServerStarted:
    """print_server_started formats and returns a message."""

    def test_returns_string_with_args(self):
        from pproxy.server import print_server_started
        result = print_server_started("server", "started")
        assert isinstance(result, str)
        assert "server" in result
        assert "started" in result

    def test_returns_none_with_no_args(self):
        from pproxy.server import print_server_started
        result = print_server_started()
        assert result is None

    def test_includes_keyword_args(self):
        from pproxy.server import print_server_started
        result = print_server_started(host="0.0.0.0", port=8080)
        assert "host=0.0.0.0" in result
        assert "port=8080" in result

