"""pproxy compatibility layer tests for Milestone C.

Tests the pproxy namespace, server utilities, proto helpers,
and cipher registry from the eggress-pproxy-compat distribution.
"""

from __future__ import annotations

import os
import sys
import tempfile

import pytest


# ---------------------------------------------------------------------------
# Namespace tests
# ---------------------------------------------------------------------------


class TestPproxyNamespace:
    """Test that pproxy namespace exposes expected symbols."""

    def test_import_pproxy(self):
        import pproxy

        assert hasattr(pproxy, "Connection")
        assert hasattr(pproxy, "Server")
        assert hasattr(pproxy, "DIRECT")
        assert hasattr(pproxy, "Rule")
        assert hasattr(pproxy, "proto")
        assert hasattr(pproxy, "cipher")
        assert hasattr(pproxy, "server")

    def test_connection_is_function(self):
        import pproxy

        assert callable(pproxy.Connection)

    def test_server_is_function(self):
        import pproxy

        assert callable(pproxy.Server)

    def test_direct_is_instance(self):
        import pproxy
        from pproxy.server import ProxyDirect

        assert isinstance(pproxy.DIRECT, ProxyDirect)
        assert pproxy.DIRECT.direct is True

    def test_rule_is_function(self):
        import pproxy

        assert callable(pproxy.Rule)

    def test_version_info(self):
        import pproxy

        assert hasattr(pproxy, "__version__")
        assert hasattr(pproxy, "__pproxy_compatibility_version__")
        assert pproxy.__pproxy_compatibility_version__ == "2.7.9"


# ---------------------------------------------------------------------------
# Server module tests
# ---------------------------------------------------------------------------


class TestPproxyServer:
    """Test pproxy.server module exports."""

    def test_auth_table_importable(self):
        from pproxy.server import AuthTable

        auth = AuthTable(remote_ip="10.0.0.1", authtime=30)
        assert auth.authed() is None

    def test_proxy_classes_importable(self):
        from pproxy.server import (
            ProxyDirect, ProxySimple, ProxyBackward, ProxyH2,
            ProxyQUIC, ProxySSH, ProxyH3,
        )

        d = ProxyDirect()
        assert d.direct is True

    def test_compile_rule_returns_match_function(self):
        from pproxy.server import compile_rule

        with tempfile.NamedTemporaryFile(mode="w", suffix=".txt", delete=False) as f:
            f.write("example.com\n")
            f.write("test.org\n")
            f.flush()
            try:
                result = compile_rule(f.name)
                assert callable(result)
                m = result("example.com")
                assert m is not None
                m = result("test.org")
                assert m is not None
                m = result("evil.com")
                assert m is None
            finally:
                os.unlink(f.name)

    def test_compile_rule_inline_regex(self):
        from pproxy.server import compile_rule

        result = compile_rule("{example\\.com}")
        assert callable(result)
        m = result("example.com")
        assert m is not None
        m = result("evil.com")
        assert m is None

    def test_schedule_fa(self):
        from pproxy.server import schedule

        class FakeProxy:
            alive = True
            connections = 0
            def match_rule(self, host, port):
                return True

        p1 = FakeProxy()
        p2 = FakeProxy()
        result = schedule([p1, p2], 'fa', 'example.com', 80)
        assert result is p1

    def test_check_server_alive_is_coroutine_function(self):
        from pproxy.server import check_server_alive
        import asyncio
        import inspect

        assert asyncio.iscoroutinefunction(check_server_alive)

    def test_prepare_ciphers_is_coroutine_function(self):
        from pproxy.server import prepare_ciphers
        import asyncio
        import inspect

        assert asyncio.iscoroutinefunction(prepare_ciphers)

    def test_prepare_ciphers_none_cipher(self):
        from pproxy.server import prepare_ciphers
        import asyncio

        reader, writer = None, None
        result = asyncio.run(prepare_ciphers(None, reader, writer))
        assert result == (None, None)


# ---------------------------------------------------------------------------
# Proto module tests
# ---------------------------------------------------------------------------


class TestPproxyProto:
    """Test pproxy.proto module exports and helpers."""

    def test_sslwrap_importable(self):
        from pproxy.proto import sslwrap

        assert callable(sslwrap)

    def test_all_exports(self):
        from pproxy.proto import __all__

        assert "socks_address" in __all__
        assert "sslwrap" in __all__
        assert "accept" in __all__
        assert "udp_accept" in __all__
        assert "get_protos" in __all__


# ---------------------------------------------------------------------------
# Cipher module tests
# ---------------------------------------------------------------------------


class TestPproxyCipher:
    """Test pproxy.cipher module exports."""

    def test_all_exports(self):
        from pproxy.cipher import __all__

        assert "get_cipher" in __all__
        assert "MAP" in __all__
        assert "BaseCipher" in __all__
        assert "AEADCipher" in __all__
        assert "StreamCipher" in __all__

    def test_get_cipher(self):
        from pproxy.cipher import get_cipher

        err, result = get_cipher("aes-256-gcm:password")
        assert err is None
        assert result is not None

    def test_map_completeness(self):
        from pproxy.cipher import MAP

        expected = [
            "aes-256-gcm", "aes-128-gcm", "chacha20-ietf-poly1305",
            "rc4", "rc4-md5", "chacha20", "salsa20",
            "aes-256-cfb", "aes-128-cfb",
            "aes-256-ofb", "aes-128-ofb",
            "aes-256-ctr", "aes-128-ctr",
        ]
        for name in expected:
            assert name in MAP, f"{name} missing from MAP"


# ---------------------------------------------------------------------------
# proxy_by_uri tests
# ---------------------------------------------------------------------------


class TestProxyByUri:
    """Test proxy_by_uri and proxies_by_uri factories."""

    def test_direct_uri(self):
        from pproxy.server import proxy_by_uri
        from pproxy.server import ProxyDirect

        proxy = proxy_by_uri("direct://", None)
        assert isinstance(proxy, ProxyDirect)

    def test_socks5_uri(self):
        from pproxy.server import proxy_by_uri
        from pproxy.server import ProxySimple

        proxy = proxy_by_uri("socks5://example.com:1080", None)
        assert isinstance(proxy, ProxySimple)

    def test_proxies_by_uri_chain(self):
        from pproxy.server import proxies_by_uri

        result = proxies_by_uri("socks5://h1:1080__socks5://h2:1080")
        assert result is not None

    def test_proxies_by_uri_single(self):
        from pproxy.server import proxies_by_uri
        from pproxy.server import ProxySimple

        result = proxies_by_uri("socks5://example.com:1080")
        assert isinstance(result, ProxySimple)
