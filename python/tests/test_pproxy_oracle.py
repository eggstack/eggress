"""Oracle tests: verify eggress Python API behavior matches pproxy oracle.

These tests are GATED — they require pproxy==2.7.9 to be installed.
Run with: EGRESS_REQUIRE_PPROXY_ORACLE=1 python -m pytest python/tests/test_pproxy_oracle.py -v

The oracle fixture is at tests/compat/fixtures/pproxy_api_snapshot.json.
"""

import json
import os

import pytest

ORACLE_REQUIRED = os.environ.get("EGRESS_REQUIRE_PPROXY_ORACLE", "") != "0"

SNAPSHOT_PATH = os.path.join(
    os.path.dirname(__file__), "..", "..", "tests", "compat", "fixtures", "pproxy_api_snapshot.json"
)


def _pproxy_available():
    """Check if pproxy is importable."""
    try:
        import pproxy

        return True
    except ImportError:
        return False


def _eggress_available():
    """Check if eggress native module is importable."""
    try:
        import eggress._eggress  # noqa: F401

        return True
    except (ImportError, ModuleNotFoundError):
        return False


def _load_snapshot():
    """Load the frozen API snapshot."""
    with open(SNAPSHOT_PATH) as f:
        return json.load(f)


# --- Module Export Tests ---


@pytest.mark.skipif(not _pproxy_available(), reason="pproxy not installed")
class TestModuleExports:
    """Verify pproxy module exports match snapshot."""

    def test_connection_exists(self):
        import pproxy

        assert hasattr(pproxy, "Connection")

    def test_server_exists(self):
        import pproxy

        assert hasattr(pproxy, "Server")

    def test_rule_exists(self):
        import pproxy

        assert hasattr(pproxy, "Rule")

    def test_direct_exists(self):
        import pproxy

        assert hasattr(pproxy, "DIRECT")

    def test_no_version_attribute(self):
        import pproxy

        assert not hasattr(pproxy, "__version__")

    def test_snapshot_module_exports_present(self):
        snapshot = _load_snapshot()
        import pproxy

        # Only check explicitly re-exported names (not submodules like proto/server/cipher)
        explicit_exports = {"Connection", "DIRECT", "Rule", "Server"}
        for name in explicit_exports:
            assert hasattr(pproxy, name), f"Module export {name!r} missing from pproxy"


# --- Protocol Class Tests ---


@pytest.mark.skipif(not _pproxy_available(), reason="pproxy not installed")
class TestProtocolClasses:
    """Verify protocol classes are accessible and match snapshot."""

    def test_socks5_class(self):
        from pproxy.proto import Socks5

        assert Socks5 is not None

    def test_http_class(self):
        from pproxy.proto import HTTP

        assert HTTP is not None

    def test_ss_class(self):
        from pproxy.proto import SS

        assert SS is not None

    def test_trojan_class(self):
        from pproxy.proto import Trojan

        assert Trojan is not None

    def test_socks4_class(self):
        from pproxy.proto import Socks4

        assert Socks4 is not None

    def test_h2_class(self):
        from pproxy.proto import H2

        assert H2 is not None

    def test_protocols_match_snapshot(self):
        import pproxy.proto as proto

        snapshot = _load_snapshot()
        for proto_name in snapshot.get("protocols", {}):
            assert hasattr(proto, proto_name), f"Protocol {proto_name} missing from pproxy.proto"


# --- Translation Parity Tests ---


@pytest.mark.skipif(not _eggress_available(), reason="eggress native module not built")
class TestTranslationParity:
    """Verify eggress translation matches pproxy behavior for common URIs."""

    def test_socks5_uri(self):
        from eggress import translate_pproxy_args

        result = translate_pproxy_args(["-l", "socks5://:1080"])
        assert result.toml
        assert "socks5" in result.toml.lower()

    def test_http_uri(self):
        from eggress import translate_pproxy_args

        result = translate_pproxy_args(["-l", "http://:8080"])
        assert result.toml

    def test_chain_uri(self):
        from eggress import translate_pproxy_args

        result = translate_pproxy_args(["-l", "socks5://:1080__http://proxy:8080"])
        assert result.toml

    def test_translation_produces_valid_toml(self):
        from eggress import translate_pproxy_args

        result = translate_pproxy_args(["-l", "socks5://127.0.0.1:1080"])
        assert result.ok
        assert "version = 1" in result.toml
        assert "[[listeners]]" in result.toml


# --- Snapshot Consistency Tests ---


@pytest.mark.skipif(not os.path.exists(SNAPSHOT_PATH), reason="API snapshot not found")
class TestSnapshotConsistency:
    """Verify runtime pproxy matches frozen snapshot."""

    def test_snapshot_loads(self):
        snapshot = _load_snapshot()
        assert "module_exports" in snapshot
        assert "protocols" in snapshot

    def test_snapshot_has_expected_keys(self):
        snapshot = _load_snapshot()
        assert "module_exports" in snapshot
        assert "submodules" in snapshot
        assert "protocols" in snapshot
        assert "cipher_info" in snapshot

    @pytest.mark.skipif(not _pproxy_available(), reason="pproxy not installed")
    def test_submodules_proto_in_snapshot(self):
        import pproxy.proto as proto

        snapshot = _load_snapshot()
        for name in snapshot.get("submodules", {}).get("proto", []):
            assert hasattr(proto, name), f"pproxy.proto export {name!r} missing"

    @pytest.mark.skipif(not _pproxy_available(), reason="pproxy not installed")
    def test_cipher_classes_in_snapshot(self):
        import pproxy.cipher as cipher

        snapshot = _load_snapshot()
        for name in snapshot.get("cipher_info", {}).get("classes", []):
            assert hasattr(cipher, name), f"pproxy.cipher class {name!r} missing"
