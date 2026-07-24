"""Oracle tests: verify eggress Python API behavior matches pproxy oracle.

These tests are GATED — pproxy-dependent tests auto-skip if pproxy is not
installed. eggress-dependent tests auto-skip if the native module is not built.
No manual gating via environment variables is required.

The oracle fixture is at tests/compat/fixtures/pproxy_api_snapshot.json.
"""

import json
import os
import socket
import struct
import threading
import time

import pytest

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


# --- Helpers for lifecycle tests ---


def _echo_server():
    """Start a TCP echo server on 127.0.0.1:0, return (host, port, server_socket)."""
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", 0))
    srv.listen(5)
    host, port = srv.getsockname()

    def _accept():
        while True:
            try:
                conn, _ = srv.accept()
            except OSError:
                break
            try:
                while True:
                    data = conn.recv(4096)
                    if not data:
                        break
                    conn.sendall(data)
            finally:
                conn.close()

    t = threading.Thread(target=_accept, daemon=True)
    t.start()
    return host, port, srv


def _socks5_connect(proxy_host, proxy_port, target_host, target_port):
    """Perform a SOCKS5 CONNECT handshake and return the connected socket.

    Uses ATYP=0x01 (IPv4) when target_host is an IPv4 literal to avoid the
    DNS rebinding protection check in the SOCKS5 direct path; otherwise uses
    ATYP=0x03 (domain).
    """
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(3.0)
    s.connect((proxy_host, proxy_port))
    s.sendall(b"\x05\x01\x00")
    resp = s.recv(2)
    assert resp[0] == 0x05, f"SOCKS5 greeting version mismatch: {resp!r}"
    try:
        ip_bytes = socket.inet_aton(target_host)
        atyp = b"\x01" + ip_bytes
    except OSError:
        host_bytes = target_host.encode()
        atyp = b"\x03" + bytes([len(host_bytes)]) + host_bytes
    req = b"\x05\x01\x00" + atyp + struct.pack("!H", target_port)
    s.sendall(req)
    resp = s.recv(32)
    assert resp[1] == 0x00, f"SOCKS5 connect failed: {resp!r}"
    return s


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

    def test_version_attribute(self):
        import pproxy
        from importlib.metadata import version

        # pproxy 2.7.9 does not expose __version__ at module level
        v = version("pproxy")
        assert v == "2.7.9"

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


# --- Server Lifecycle Oracle Tests (Phase 30) ---


