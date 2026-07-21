"""pproxy compatibility layer tests for Milestone C.

Tests the pproxy namespace, server utilities, proto helpers,
and cipher registry from the eggress-pproxy-compat distribution.
"""

from __future__ import annotations

import os
import sys

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
        from eggress._pproxy_proxy import ProxyDirect

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

        s = ProxySimple(host_name="example.com", port=443)
        assert s.direct is False

    def test_compile_rule(self):
        from pproxy.server import compile_rule

        result = compile_rule("test.txt")
        assert isinstance(result, dict)

    def test_schedule(self):
        from pproxy.server import schedule
        from eggress._pproxy_proxy import ProxyDirect

        assert schedule([]) is None
        p = ProxyDirect()
        assert schedule([p]) is p

    def test_check_server_alive(self):
        from pproxy.server import check_server_alive

        assert check_server_alive(None) is False

    def test_prepare_ciphers(self):
        from pproxy.server import prepare_ciphers

        result = prepare_ciphers(cipher_key="aes-256-gcm:testpass")
        assert "cipher" in result


# ---------------------------------------------------------------------------
# Proto module tests
# ---------------------------------------------------------------------------


class TestPproxyProto:
    """Test pproxy.proto module exports and helpers."""

    def test_socks_address_ipv4(self):
        from pproxy.proto import socks_address

        addr = socks_address("10.0.0.1", 8080)
        assert addr[0:1] == b"\x01"

    def test_socks_address_domain(self):
        from pproxy.proto import socks_address

        addr = socks_address("example.com", 443)
        assert addr[0:1] == b"\x03"

    def test_socks_address_ipv6(self):
        from pproxy.proto import socks_address

        addr = socks_address("::1", 80)
        assert addr[0:1] == b"\x04"

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
        from eggress._pproxy_proxy import ProxyDirect

        proxy = proxy_by_uri("direct://")
        assert isinstance(proxy, ProxyDirect)

    def test_socks5_uri(self):
        from pproxy.server import proxy_by_uri
        from eggress._pproxy_proxy import ProxySimple

        proxy = proxy_by_uri("socks5://example.com:1080")
        assert isinstance(proxy, ProxySimple)

    def test_empty_raises(self):
        from pproxy.server import proxy_by_uri

        with pytest.raises(TypeError):
            proxy_by_uri("")

    def test_proxies_by_uri_chain(self):
        from pproxy.server import proxies_by_uri

        result = proxies_by_uri("socks5://h1:1080__socks5://h2:1080")
        assert isinstance(result, list)
        assert len(result) == 2

    def test_proxies_by_uri_single(self):
        from pproxy.server import proxies_by_uri
        from eggress._pproxy_proxy import ProxySimple

        result = proxies_by_uri("socks5://example.com:1080")
        assert isinstance(result, ProxySimple)
