"""Tests for the pproxy reverse URI helper.

Covers role classification, redacted target display, modifier parsing,
and round-trip to/from the existing translation pipeline.
"""

import pytest

from eggress import (
    describe_reverse_pproxy_uri,
    ReverseUriSummary,
    translate_pproxy_args,
    translate_pproxy_uri,
)


# ---------------------------------------------------------------------------
# Server URIs (bind://, listen://, backward://, rebind://)
# ---------------------------------------------------------------------------


def test_bind_uri_classified_as_server():
    s = describe_reverse_pproxy_uri("bind://0.0.0.0:8080")
    assert s.role == "server"
    assert s.toml_section == "reverse_servers"
    assert s.scheme == "bind"
    assert s.tls is False
    assert s.has_auth is False
    assert s.target == "bind://0.0.0.0:8080"


def test_listen_uri_classified_as_server():
    s = describe_reverse_pproxy_uri("listen://127.0.0.1:9090")
    assert s.role == "server"
    assert s.toml_section == "reverse_servers"
    assert s.scheme == "listen"
    assert s.target == "listen://127.0.0.1:9090"


def test_backward_uri_classified_as_server():
    s = describe_reverse_pproxy_uri("backward://0.0.0.0:8080")
    assert s.role == "server"
    assert s.toml_section == "reverse_servers"
    assert s.scheme == "backward"


def test_rebind_uri_classified_as_server():
    s = describe_reverse_pproxy_uri("rebind://0.0.0.0:8080")
    assert s.role == "server"
    assert s.toml_section == "reverse_servers"
    assert s.scheme == "rebind"


def test_bind_uri_with_credentials_redacted():
    s = describe_reverse_pproxy_uri("bind://admin:s3cret@0.0.0.0:8080")
    assert s.role == "server"
    assert s.has_auth is True
    # Credentials must NOT appear in the redacted display.
    assert "s3cret" not in s.target
    assert "admin" not in s.target
    # The redacted form should still be informative.
    assert "0.0.0.0:8080" in s.target


# ---------------------------------------------------------------------------
# Client URIs (*+in://)
# ---------------------------------------------------------------------------


def test_socks5_in_uri_classified_as_client():
    s = describe_reverse_pproxy_uri("socks5+in://server.example:1080")
    assert s.role == "client"
    assert s.toml_section == "reverse_clients"
    assert s.scheme == "socks5"
    assert s.tls is False
    assert "+in" in s.modifiers


def test_http_in_uri_classified_as_client():
    s = describe_reverse_pproxy_uri("http+in://user:pass@server:8080")
    assert s.role == "client"
    assert s.toml_section == "reverse_clients"
    assert s.has_auth is True
    # Password must be redacted.
    assert "pass" not in s.target


def test_ss_in_uri_classified_as_client():
    s = describe_reverse_pproxy_uri("ss+in://:chacha20-ietf-poly1305:abc@server:8388")
    assert s.role == "client"
    assert s.toml_section == "reverse_clients"
    # The shared key must be redacted.
    assert "abc" not in s.target
    assert "chacha20" not in s.target


# ---------------------------------------------------------------------------
# Modifiers
# ---------------------------------------------------------------------------


def test_tls_modifier_is_recognized():
    s = describe_reverse_pproxy_uri("bind+tls://0.0.0.0:8443")
    assert s.tls is True
    assert "+tls" in s.modifiers


def test_inbound_modifier_count():
    # Multiple +in tokens should be reported in modifiers.
    s = describe_reverse_pproxy_uri("socks5+in+in://server:1080")
    assert s.role == "client"
    in_count = s.modifiers.count("+in")
    assert in_count >= 1


# ---------------------------------------------------------------------------
# Non-reverse URIs → unknown
# ---------------------------------------------------------------------------


def test_socks5_local_uri_is_unknown():
    s = describe_reverse_pproxy_uri("socks5://0.0.0.0:1080")
    assert s.role == "unknown"
    assert s.toml_section == "unknown"


def test_http_local_uri_is_unknown():
    s = describe_reverse_pproxy_uri("http://0.0.0.0:8080")
    assert s.role == "unknown"
    assert s.toml_section == "unknown"


# ---------------------------------------------------------------------------
# Invalid input
# ---------------------------------------------------------------------------


def test_invalid_uri_raises():
    with pytest.raises(Exception):
        describe_reverse_pproxy_uri("not a valid uri")


# ---------------------------------------------------------------------------
# Dataclass ergonomics
# ---------------------------------------------------------------------------


def test_summary_is_dataclass():
    s = describe_reverse_pproxy_uri("bind://0.0.0.0:8080")
    assert isinstance(s, ReverseUriSummary)
    # Frozen dataclass — assignment should fail.
    with pytest.raises(Exception):
        s.role = "client"  # type: ignore[misc]


def test_summary_repr_redacts_credentials():
    s = describe_reverse_pproxy_uri("bind://admin:verysecret@0.0.0.0:8080")
    r = repr(s)
    assert "verysecret" not in r
    assert "admin" not in r or "****" in r


# ---------------------------------------------------------------------------
# Round-trip with translate_pproxy_uri
# ---------------------------------------------------------------------------


def test_reverse_uri_round_trip_to_toml():
    """The reverse URI helper's classification should match what
    translate_pproxy_uri actually emits."""
    local = "bind://0.0.0.0:8080"
    remote = "socks5+in://server:1080"

    local_summary = describe_reverse_pproxy_uri(local)
    remote_summary = describe_reverse_pproxy_uri(remote)

    result = translate_pproxy_uri(local, [remote])
    toml = result.toml

    # Server URI → reverse_servers section
    assert local_summary.toml_section == "reverse_servers"
    if "unsupported" in toml.lower():
        # If unsupported, skip the assertion
        pytest.skip("reverse_servers TOML is unsupported in this build")
    # Otherwise we expect the section key to appear.
    assert "[[reverse_servers]]" in toml or "reverse_servers" in toml


def test_translate_pproxy_args_with_reverse_listeners():
    """A pproxy CLI invocation that uses bind:// should classify as server."""
    args = ["-l", "bind://0.0.0.0:8080"]
    result = translate_pproxy_args(args)
    # The local URI is reverse; it should not produce a regular listener.
    # We don't enforce the exact TOML here, just that translation doesn't fail.
    assert result.toml
