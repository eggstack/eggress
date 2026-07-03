"""Tests for config explanation helpers (31.5), upstream test helpers (31.7),
and route explanation helpers."""
from __future__ import annotations

import pytest

from eggress.pproxy import (
    explain_config_toml,
    explain_pproxy_args,
    explain_pproxy_uri,
    route_explain,
    check_upstream,
)


# ---------------------------------------------------------------------------
# explain_config_toml
# ---------------------------------------------------------------------------

class TestExplainConfigToml:
    def test_empty_config(self) -> None:
        result = explain_config_toml("")
        assert result["listeners"] == []
        assert result["upstreams"] == []
        assert result["rules"] == []
        assert result["security_notes"] == []

    def test_basic_socks5_listener(self) -> None:
        toml = """\
[[listeners]]
name = "main"
bind = "socks5://0.0.0.0:1080"
protocols = ["socks5"]
"""
        result = explain_config_toml(toml)
        assert len(result["listeners"]) == 1
        l = result["listeners"][0]
        assert l["name"] == "main"
        assert l["bind"] == "socks5://0.0.0.0:1080"
        assert l["protocols"] == ["socks5"]

    def test_tls_listener_detected(self) -> None:
        toml = """\
[[listeners]]
name = "tls"
bind = "socks5://0.0.0.0:1080"
protocols = ["socks5"]

[listeners.tls]
cert_path = "/tmp/cert.pem"
key_path = "/tmp/key.pem"
"""
        result = explain_config_toml(toml)
        l = result["listeners"][0]
        assert l.get("tls") is True

    def test_udp_listener_detected(self) -> None:
        toml = """\
[[listeners]]
name = "udp"
bind = "socks5://0.0.0.0:1080"
protocols = ["socks5"]

[listeners.udp]
"""
        result = explain_config_toml(toml)
        l = result["listeners"][0]
        assert l.get("udp_enabled") is True

    def test_transparent_listener(self) -> None:
        toml = """\
[[listeners]]
name = "tproxy"
bind = "tcp://0.0.0.0:12345"
protocols = ["socks5"]

[listeners.transparent]
"""
        result = explain_config_toml(toml)
        l = result["listeners"][0]
        assert l.get("transparent") is True
        assert any("elevated privileges" in n for n in result["security_notes"])

    def test_upstream_redacted(self) -> None:
        toml = """\
[[upstreams]]
id = "p1"
uri = "socks5://user:pass@proxy.example.com:1080"
"""
        result = explain_config_toml(toml)
        u = result["upstreams"][0]
        assert u["id"] == "p1"
        assert "user" not in u["uri"]
        assert "pass" not in u["uri"]
        assert "****" in u["uri"]

    def test_upstream_group(self) -> None:
        toml = """\
[[upstream_groups]]
id = "g1"
scheduler = "round_robin"
members = ["u1", "u2"]
"""
        result = explain_config_toml(toml)
        g = result["upstream_groups"][0]
        assert g["id"] == "g1"
        assert g["scheduler"] == "round_robin"
        assert g["members"] == ["u1", "u2"]

    def test_rules(self) -> None:
        toml = """\
[[rules]]
id = "direct"
direct = true

[[rules]]
id = "proxy"
upstream_group = "g1"

[[rules]]
id = "block"
reject = "policy"
"""
        result = explain_config_toml(toml)
        assert len(result["rules"]) == 3
        assert result["rules"][0]["action"] == "direct"
        assert result["rules"][1]["action"] == "upstream"
        assert result["rules"][2]["action"] == "reject(policy)"

    def test_reverse_servers(self) -> None:
        toml = """\
[[reverse_servers]]
id = "rs1"
control_bind = "tcp://127.0.0.1:9999"
"""
        result = explain_config_toml(toml)
        rs = result["reverse_servers"][0]
        assert rs["id"] == "rs1"
        assert rs["control_bind"] == "tcp://127.0.0.1:9999"

    def test_reverse_clients(self) -> None:
        toml = """\
[[reverse_clients]]
id = "rc1"
server_addr = "tcp://10.0.0.1:9999"
"""
        result = explain_config_toml(toml)
        rc = result["reverse_clients"][0]
        assert rc["id"] == "rc1"
        assert rc["server_addr"] == "tcp://10.0.0.1:9999"

    def test_security_notes_transparent(self) -> None:
        toml = """\
[[listeners]]
name = "t"
bind = "tcp://0.0.0.0:12345"
protocols = ["socks5"]

[listeners.transparent]
"""
        result = explain_config_toml(toml)
        assert any("elevated privileges" in n for n in result["security_notes"])

    def test_security_notes_plaintext_creds(self) -> None:
        toml = """\
[[listeners]]
name = "auth"
bind = "socks5://0.0.0.0:1080"
protocols = ["socks5"]

[listeners.auth]
username = "admin"
password = "secret"
"""
        result = explain_config_toml(toml)
        assert any("plaintext auth" in n for n in result["security_notes"])

    def test_invalid_toml_raises(self) -> None:
        with pytest.raises(Exception, match="failed to parse TOML"):
            explain_config_toml("this is not valid {{{ TOML")

    def test_full_toml_config(self) -> None:
        toml = """\
[[listeners]]
name = "socks"
bind = "socks5://0.0.0.0:1080"
protocols = ["socks5"]

[[upstreams]]
id = "remote"
uri = "socks5://proxy:1080"

[[upstream_groups]]
id = "default"
scheduler = "round_robin"
members = ["remote"]

[[rules]]
id = "all"
upstream_group = "default"
"""
        result = explain_config_toml(toml)
        assert len(result["listeners"]) == 1
        assert len(result["upstreams"]) == 1
        assert len(result["upstream_groups"]) == 1
        assert len(result["rules"]) == 1


