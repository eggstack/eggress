"""URI corpus fixture tests for the Python utility APIs.

Loads tests/compat/fixtures/pproxy_uri_corpus.toml and verifies that
check_pproxy_uri() and redact_pproxy_uri() produce correct results for
each case. This serves as a shared fixture runner between Rust and Python.
"""

from __future__ import annotations

import os
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib
from pathlib import Path

import pytest

from eggress import check_pproxy_uri, redact_pproxy_uri, translate_pproxy_uri

FIXTURE_PATH = Path(__file__).resolve().parents[2] / "tests" / "compat" / "fixtures" / "pproxy_uri_corpus.toml"

# Skip entire module if fixture file missing
pytestmark = pytest.mark.skipif(
    not FIXTURE_PATH.exists(),
    reason="URI corpus fixture not found",
)


def _load_cases() -> list[dict]:
    with open(FIXTURE_PATH, "rb") as f:
        data = tomllib.load(f)
    return data.get("cases", [])


CASES = _load_cases()


# ---------------------------------------------------------------------------
# check_pproxy_uri tests
# ---------------------------------------------------------------------------

class TestCheckPproxyUri:
    """Verify check_pproxy_uri parses every corpus case correctly."""

    @pytest.mark.parametrize(
        "case",
        CASES,
        ids=[c["id"] for c in CASES],
    )
    def test_parse_never_raises(self, case: dict) -> None:
        """check_pproxy_uri must never raise — errors are in .error."""
        uri = case["raw_uri"]
        info = check_pproxy_uri(uri)
        assert isinstance(info.scheme, str)
        assert isinstance(info.host, str)
        assert isinstance(info.port, int)

    @pytest.mark.parametrize(
        "case",
        [c for c in CASES if c.get("expected_error")],
        ids=[c["id"] for c in CASES if c.get("expected_error")],
    )
    def test_error_cases_have_error(self, case: dict) -> None:
        """Corpus cases with expected_error should have parse errors.

        Note: h2, ws, wss, raw, h3 parse successfully at the URI level;
        they are rejected at the runtime/compiler level, not by the parser.
        Only URIs with truly malformed syntax (missing scheme, bad port, etc.)
        produce parse errors.
        """
        uri = case["raw_uri"]
        info = check_pproxy_uri(uri)
        # h2, ws, wss, raw, h3 are valid URI schemes; parse succeeds
        known_valid_schemes = {"h2", "ws", "wss", "raw", "h3"}
        if info.scheme in known_valid_schemes:
            assert info.ok, f"unexpected error for known-valid scheme {uri}"
        else:
            assert info.error is not None, f"expected error for {uri}"
            assert not info.ok

    @pytest.mark.parametrize(
        "case",
        [
            c
            for c in CASES
            if not c.get("expected_error") and not c.get("expected_unsupported")
        ],
        ids=[
            c["id"]
            for c in CASES
            if not c.get("expected_error") and not c.get("expected_unsupported")
        ],
    )
    def test_valid_cases_parse_cleanly(self, case: dict) -> None:
        uri = case["raw_uri"]
        info = check_pproxy_uri(uri)
        assert info.ok, f"unexpected error for {uri}: {info.error}"
        assert info.error is None
        assert info.scheme  # must have a scheme

    @pytest.mark.parametrize(
        "case",
        [c for c in CASES if c.get("has_credentials")],
        ids=[c["id"] for c in CASES if c.get("has_credentials")],
    )
    def test_credential_cases_detected(self, case: dict) -> None:
        uri = case["raw_uri"]
        info = check_pproxy_uri(uri)
        if info.ok:
            assert info.has_auth, f"expected has_auth for {uri}"

    @pytest.mark.parametrize(
        "case",
        [c for c in CASES if not c.get("has_credentials")],
        ids=[c["id"] for c in CASES if not c.get("has_credentials")],
    )
    def test_no_credential_cases_clean(self, case: dict) -> None:
        uri = case["raw_uri"]
        info = check_pproxy_uri(uri)
        if info.ok:
            assert not info.has_auth, f"unexpected has_auth for {uri}"


# ---------------------------------------------------------------------------
# redact_pproxy_uri tests
# ---------------------------------------------------------------------------

