"""Paired differential tests for protocol wire-level behavior.

These tests compare the pproxy oracle protocol wire encoding against
the eggress candidate for SOCKS5, HTTP, and SOCKS4 protocols.  They
consume pre-generated protocol_wire observations from the observation
directories.

Tier: 2 (paired API oracle)
Gate: --oracle-observations-dir and --candidate-observations-dir required
"""

import json
import os

import pytest

from .conftest import compare_observations


def load_observation(obs_dir, rid, side):
    """Load an observation JSON file."""
    filename = f"{rid.replace('.', '_')}_{side}.json"
    filepath = os.path.join(str(obs_dir), filename)
    if not os.path.exists(filepath):
        return {"exists": False, "error": f"Observation file not found: {filepath}"}
    with open(filepath) as fh:
        return json.load(fh)


# Protocol wire observations to compare
WIRE_RECORDS = [
    ("protocol.wire.socks5.greeting", "SOCKS5 greeting encoding"),
    ("protocol.wire.socks5.connect_ipv4", "SOCKS5 CONNECT IPv4"),
    ("protocol.wire.socks5.connect_domain", "SOCKS5 CONNECT domain"),
    ("protocol.wire.socks5.connect_ipv6", "SOCKS5 CONNECT IPv6"),
    ("protocol.wire.socks5.auth_method_selection", "SOCKS5 auth method selection"),
    ("protocol.wire.http.connect_request", "HTTP CONNECT request"),
    ("protocol.wire.http.absolute_form", "HTTP absolute-form URI"),
    ("protocol.wire.http.origin_form", "HTTP origin-form URI"),
    ("protocol.wire.http.header_removal", "HTTP proxy header removal"),
    ("protocol.wire.socks4.connect_ipv4", "SOCKS4 CONNECT IPv4"),
    ("protocol.wire.socks4.connect_domain", "SOCKS4a CONNECT domain"),
]


@pytest.mark.differential
class TestProtocolWireDifferential:
    """Paired tests for protocol wire-level encoding."""

    @pytest.mark.parametrize("rid,description", WIRE_RECORDS)
    def test_wire_encoding_matches(self, rid, description, require_obs_dirs):
        """Verify protocol wire encoding matches oracle."""
        oracle_dir, candidate_dir = require_obs_dirs

        oracle_obs = load_observation(oracle_dir, rid, "oracle")
        candidate_obs = load_observation(candidate_dir, rid, "candidate")

        if oracle_obs.get("error") and "not found" in oracle_obs["error"]:
            pytest.skip(f"No oracle observation for {rid}")
        if candidate_obs.get("error") and "not found" in candidate_obs["error"]:
            pytest.skip(f"No candidate observation for {rid}")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"Wire encoding mismatch for {description}: "
            f"{[c for c in result['comparisons'] if not c['match']]}"
        )


@pytest.mark.differential
class TestSOCKS5WireEncoding:
    """Specific SOCKS5 wire-level comparison tests."""

    def test_socks5_greeting_version(self, require_obs_dirs):
        """SOCKS5 greeting uses version 5."""
        oracle_dir, candidate_dir = require_obs_dirs
        oracle_obs = load_observation(oracle_dir, "protocol.wire.socks5.greeting", "oracle")
        candidate_obs = load_observation(candidate_dir, "protocol.wire.socks5.greeting", "candidate")

        if oracle_obs.get("exists") is False or candidate_obs.get("exists") is False:
            pytest.skip("Wire observation not available")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"SOCKS5 greeting mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )

    def test_socks5_connect_reply_format(self, require_obs_dirs):
        """SOCKS5 CONNECT reply has correct format."""
        oracle_dir, candidate_dir = require_obs_dirs
        oracle_obs = load_observation(oracle_dir, "protocol.wire.socks5.connect_ipv4", "oracle")
        candidate_obs = load_observation(candidate_dir, "protocol.wire.socks5.connect_ipv4", "candidate")

        if oracle_obs.get("exists") is False or candidate_obs.get("exists") is False:
            pytest.skip("Wire observation not available")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"SOCKS5 CONNECT reply mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )


@pytest.mark.differential
class TestHTTPWireEncoding:
    """Specific HTTP wire-level comparison tests."""

    def test_http_connect_method(self, require_obs_dirs):
        """HTTP CONNECT request uses correct method."""
        oracle_dir, candidate_dir = require_obs_dirs
        oracle_obs = load_observation(oracle_dir, "protocol.wire.http.connect_request", "oracle")
        candidate_obs = load_observation(candidate_dir, "protocol.wire.http.connect_request", "candidate")

        if oracle_obs.get("exists") is False or candidate_obs.get("exists") is False:
            pytest.skip("Wire observation not available")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"HTTP CONNECT mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )

    def test_http_proxy_header_removal(self, require_obs_dirs):
        """HTTP proxy-only headers are removed before forwarding."""
        oracle_dir, candidate_dir = require_obs_dirs
        oracle_obs = load_observation(oracle_dir, "protocol.wire.http.header_removal", "oracle")
        candidate_obs = load_observation(candidate_dir, "protocol.wire.http.header_removal", "candidate")

        if oracle_obs.get("exists") is False or candidate_obs.get("exists") is False:
            pytest.skip("Wire observation not available")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"HTTP header removal mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )


@pytest.mark.differential
class TestSOCKS4WireEncoding:
    """Specific SOCKS4 wire-level comparison tests."""

    def test_socks4_connect_format(self, require_obs_dirs):
        """SOCKS4 CONNECT request has correct format."""
        oracle_dir, candidate_dir = require_obs_dirs
        oracle_obs = load_observation(oracle_dir, "protocol.wire.socks4.connect_ipv4", "oracle")
        candidate_obs = load_observation(candidate_dir, "protocol.wire.socks4.connect_ipv4", "candidate")

        if oracle_obs.get("exists") is False or candidate_obs.get("exists") is False:
            pytest.skip("Wire observation not available")

        result = compare_observations(oracle_obs, candidate_obs)
        assert result["all_match"], (
            f"SOCKS4 CONNECT mismatch: {[c for c in result['comparisons'] if not c['match']]}"
        )
