"""Tests for the Phase 40 Python pproxy drop-in API."""

from __future__ import annotations

import os
import tempfile

import pytest

import eggress
from eggress import (
    PPProxyService,
    PPProxyHandle,
    CompatibilityReport,
    FeatureInfo,
    EggressHandle,
    EggressService,
    UnsupportedFeatureError,
    start_pproxy,
    check_pproxy_args,
    translate_pproxy_args,
)


class TestImports:
    """Verify all new types are importable."""

    def test_import_pproxy_service(self):
        assert PPProxyService is not None

    def test_import_pproxy_handle_alias(self):
        assert PPProxyHandle is EggressHandle

    def test_import_compatibility_report(self):
        assert CompatibilityReport is not None

    def test_import_feature_info(self):
        assert FeatureInfo is not None

    def test_check_pproxy_args_returns_report(self):
        result = check_pproxy_args(["-l", "socks5://127.0.0.1:0"])
        assert isinstance(result, CompatibilityReport)


class TestPPProxyServiceFromArgs:
    """Test PPProxyService.from_args factory method."""

    def test_from_args_starts_service(self):
        svc = PPProxyService.from_args(["-l", "socks5://127.0.0.1:0"])
        with svc as handle:
            addrs = handle.bound_addresses
            assert len(addrs) > 0

    def test_from_args_with_remote(self):
        svc = PPProxyService.from_args(
            ["-l", "socks5://127.0.0.1:0", "-r", "http://127.0.0.1:9999"]
        )
        with svc as handle:
            addrs = handle.bound_addresses
            assert len(addrs) > 0

    def test_from_args_context_manager(self):
        with PPProxyService.from_args(["-l", "socks5://127.0.0.1:0"]) as handle:
            addrs = handle.bound_addresses
            assert len(addrs) > 0

    def test_from_args_unsupported_raises(self):
        with pytest.raises(UnsupportedFeatureError):
            PPProxyService.from_args(
                ["-l", "ssh://user@host:22"], allow_partial=False
            )

    def test_from_args_unsupported_allow_partial(self):
        svc = PPProxyService.from_args(
            ["-l", "ssh://user@host:22"], allow_partial=True
        )
        assert svc is not None


class TestPPProxyServiceFromUri:
    """Test PPProxyService.from_uri factory method."""

    def test_from_uri_starts_service(self):
        svc = PPProxyService.from_uri("socks5://127.0.0.1:0")
        with svc as handle:
            addrs = handle.bound_addresses
            assert len(addrs) > 0

    def test_from_uri_with_remotes(self):
        svc = PPProxyService.from_uri(
            "socks5://127.0.0.1:0",
            remotes=["http://127.0.0.1:9999"],
        )
        with svc as handle:
            addrs = handle.bound_addresses
            assert len(addrs) > 0

    def test_from_uri_context_manager(self):
        with PPProxyService.from_uri("socks5://127.0.0.1:0") as handle:
            addrs = handle.bound_addresses
            assert len(addrs) > 0


class TestPPProxyServiceFromToml:
    """Test PPProxyService.from_toml factory method."""

    def test_from_toml_starts_service(self):
        toml = """\
[[listeners]]
name = "test"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""
        svc = PPProxyService.from_toml(toml)
        with svc as handle:
            addrs = handle.bound_addresses
            assert len(addrs) > 0

    def test_from_toml_invalid_raises(self):
        with pytest.raises(Exception):
            PPProxyService.from_toml("not valid toml {{")


class TestPPProxyServiceFromFile:
    """Test PPProxyService.from_file factory method."""

    def test_from_file_starts_service(self):
        toml = """\
[[listeners]]
name = "test"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".toml", delete=False
        ) as f:
            f.write(toml)
            f.flush()
            path = f.name
        try:
            svc = PPProxyService.from_file(path)
            with svc as handle:
                addrs = handle.bound_addresses
                assert len(addrs) > 0
        finally:
            os.unlink(path)


class TestPPProxyServiceRepr:
    """Test PPProxyService repr."""

    def test_repr_stopped(self):
        svc = PPProxyService.from_args(["-l", "socks5://127.0.0.1:0"])
        assert "stopped" in repr(svc)

    def test_repr_running(self):
        svc = PPProxyService.from_args(["-l", "socks5://127.0.0.1:0"])
        with svc as _:
            assert "running" in repr(svc)