# ---------------------------------------------------------------------------
# explain_pproxy_args
# ---------------------------------------------------------------------------

class TestExplainPproxyArgs:
    def test_basic_socks5(self) -> None:
        result = explain_pproxy_args(["-l", "socks5://:1080"])
        assert "listeners" in result
        assert "toml" in result
        assert "ok" in result
        assert "warnings" in result
        assert "unsupported" in result
        assert result["ok"] is True
        assert len(result["listeners"]) >= 1

    def test_with_remote(self) -> None:
        result = explain_pproxy_args([
            "-l", "socks5://:1080",
            "-r", "socks5://proxy:1080",
        ])
        assert result["ok"] is True
        assert len(result["upstreams"]) >= 1

    def test_result_has_toml(self) -> None:
        result = explain_pproxy_args(["-l", "socks5://:1080"])
        assert isinstance(result["toml"], str)
        assert "listeners" in result["toml"] or "[[" in result["toml"]

    def test_unsupported_feature(self) -> None:
        result = explain_pproxy_args(["-l", "ssh://host:22"])
        assert result["ok"] is False
        assert len(result["unsupported"]) >= 1


# ---------------------------------------------------------------------------
# explain_pproxy_uri
# ---------------------------------------------------------------------------

class TestExplainPproxyUri:
    def test_basic_socks5_uri(self) -> None:
        result = explain_pproxy_uri("socks5://0.0.0.0:1080")
        assert "listeners" in result
        assert "toml" in result
        assert "ok" in result
        assert result["ok"] is True

    def test_uri_with_remote(self) -> None:
        result = explain_pproxy_uri(
            "socks5+in://0.0.0.0:1080;socks5://proxy:1080"
        )
        assert "listeners" in result
        assert "toml" in result

    def test_invalid_uri_raises(self) -> None:
        with pytest.raises(Exception):
            explain_pproxy_uri("not a valid uri")

    def test_result_structure(self) -> None:
        result = explain_pproxy_uri("socks5://0.0.0.0:1080")
        assert isinstance(result["listeners"], list)
        assert isinstance(result["upstreams"], list)
        assert isinstance(result["rules"], list)
        assert isinstance(result["warnings"], list)
        assert isinstance(result["unsupported"], list)
        assert isinstance(result["toml"], str)


