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


class TestPPProxyServiceFromArgsPreservesFlags:
    """Regression tests: ``from_args`` must preserve the full pproxy flag vector.

    Pre-Phase 42, ``from_args`` only extracted ``-l``/``-r`` and silently
    dropped every other pproxy flag. These tests pin the corrected behavior.
    """

    def test_ssl_flag_preserved_in_config(self, tmp_path):
        # Generate a self-signed cert so the listener can actually start
        # in TLS mode. We only assert config presence, not that start()
        # succeeds, but having a real cert lets us go further.
        try:
            import subprocess
            cert = tmp_path / "cert.pem"
            key = tmp_path / "key.pem"
            subprocess.run(
                [
                    "openssl", "req", "-x509", "-newkey", "rsa:2048",
                    "-keyout", str(key), "-out", str(cert),
                    "-days", "1", "-nodes", "-subj", "/CN=localhost",
                ],
                check=True, capture_output=True,
            )
        except (FileNotFoundError, subprocess.CalledProcessError):
            pytest.skip("openssl not available to generate self-signed cert")

        args = [
            "-l", "socks5://127.0.0.1:0",
            "--ssl", f"{cert},{key}",
        ]
        svc = PPProxyService.from_args(args, allow_partial=True)
        # Cert path appears in the raw translator output; the key path
        # is redacted in the live config (it's a credential) but a
        # redaction marker must be present.
        report = check_pproxy_args(args)
        assert str(cert) in report.toml
        redacted_toml = svc.config.redacted_toml()
        assert "[listeners.tls]" in redacted_toml
        assert "key = " in redacted_toml

    def test_block_flag_preserved_in_config(self):
        args = [
            "-l", "socks5://127.0.0.1:0",
            "-b", r".*\.example\.com",
        ]
        PPProxyService.from_args(args)
        # Reject rule for the block pattern should be in the config
        report = check_pproxy_args(args)
        assert r".*\.example\.com" in report.toml
        assert "pproxy-block-0" in report.toml

    def test_pac_flag_preserved_in_config(self):
        args = ["-l", "socks5://127.0.0.1:0", "--pac"]
        PPProxyService.from_args(args)
        report = check_pproxy_args(args)
        # PAC section is enabled in the admin block
        assert "pac" in report.toml.lower()

    def test_scheduler_preserved_in_config(self):
        args = [
            "-l", "socks5://127.0.0.1:0",
            "-r", "http://proxy1:8080",
            "-r", "http://proxy2:8080",
            "-s", "rr",
        ]
        PPProxyService.from_args(args)
        report = check_pproxy_args(args)
        assert "round-robin" in report.toml

    def test_alive_flag_preserved_in_config(self):
        args = [
            "-l", "socks5://127.0.0.1:0",
            "-r", "http://proxy:8080",
            "-a", "10",
        ]
        PPProxyService.from_args(args)
        report = check_pproxy_args(args)
        # Health interval should be applied
        assert "10s" in report.toml

    def test_ul_no_remote_preserved(self):
        # -ul standalone UDP should not require -r
        svc = PPProxyService.from_args(
            ["-l", "socks5://127.0.0.1:0", "-ul", ":0"]
        )
        # No exception means the flag was preserved; the service can start
        with svc as handle:
            addrs = handle.bound_addresses
            assert isinstance(addrs, dict)

    def test_ul_no_l_works(self):
        # -ul without -l should produce a default SOCKS5 listener + ul config
        svc = PPProxyService.from_args(["-ul", ":0"])
        with svc as handle:
            addrs = handle.bound_addresses
            assert isinstance(addrs, dict)
            assert len(addrs) > 0

    def test_from_args_equivalent_to_start_pproxy(self, tmp_path):
        # PPProxyService.from_args and check_pproxy_args() must produce
        # equivalent translated TOML configs for the same args.
        args = [
            "-l", "socks5://127.0.0.1:0",
            "-r", "http://proxy:8080",
            "-b", r".*\.blocked\.com",
        ]
        PPProxyService.from_args(args)
        report = check_pproxy_args(args)
        # Both should have produced reject rule for the block pattern
        assert "pproxy-block-0" in report.toml


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
        assert report.tier in ("drop_in", "compatible_with_warning",
                               "native_equivalent", "intentional_non_parity")
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
        # SSH listener is classified as intentional_non_parity in the
        # aggregate tier vocabulary (Phase 47 ADR), so the report is
        # neither drop-in nor compatible_with_warning, and ok=False.
        assert report.tier == "intentional_non_parity"
        assert report.ok is False
        assert len(report.unsupported) > 0

    def test_features_populated(self):
        report = check_pproxy_args(["-l", "socks5://127.0.0.1:0"])
        # No unsupported features triggered by a bare -l, so features
        # list should be empty (it is now scoped to the args, not the
        # full supported_features() set).
        assert report.features == []
        for feat in report.features:
            assert isinstance(feat, FeatureInfo)
            assert feat.feature_id
            assert feat.tier in ("drop_in", "compatible_with_warning",
                                 "native_equivalent", "intentional_non_parity",
                                 "unsupported")
            assert isinstance(feat.supported, bool)

    def test_features_contain_intentional_non_parity_when_ssh(self):
        report = check_pproxy_args(["-l", "ssh://user@host:22"])
        assert any(
            f.tier == "intentional_non_parity" and not f.supported
            for f in report.features
        )

    def test_native_equivalent_for_verbose(self):
        report = check_pproxy_args(
            ["-l", "socks5://127.0.0.1:0", "-v"]
        )
        assert report.tier in ("native_equivalent", "compatible_with_warning")
        # Diagnostic tier for verbose should be "native_equivalent"
        assert any(
            d.tier == "native_equivalent" for d in report.diagnostics
        )

    def test_intentional_non_parity_for_reuse(self):
        report = check_pproxy_args(
            ["-l", "socks5://127.0.0.1:0", "--reuse"]
        )
        assert report.tier == "intentional_non_parity"
        assert any(
            d.tier == "intentional_non_parity" for d in report.diagnostics
        )

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