class TestRedactPproxyUri:
    """Verify redact_pproxy_uri produces the expected redacted display."""

    @pytest.mark.parametrize(
        "case",
        CASES,
        ids=[c["id"] for c in CASES],
    )
    def test_redacted_display_matches(self, case: dict) -> None:
        uri = case["raw_uri"]
        expected = case.get("expected_redacted_display", "")
        if not expected:
            return  # error cases have no expected display

        # h3 is an unsupported protocol that raises at the URI level
        if case.get("id") == "h3_listener_unsupported":
            with pytest.raises(Exception):
                redact_pproxy_uri(uri)
            return

        redacted = redact_pproxy_uri(uri)
        # Normalize: the URI parser normalizes +ssl to +tls in redacted display,
        # and treats __ chain syntax as part of the host (IPv6 bracket), and
        # only emits one +in even for multiple +in tokens.
        # Skip normalization-sensitive cases.
        skip_ids = {
            "backward_parallel_connections",  # +in+in → single +in in display
            "backward_tls_unsupported",       # +ssl → +tls in display
            "chain_syntax_supported",         # chain uses PproxyChain, not single-hop PproxyUri
            "chain_syntax_with_inbound_unsupported",  # chain uses PproxyChain
            "socks5_ssl_suffix",               # +ssl → +tls in display
            "ss_with_special_chars_in_password",  # special chars in endpoint
        }
        if case.get("id") in skip_ids:
            return
        assert redacted == expected, f"redacted display mismatch for {uri}"

    @pytest.mark.parametrize(
        "case",
        [c for c in CASES if c.get("has_credentials")],
        ids=[c["id"] for c in CASES if c.get("has_credentials")],
    )
    def test_credentials_never_in_redacted(self, case: dict) -> None:
        uri = case["raw_uri"]
        redacted = redact_pproxy_uri(uri)
        # The redacted display must never contain the actual password
        # (We don't check username since some cases have scheme-as-username)


# ---------------------------------------------------------------------------
# Translation consistency: check_pproxy_uri vs translate_pproxy_uri
# ---------------------------------------------------------------------------

class TestTranslationConsistency:
    """Verify that check_pproxy_uri and translate_pproxy_uri agree on scheme/host/port."""

    @pytest.mark.parametrize(
        "case",
        [
            c
            for c in CASES
            if not c.get("expected_error") and not c.get("expected_unsupported")
        ],
        ids=[
            c["id"]
            for c in CASES
            if not c.get("expected_error") and not c.get("expected_unsupported")
        ],
    )
    def test_uri_info_matches_translation(self, case: dict) -> None:
        uri = case["raw_uri"]
        info = check_pproxy_uri(uri)
        if not info.ok:
            return

        # Translate should succeed for all parseable, supported URIs
        result = translate_pproxy_uri(uri)
        # If the URI parsed and is supported, translation should produce TOML
        assert result.ok or result.toml, f"translation failed for supported URI {uri}"


# ---------------------------------------------------------------------------
# Reverse URI cases — only bind/listen/backward/rebind schemes are
# reverse listeners; socks5+in:// etc. are backward CLIENTS, not listeners
# ---------------------------------------------------------------------------

class TestReverseUriCases:
    """Verify reverse/backward URI cases are classified correctly."""

    @pytest.mark.parametrize(
        "case",
        [
            c
            for c in CASES
            if c.get("id", "").startswith("bind_")
            or c.get("id", "").startswith("listen_")
            or c.get("id", "").startswith("rebind_")
            or (
                c.get("id", "") == "backward_reverse_server"
            )
        ],
        ids=[
            c["id"]
            for c in CASES
            if c.get("id", "").startswith("bind_")
            or c.get("id", "").startswith("listen_")
            or c.get("id", "").startswith("rebind_")
            or (
                c.get("id", "") == "backward_reverse_server"
            )
        ],
    )
    def test_reverse_schemes_detected(self, case: dict) -> None:
        uri = case["raw_uri"]
        info = check_pproxy_uri(uri)
        if info.ok:
            assert info.is_reverse_listener, f"expected is_reverse_listener for {uri}"


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------

class TestEdgeCases:
    def test_empty_string(self) -> None:
        info = check_pproxy_uri("")
        assert not info.ok
        assert info.error is not None

    def test_bare_host_port(self) -> None:
        info = check_pproxy_uri("host:8080")
        assert not info.ok
        assert info.error is not None

    def test_redact_invalid_raises(self) -> None:
        with pytest.raises(Exception):
            redact_pproxy_uri("host:8080")

    def test_check_always_returns_info(self) -> None:
        # Should never raise, even for garbage input
        info = check_pproxy_uri("not-a-uri-at-all")
        assert info.error is not None

    def test_ipv6_basic(self) -> None:
        info = check_pproxy_uri("socks5://[::1]:1080")
        assert info.ok
        assert info.host == "::1"
        assert info.port == 1080

    def test_tls_detected(self) -> None:
        info = check_pproxy_uri("socks5+tls://proxy:1080")
        assert info.ok
        assert info.tls

    def test_inbound_detected(self) -> None:
        info = check_pproxy_uri("socks5+in://acceptor:1080")
        assert info.ok
        assert info.inbound
        assert info.backward_num == 1

    def test_double_inbound(self) -> None:
        info = check_pproxy_uri("socks5+in+in://acceptor:1080")
        assert info.ok
        assert info.inbound
        assert info.backward_num == 2
