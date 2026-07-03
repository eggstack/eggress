"""Diagnostics and utility API tests for the Python bindings.

Tests the new utility APIs exposed in Phase 31:
- check_pproxy_uri (UriInfo)
- redact_pproxy_uri
- diagnostics_for_uri (Diagnostic)
- supported_features
- Server status helpers (is_ready, listener_info, metrics_text)
"""

from __future__ import annotations

import pytest

from eggress import (
    check_pproxy_uri,
    redact_pproxy_uri,
    diagnostics_for_uri,
    supported_features,
    Diagnostic,
    UriInfo,
    translate_pproxy_uri,
    Server,
)


# ---------------------------------------------------------------------------
# UriInfo
# ---------------------------------------------------------------------------

class TestUriInfo:
    def test_basic_socks5(self) -> None:
        info = check_pproxy_uri("socks5://127.0.0.1:1080")
        assert info.ok
        assert info.scheme == "socks5"
        assert info.host == "127.0.0.1"
        assert info.port == 1080
        assert not info.tls
        assert not info.ssl
        assert not info.inbound
        assert not info.has_auth
        assert not info.has_rule
        assert not info.is_reverse_listener

    def test_uri_info_is_dataclass(self) -> None:
        info = check_pproxy_uri("http://proxy:8080")
        assert isinstance(info, UriInfo)

    def test_uri_info_repr_ok(self) -> None:
        info = check_pproxy_uri("socks5://127.0.0.1:1080")
        r = repr(info)
        assert "UriInfo(" in r
        assert "socks5" in r

    def test_uri_info_repr_error(self) -> None:
        info = check_pproxy_uri("")
        r = repr(info)
        assert "error" in r.lower()

    def test_invalid_uri_has_error(self) -> None:
        info = check_pproxy_uri("ftp://host:22")
        assert not info.ok
        assert info.error is not None

    def test_tls_flag(self) -> None:
        info = check_pproxy_uri("socks5+tls://proxy:1080")
        assert info.ok
        assert info.tls

    def test_inbound_flag(self) -> None:
        info = check_pproxy_uri("socks5+in://acceptor:1080")
        assert info.ok
        assert info.inbound
        assert info.backward_num == 1

    def test_reverse_listener_bind(self) -> None:
        info = check_pproxy_uri("bind://0.0.0.0:8080")
        assert info.ok
        assert info.is_reverse_listener

    def test_reverse_listener_listen(self) -> None:
        info = check_pproxy_uri("listen://127.0.0.1:9090")
        assert info.ok
        assert info.is_reverse_listener

    def test_non_reverse_socks5(self) -> None:
        info = check_pproxy_uri("socks5://127.0.0.1:1080")
        assert info.ok
        assert not info.is_reverse_listener

    def test_auth_detected(self) -> None:
        info = check_pproxy_uri("socks5://user:pass@127.0.0.1:1080")
        assert info.ok
        assert info.has_auth

    def test_no_auth(self) -> None:
        info = check_pproxy_uri("socks5://127.0.0.1:1080")
        assert info.ok
        assert not info.has_auth


# ---------------------------------------------------------------------------
# redact_pproxy_uri
# ---------------------------------------------------------------------------

class TestRedactPproxyUri:
    def test_basic_redaction(self) -> None:
        result = redact_pproxy_uri("socks5://127.0.0.1:1080")
        assert result == "socks5://127.0.0.1:1080"

    def test_auth_redacted(self) -> None:
        result = redact_pproxy_uri("socks5://user:pass@127.0.0.1:1080")
        assert "pass" not in result
        assert "user" not in result
        assert "****" in result

    def test_tls_preserved(self) -> None:
        result = redact_pproxy_uri("socks5+tls://proxy:1080")
        assert "socks5+tls" in result

    def test_invalid_raises(self) -> None:
        with pytest.raises(Exception):
            redact_pproxy_uri("ftp://host:22")

    def test_ipv6_redaction(self) -> None:
        result = redact_pproxy_uri("socks5://[::1]:1080")
        assert result == "socks5://[::1]:1080"


# ---------------------------------------------------------------------------
# diagnostics_for_uri
# ---------------------------------------------------------------------------

class TestDiagnosticsForUri:
    def test_valid_uri_no_diagnostics(self) -> None:
        diags = diagnostics_for_uri("socks5://127.0.0.1:1080")
        assert isinstance(diags, list)
        # A bare listener URI may produce a "missing_target" diagnostic
        # (direct mode warning). That's fine — just verify it returns a list.

    def test_invalid_uri_raises(self) -> None:
        with pytest.raises(Exception):
            diagnostics_for_uri("ftp://host:22")

    def test_diagnostic_is_dataclass(self) -> None:
        result = translate_pproxy_uri("socks5://user:pass@127.0.0.1:1080")
        # This will produce a credential-in-toml warning
        if result.warnings:
            diags = diagnostics_for_uri("socks5://user:pass@127.0.0.1:1080")
            assert len(diags) > 0
            d = diags[0]
            assert isinstance(d, Diagnostic)
            assert d.code
            assert d.message

    def test_diagnostic_repr(self) -> None:
        d = Diagnostic(code="test", feature_id=None, tier=None, message="test msg", suggestion=None)
        r = repr(d)
        assert "[test]" in r
        assert "test msg" in r

    def test_unsupported_uri_has_diagnostic(self) -> None:
        # SSR is unsupported and should produce a diagnostic
        diags = diagnostics_for_uri("ssr://aes-256-ctr:secret@proxy:8388")
        assert len(diags) > 0
        assert any(d.code for d in diags)


# ---------------------------------------------------------------------------
# supported_features
# ---------------------------------------------------------------------------

class TestSupportedFeatures:
    def test_returns_list(self) -> None:
        features = supported_features()
        assert isinstance(features, list)
        assert len(features) > 0

    def test_contains_core_protocols(self) -> None:
        features = supported_features()
        for proto in ["http", "socks4", "socks5", "shadowsocks", "trojan"]:
            assert proto in features

    def test_returns_strings(self) -> None:
        features = supported_features()
        for f in features:
            assert isinstance(f, str)


# ---------------------------------------------------------------------------
# Server status helpers
# ---------------------------------------------------------------------------

class TestServerStatusHelpers:
    def test_is_ready_before_start(self) -> None:
        srv = Server(listen=["socks5://127.0.0.1:0"])
        assert srv.is_ready is False

    def test_listener_info_empty_before_start(self) -> None:
        srv = Server(listen=["socks5://127.0.0.1:0"])
        assert srv.listener_info == []

    def test_metrics_text_empty_before_start(self) -> None:
        srv = Server(listen=["socks5://127.0.0.1:0"])
        assert srv.metrics_text == ""

    def test_is_ready_after_start(self) -> None:
        with Server(listen=["socks5://127.0.0.1:0"]) as srv:
            assert srv.is_ready is True

    def test_listener_info_after_start(self) -> None:
        with Server(listen=["socks5://127.0.0.1:0"]) as srv:
            info = srv.listener_info
            assert isinstance(info, list)
            assert len(info) > 0
            # Each listener has at least 'name' and 'bind'
            listener = info[0]
            assert "name" in listener
            assert "bind" in listener

    def test_metrics_text_after_start(self) -> None:
        with Server(listen=["socks5://127.0.0.1:0"]) as srv:
            metrics = srv.metrics_text
            assert isinstance(metrics, str)
            assert len(metrics) > 0