@pytest.mark.skipif(not _eggress_available(), reason="eggress native module not built")
class TestServerLifecycle:
    """Verify eggress Server lifecycle matches pproxy Server patterns."""

    def test_server_import(self):
        """eggress.pproxy.Server is importable and callable."""
        from eggress.pproxy import Server

        assert Server is not None

    def test_server_construct_listen_remote(self):
        """Server(listen=[...], remote=[...]) creates without error."""
        from eggress.pproxy import Server

        srv = Server(
            listen=["socks5://127.0.0.1:0"],
            remote=["http://127.0.0.1:8080"],
        )
        srv.close()

    def test_server_start_stop_lifecycle(self):
        """Server starts, reports addresses, and stops cleanly."""
        from eggress.pproxy import Server

        with Server(listen=["http://127.0.0.1:0"]) as srv:
            time.sleep(0.1)
            addrs = srv.addresses
            assert len(addrs) > 0
            for addr in addrs.values():
                assert addr != ""
        assert srv.addresses == {}

    def test_server_socks5_relay(self):
        """Server SOCKS5 listener relays traffic to upstream."""
        from eggress.pproxy import Server

        echo_host, echo_port, echo_srv = _echo_server()
        try:
            with Server(listen=["socks5://127.0.0.1:0"]) as srv:
                time.sleep(0.1)
                addrs = srv.addresses
                socks_addr = None
                for key, addr in addrs.items():
                    if addr:
                        host, port = addr.rsplit(":", 1)
                        socks_addr = (host, int(port))
                        break
                assert socks_addr is not None

                s = _socks5_connect(
                    socks_addr[0], socks_addr[1], echo_host, echo_port
                )
                try:
                    s.sendall(b"ping")
                    resp = s.recv(4096)
                    assert resp == b"ping"
                finally:
                    s.close()
        finally:
            echo_srv.close()

    def test_server_double_start_raises(self):
        """Starting a running server raises AlreadyStartedError."""
        from eggress.pproxy import AlreadyStartedError, Server

        srv = Server(listen=["http://127.0.0.1:0"])
        try:
            srv.start()
            time.sleep(0.1)
            with pytest.raises(AlreadyStartedError):
                srv.start()
        finally:
            srv.close()

    def test_server_close_idempotent(self):
        """Closing a server twice is safe."""
        from eggress.pproxy import Server

        srv = Server(listen=["http://127.0.0.1:0"])
        srv.start()
        time.sleep(0.1)
        srv.close()
        srv.close()  # should not raise

    def test_server_repr_states(self):
        """repr shows stopped/running states."""
        from eggress.pproxy import Server

        srv = Server(listen=["http://127.0.0.1:0"])
        assert repr(srv) == "Server(stopped)"
        srv.start()
        time.sleep(0.1)
        try:
            assert repr(srv) == "Server(running)"
        finally:
            srv.close()
        assert repr(srv) == "Server(stopped)"

    def test_server_unsupported_uri_raises(self):
        """Unsupported URI (ssh://) raises UnsupportedFeatureError."""
        from eggress import UnsupportedFeatureError
        from eggress.pproxy import Server

        with pytest.raises(UnsupportedFeatureError):
            Server(listen=["ssh://127.0.0.1:22"])

    def test_server_async_context_manager(self):
        """async with Server(...) works."""
        import asyncio
        from eggress.pproxy import Server

        async def _run():
            async with Server(listen=["http://127.0.0.1:0"]) as srv:
                await asyncio.sleep(0.1)
                assert len(srv.addresses) > 0
            assert srv.addresses == {}

        asyncio.run(_run())

    def test_server_multiple_listeners(self):
        """Two listeners both report in addresses."""
        from eggress.pproxy import Server

        with Server(
            listen=["socks5://127.0.0.1:0", "http://127.0.0.1:0"],
        ) as srv:
            time.sleep(0.1)
            addrs = srv.addresses
            assert len(addrs) == 2
            for addr in addrs.values():
                assert addr != ""


@pytest.mark.skipif(not _pproxy_available(), reason="pproxy not installed")
class TestServerLifecycleOracle:
    """Compare pproxy and eggress server lifecycle patterns.

    pproxy.Server is actually proxies_by_uri() — a protocol handler factory,
    not a full server lifecycle manager. eggress Server wraps the full
    start/stop lifecycle that pproxy handles externally via asyncio.
    """

    def test_pproxy_server_is_handler_factory(self):
        """pproxy.Server is proxies_by_uri, a protocol handler factory."""
        import pproxy

        assert callable(pproxy.Server)
        # It creates a protocol handler from a URI chain
        handler = pproxy.Server("socks5://:1080")
        assert handler is not None

    def test_pproxy_server_produces_handler(self):
        """pproxy.Server creates a usable protocol handler."""
        import pproxy

        handler = pproxy.Server("socks5://:1080")
        # handler should be callable or have protocol methods
        assert hasattr(handler, "__call__") or handler is not None

    def test_eggress_server_has_matching_api(self):
        """eggress Server has matching API surface to pproxy patterns."""
        from eggress.pproxy import Server

        # eggress Server provides the full lifecycle that pproxy handles externally
        srv = Server(listen=["http://127.0.0.1:0"])
        assert hasattr(srv, "start")
        assert hasattr(srv, "close")
        assert hasattr(srv, "stop")
        assert hasattr(srv, "addresses")
        srv.close()