class TestStartPproxy:
    """Test the updated start_pproxy function."""

    def test_start_pproxy_with_args(self):
        with start_pproxy(["-l", "socks5://127.0.0.1:0"]) as handle:
            addrs = handle.bound_addresses
            assert len(addrs) > 0

    def test_start_pproxy_with_local(self):
        with start_pproxy(local="socks5://127.0.0.1:0") as handle:
            addrs = handle.bound_addresses
            assert len(addrs) > 0

    def test_start_pproxy_with_local_and_remote(self):
        with start_pproxy(
            local="socks5://127.0.0.1:0",
            remote="http://127.0.0.1:9999",
        ) as handle:
            addrs = handle.bound_addresses
            assert len(addrs) > 0

    def test_start_pproxy_with_config(self):
        toml = """\
[[listeners]]
name = "test"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""
        with start_pproxy(config=toml) as handle:
            addrs = handle.bound_addresses
            assert len(addrs) > 0

    def test_start_pproxy_conflicting_args_and_local(self):
        with pytest.raises(ValueError, match="mutually exclusive"):
            start_pproxy(
                ["-l", "socks5://127.0.0.1:0"],
                local="socks5://127.0.0.1:0",
            )

    def test_start_pproxy_no_args_raises(self):
        with pytest.raises(ValueError, match="at least one"):
            start_pproxy()

    def test_start_pproxy_unsupported_raises(self):
        with pytest.raises(UnsupportedFeatureError):
            start_pproxy(
                ["-l", "ssh://user@host:22"],
                allow_partial=False,
            )

    def test_start_pproxy_allow_partial(self):
        with start_pproxy(
            ["-l", "ssh://user@host:22"],
            allow_partial=True,
        ) as handle:
            assert handle is not None


class TestCompatibilityReport:
    """Test CompatibilityReport structure and content."""

    def test_supported_args_full_tier(self):
        report = check_pproxy_args(["-l", "socks5://127.0.0.1:0"])
        assert report.tier in ("full", "partial")
        assert report.ok is True
        assert isinstance(report.warnings, list)
        assert isinstance(report.unsupported, list)
        assert isinstance(report.diagnostics, list)
        assert isinstance(report.features, list)
        assert report.toml is not None
        assert isinstance(report.parsed_uris, dict)
        assert isinstance(report.raw_args, list)

    def test_unsupported_args_tier(self):
        report = check_pproxy_args(["-l", "ssh://user@host:22"])
        assert report.tier == "unsupported"
        assert report.ok is False
        assert len(report.unsupported) > 0

    def test_features_populated(self):
        report = check_pproxy_args(["-l", "socks5://127.0.0.1:0"])
        assert len(report.features) > 0
        for feat in report.features:
            assert isinstance(feat, FeatureInfo)
            assert feat.feature_id
            assert feat.tier in ("compatible", "partial", "unsupported")
            assert isinstance(feat.supported, bool)

    def test_parsed_uris_populated(self):
        report = check_pproxy_args(["-l", "socks5://127.0.0.1:0"])
        assert "socks5://127.0.0.1:0" in report.parsed_uris
        uri_info = report.parsed_uris["socks5://127.0.0.1:0"]
        assert uri_info.scheme == "socks5"
        assert uri_info.ok is True

    def test_raw_args_stored(self):
        args = ["-l", "socks5://127.0.0.1:0", "-r", "http://proxy:8080"]
        report = check_pproxy_args(args)
        assert report.raw_args == args

    def test_toml_generated(self):
        report = check_pproxy_args(["-l", "socks5://127.0.0.1:0"])
        assert report.toml is not None
        assert "[[listeners]]" in report.toml


class TestDoubleShutdown:
    """Test that double shutdown is safe."""

    def test_double_shutdown_no_error(self):
        svc = PPProxyService.from_args(["-l", "socks5://127.0.0.1:0"])
        handle = svc.start()
        handle.shutdown()
        # Second shutdown should not raise
        handle.shutdown()


class TestCredentialRedaction:
    """Test that credentials are not leaked in reprs or diagnostics."""

    def test_pproxy_service_repr_no_creds(self):
        svc = PPProxyService.from_uri(
            "socks5://user:pass@127.0.0.1:0",
            allow_partial=True,
        )
        r = repr(svc)
        assert "pass" not in r

    def test_compatibility_report_no_creds(self):
        report = check_pproxy_args(
            ["-l", "socks5://user:pass@127.0.0.1:0"]
        )
        # The raw_args contain the original URI, but TOML should be redacted
        assert report.toml is not None
        assert '"pass"' not in report.toml