# ---------------------------------------------------------------------------
# route_explain
# ---------------------------------------------------------------------------

class TestRouteExplain:
    def test_direct_rule(self) -> None:
        toml = """\
[[rules]]
id = "direct"
direct = true
"""
        result = route_explain(toml, "example.com:443")
        assert "target" in result
        assert result["target"] == "example.com:443"
        assert "action" in result
        assert result["matched_rule"] == "direct"

    def test_upstream_group_rule(self) -> None:
        toml = """\
[[upstream_groups]]
id = "proxy"
scheduler = "round-robin"
members = []

[[rules]]
id = "proxy"
upstream_group = "proxy"
"""
        result = route_explain(toml, "example.com:443")
        assert result["matched_rule"] == "proxy"
        assert result["upstream_group"] == "proxy"

    def test_reject_rule(self) -> None:
        toml = """\
[[rules]]
id = "block"
reject = "blocked"
"""
        result = route_explain(toml, "blocked.example.com:443")
        assert result["matched_rule"] == "block"
        assert "blocked" in result["action"]

    def test_default_action(self) -> None:
        toml = """\
[routing]
default = "direct"
"""
        result = route_explain(toml, "example.com:443")
        assert result["action"] == "direct"

    def test_empty_config_default(self) -> None:
        result = route_explain("", "example.com:443")
        assert "target" in result
        assert "action" in result

    def test_result_structure(self) -> None:
        result = route_explain("", "1.2.3.4:80")
        expected_keys = [
            "target", "listener", "protocol", "transport",
            "matched_rule", "action", "upstream_group", "scheduler",
            "eligible_upstreams", "selected_upstream", "chain", "generation",
        ]
        for key in expected_keys:
            assert key in result, f"missing key: {key}"

    def test_invalid_toml_raises(self) -> None:
        with pytest.raises(Exception, match="failed to parse TOML"):
            route_explain("not valid {{{ toml", "example.com:443")

    def test_invalid_target_raises(self) -> None:
        with pytest.raises(Exception, match="invalid target"):
            route_explain("", "not a target")


# ---------------------------------------------------------------------------
# check_upstream
# ---------------------------------------------------------------------------

class TestUpstream:
    def test_result_structure(self) -> None:
        result = check_upstream("socks5://127.0.0.1:19999", timeout=1.0)
        expected_keys = [
            "host", "port", "scheme", "has_auth",
            "redacted_uri", "connected", "error",
        ]
        for key in expected_keys:
            assert key in result, f"missing key: {key}"

    def test_local_unreachable(self) -> None:
        result = check_upstream("socks5://127.0.0.1:19999", timeout=0.5)
        assert result["connected"] is False
        assert result["error"] is not None
        assert result["host"] == "127.0.0.1"
        assert result["port"] == 19999
        assert result["scheme"] == "socks5"

    def test_redacted_with_auth(self) -> None:
        result = check_upstream(
            "socks5://user:pass@127.0.0.1:1080", timeout=0.5
        )
        assert "user" not in result["redacted_uri"]
        assert "pass" not in result["redacted_uri"]
        assert "****" in result["redacted_uri"]
        assert result["has_auth"] is True

    def test_redacted_without_auth(self) -> None:
        result = check_upstream("socks5://127.0.0.1:1080", timeout=0.5)
        assert "****" not in result["redacted_uri"]
        assert result["has_auth"] is False

    def test_default_ports(self) -> None:
        r1 = check_upstream("http://example.com", timeout=0.5)
        assert r1["port"] == 80
        r2 = check_upstream("ss://example.com:8388", timeout=0.5)
        assert r2["port"] == 8388

    def test_invalid_uri_raises(self) -> None:
        with pytest.raises(Exception, match="invalid URI"):
            check_upstream("not a uri", timeout=0.5)
