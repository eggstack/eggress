"""Verify documented code examples from docs/PYTHON_BINDINGS.md run successfully.

These tests exercise the snippets shown in the user-facing documentation so that
documentation drift is caught at CI time. Each test maps to a doc section.
"""

import time

import pytest

from eggress import (
    EggressConfig,
    EggressService,
    Server,
    capabilities,
    check_pproxy_uri,
    describe_reverse_pproxy_uri,
    diagnostics_for_uri,
    explain_config_toml,
    explain_pproxy_args,
    explain_pproxy_uri,
    redact_pproxy_uri,
    route_explain,
    start_pproxy,
    supported_features,
    translate_pproxy_args,
    version,
)
from eggress.pproxy import compatibility_version


VALID_TOML = """
version = 1

[[listeners]]
name = "proxy"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""


# --- "Quick start > Context manager (recommended)" -----------------------


def test_docs_context_manager():
    with EggressService.from_toml(VALID_TOML).start() as handle:
        assert handle.bound_addresses


# --- "Quick start > Async context manager" -------------------------------


def test_docs_async_context_manager():
    import asyncio

    async def main():
        async with await EggressService.from_toml(VALID_TOML).astart() as handle:
            return await handle.bound_addresses()

    addrs = asyncio.run(main())
    assert addrs


# --- "Quick start > Explicit start/stop" ---------------------------------


def test_docs_explicit_start_stop():
    svc = EggressService.from_toml(VALID_TOML)
    handle = svc.start()
    try:
        assert handle.status()
        assert handle.metrics_text()
    finally:
        handle.shutdown()


# --- "Quick start > Loading from a file" ---------------------------------


def test_docs_load_from_file(tmp_path):
    cfg_path = tmp_path / "config.toml"
    cfg_path.write_text(VALID_TOML)
    config = EggressConfig.from_file(str(cfg_path))
    handle = EggressService(config).start()
    try:
        assert handle.bound_addresses
    finally:
        handle.shutdown()


# --- "Quick start > Starting from pproxy arguments" ----------------------


def test_docs_from_pproxy_args():
    svc = EggressService.from_pproxy_args([
        "-l", "socks5://127.0.0.1:0",
    ])
    with svc.start() as handle:
        assert handle.bound_addresses


def test_docs_start_pproxy_convenience():
    with start_pproxy(["-l", "socks5://127.0.0.1:0"]) as handle:
        assert handle.bound_addresses


# --- "Translation result types" ------------------------------------------


def test_docs_translate_pproxy_args():
    result = translate_pproxy_args(["-l", "socks5://:1080", "-r", "http://proxy:8080"])
    assert result.toml
    assert result.warnings == [] or isinstance(result.warnings, list)
    assert isinstance(result.unsupported, list)
    assert isinstance(result.ok, bool)


def test_docs_translate_pproxy_uri():
    result = translate_pproxy_args(["-l", "socks5://:1080"])
    assert result.ok


# --- "URI inspection and diagnostics" ------------------------------------


def test_docs_check_pproxy_uri_creds():
    info = check_pproxy_uri("socks5://user:pass@example.com:1080")
    assert info.scheme == "socks5"
    assert info.host == "example.com"
    assert info.port == 1080
    assert info.has_auth is True
    assert info.ok is True


def test_docs_check_pproxy_uri_error():
    info = check_pproxy_uri("invalid://")
    assert info.ok is False
    assert info.error is not None


def test_docs_redact_pproxy_uri():
    out = redact_pproxy_uri("socks5://secret:word@proxy:1080")
    assert "****" in out


def test_docs_diagnostics_for_uri():
    diags = diagnostics_for_uri("ssh://proxy:22")
    for d in diags:
        assert d.code
        assert d.message


def test_docs_supported_features():
    features = supported_features()
    assert "socks5" in features
    assert "http" in features
    assert "ssh" not in features


# --- "Reverse URI inspection" --------------------------------------------


def test_docs_describe_reverse_pproxy_uri():
    summary = describe_reverse_pproxy_uri("socks5+in://user:pass@host:1080")
    assert summary.role in ("server", "client", "unknown")
    assert summary.toml_section in ("reverse_servers", "reverse_clients", "unknown")
    assert summary.has_auth is True


# --- "Config explanation" ------------------------------------------------


def test_docs_explain_config_toml():
    info = explain_config_toml(VALID_TOML)
    assert "listeners" in info
    assert "upstreams" in info
    assert "rules" in info


def test_docs_explain_pproxy_args():
    info = explain_pproxy_args(["-l", "socks5://:1080", "-r", "http://proxy:8080"])
    assert "ok" in info
    assert "toml" in info


def test_docs_explain_pproxy_uri():
    info = explain_pproxy_uri("socks5://:1080")
    assert "ok" in info
    assert "toml" in info


# --- "Routing and upstream helpers" --------------------------------------


def test_docs_route_explain():
    toml_with_rule = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[[rules]]
id = "all"
direct = true

[[upstreams]]
id = "direct"
uri = "socks5://127.0.0.1:1080"
"""
    info = route_explain(toml_with_rule, "example.com:443")
    assert "matched_rule" in info
    assert "action" in info


# --- "Package metadata" --------------------------------------------------


def test_docs_version():
    assert isinstance(version(), str)
    assert version() == __import__("eggress").__version__


def test_docs_capabilities():
    caps = capabilities()
    assert caps["version"]
    assert caps["pproxy_compatibility_version"] == "2.7.9"
    assert "supported_protocols" in caps
    assert "supported_schedulers" in caps


def test_docs_compatibility_version():
    assert compatibility_version() == "2.7.9"


# --- "Server status helpers" ---------------------------------------------


def test_docs_server_status_helpers():
    server = Server(listen=["socks5://127.0.0.1:0"])
    try:
        assert server.addresses == {}
        assert server.is_ready is False
        assert server.listener_info == []
        assert server.metrics_text == ""
        server.start()
        time.sleep(0.1)
        assert server.is_ready is True
        assert server.config is not None  # Phase 29-32 closure: Server.config
    finally:
        server.close()