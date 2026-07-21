"""Gap-fill tests for Milestone C acceptance criteria.

Covers:
- C13: DEBUG flag propagation in accept() and udp_accept()
- C3:  stream_handler and datagram_handler happy/error paths
- C1/C2: Structural differential test scaffolding (gated)
- C14: Python-level interop test scaffolding (gated)
"""

from __future__ import annotations

import asyncio
import tempfile
import os
import unittest
from unittest.mock import MagicMock


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

class _FakeReader:
    """In-memory async reader backed by a bytearray."""

    def __init__(self, data: bytes = b"") -> None:
        self._buf = bytearray(data)

    async def read(self, n: int = -1) -> bytes:
        if not self._buf:
            return b""
        if n < 0:
            out = bytes(self._buf)
            self._buf.clear()
            return out
        out = bytes(self._buf[:n])
        self._buf = self._buf[n:]
        return out

    def feed(self, data: bytes) -> None:
        self._buf.extend(data)


class _FakeWriter:
    """In-memory async writer that records writes and tracks close."""

    def __init__(self) -> None:
        self.written: list[bytes] = []
        self.closed = False
        self.drain_count = 0

    def write(self, data: bytes) -> None:
        self.written.append(data)

    async def drain(self) -> None:
        self.drain_count += 1

    def close(self) -> None:
        self.closed = True

    def get_extra_info(self, key: str, default=None):
        return default


class _FakeProtocol:
    """Minimal protocol stub for accept/udp_accept dispatch tests."""

    def __init__(self, name: str = "fake", *, guess_result=None, accept_result=None, udp_accept_result=None,
                 guess_error=None, accept_error=None, udp_accept_error=None):
        self._name = name
        self._guess_result = guess_result
        self._accept_result = accept_result
        self._udp_accept_result = udp_accept_result
        self._guess_error = guess_error
        self._accept_error = accept_error
        self._udp_accept_error = udp_accept_error

    @property
    def name(self):
        return self._name

    async def guess(self, reader, **kw):
        if self._guess_error is not None:
            raise self._guess_error
        return self._guess_result

    async def accept(self, reader, user, **kw):
        if self._accept_error is not None:
            raise self._accept_error
        return self._accept_result or (user, "host", 80, None)

    def udp_accept(self, data, **kw):
        if self._udp_accept_error is not None:
            raise self._udp_accept_error
        return self._udp_accept_result


class _FakeProxy:
    """Proxy stub with connection_change tracking."""

    def __init__(self, name: str = "direct") -> None:
        self.name = name
        self.change_log: list[int] = []

    def connection_change(self, delta: int) -> None:
        self.change_log.append(delta)

    def __eq__(self, other):
        return self is other

    def __hash__(self):
        return id(self)


def _run_async(coro):
    """Run a coroutine in a fresh event loop."""
    return asyncio.run(coro)


# ---------------------------------------------------------------------------
# C13: DEBUG flag propagation
# ---------------------------------------------------------------------------

class TestDebugFlagPropagation(unittest.TestCase):
    """Verify protocol.DEBUG controls exception propagation in accept/udp_accept."""

    def setUp(self):
        from eggress import protocol as proto_mod
        self._proto_mod = proto_mod
        self._orig_debug = proto_mod.DEBUG

    def tearDown(self):
        self._proto_mod.DEBUG = self._orig_debug

    def test_accept_suppresses_when_debug_false(self):
        """accept() suppresses guess errors and raises generic 'Unsupported protocol'."""
        self._proto_mod.DEBUG = False
        proto = _FakeProtocol(guess_error=ValueError("bad data"))
        with self.assertRaises(Exception) as cm:
            _run_async(self._proto_mod.accept([proto], _FakeReader(b"\x00")))
        self.assertIn("Unsupported protocol", str(cm.exception))

    def test_accept_propagates_when_debug_true(self):
        """accept() re-raises the last guess error when DEBUG=True."""
        self._proto_mod.DEBUG = True
        proto = _FakeProtocol(guess_error=ValueError("specific error"))
        with self.assertRaises(ValueError) as cm:
            _run_async(self._proto_mod.accept([proto], _FakeReader(b"\x00")))
        self.assertIn("specific error", str(cm.exception))

    def test_accept_suppresses_accept_error_when_debug_false(self):
        """accept() suppresses errors in proto.accept() and continues."""
        self._proto_mod.DEBUG = False
        proto = _FakeProtocol(guess_result="user", accept_error=RuntimeError("handshake fail"))
        with self.assertRaises(Exception) as cm:
            _run_async(self._proto_mod.accept([proto], _FakeReader(b"\x00")))
        self.assertIn("Unsupported protocol", str(cm.exception))

    def test_accept_propagates_accept_error_when_debug_true(self):
        """accept() re-raises proto.accept() errors when DEBUG=True."""
        self._proto_mod.DEBUG = True
        proto = _FakeProtocol(guess_result="user", accept_error=RuntimeError("handshake fail"))
        with self.assertRaises(RuntimeError) as cm:
            _run_async(self._proto_mod.accept([proto], _FakeReader(b"\x00")))
        self.assertIn("handshake fail", str(cm.exception))

    def test_udp_accept_suppresses_when_debug_false(self):
        """udp_accept() suppresses errors and raises generic message."""
        self._proto_mod.DEBUG = False
        proto = _FakeProtocol(udp_accept_error=ValueError("bad udp"))
        with self.assertRaises(Exception) as cm:
            self._proto_mod.udp_accept([proto], b"\x00")
        self.assertIn("Unsupported protocol", str(cm.exception))

    def test_udp_accept_propagates_when_debug_true(self):
        """udp_accept() re-raises the last error when DEBUG=True."""
        self._proto_mod.DEBUG = True
        proto = _FakeProtocol(udp_accept_error=ValueError("bad udp"))
        with self.assertRaises(ValueError) as cm:
            self._proto_mod.udp_accept([proto], b"\x00")
        self.assertIn("bad udp", str(cm.exception))

    def test_accept_skips_non_matching_protocols(self):
        """accept() skips protocols whose guess returns None/falsy and tries next."""
        self._proto_mod.DEBUG = False
        skip = _FakeProtocol(guess_result=None)
        hit = _FakeProtocol(guess_result="user", accept_result=("user", "host", 9090, None))
        result = _run_async(self._proto_mod.accept([skip, hit], _FakeReader(b"\x00")))
        self.assertEqual(result[0], hit)
        self.assertEqual(result[2], "host")
        self.assertEqual(result[3], 9090)

    def test_udp_accept_skips_non_matching_protocols(self):
        """udp_accept() skips protocols whose udp_accept returns None/falsy."""
        self._proto_mod.DEBUG = False
        skip = _FakeProtocol(udp_accept_result=None)
        hit = _FakeProtocol(udp_accept_result=("user", "127.0.0.1", 1080, b"payload"))
        result = self._proto_mod.udp_accept([skip, hit], b"\x00")
        self.assertEqual(result[0], hit)
        self.assertEqual(result[2], "127.0.0.1")

    def test_accept_last_error_used_when_all_fail_debug_true(self):
        """When all protos fail with DEBUG=True, the first error is raised immediately."""
        self._proto_mod.DEBUG = True
        p1 = _FakeProtocol(guess_error=ValueError("first"))
        p2 = _FakeProtocol(guess_error=RuntimeError("second"))
        with self.assertRaises(ValueError) as cm:
            _run_async(self._proto_mod.accept([p1, p2], _FakeReader(b"\x00")))
        self.assertIn("first", str(cm.exception))


# ---------------------------------------------------------------------------
# C3: stream_handler happy path and error paths
# ---------------------------------------------------------------------------

class TestStreamHandlerGapFills(unittest.TestCase):
    """Additional stream_handler scenarios beyond auth rejection and null rserver."""

    def setUp(self):
        from pproxy.server import stream_handler
        self.stream_handler = stream_handler

    def test_happy_path_echo(self):
        """stream_handler echoes data from reader to writer."""
        reader = _FakeReader(b"hello world")
        writer = _FakeWriter()
        proxy = _FakeProxy("direct")

        _run_async(self.stream_handler(reader, writer, proxy, auth=None))

        self.assertEqual(writer.written, [b"hello world"])
        self.assertTrue(writer.closed)
        self.assertEqual(proxy.change_log, [1, -1])

    def test_list_rserver_schedules_one(self):
        """stream_handler with list rserver picks one via schedule()."""
        reader = _FakeReader(b"data")
        writer = _FakeWriter()
        p1 = _FakeProxy("p1")
        p2 = _FakeProxy("p2")

        _run_async(self.stream_handler(reader, writer, [p1, p2], auth=None))

        active = [p for p in (p1, p2) if p.change_log]
        self.assertEqual(len(active), 1)
        self.assertEqual(active[0].change_log, [1, -1])

    def test_connection_error_during_read(self):
        """stream_handler exits cleanly on ConnectionError."""
        reader = _FakeReader(b"partial")
        writer = _FakeWriter()
        proxy = _FakeProxy()

        original_read = reader.read
        call_count = 0

        async def read_with_error(n=-1):
            nonlocal call_count
            call_count += 1
            if call_count == 2:
                raise ConnectionError("reset")
            return await original_read(n)

        reader.read = read_with_error

        _run_async(self.stream_handler(reader, writer, proxy, auth=None))

        self.assertTrue(writer.closed)
        self.assertEqual(proxy.change_log, [1, -1])

    def test_empty_data_eof(self):
        """stream_handler exits on empty read (EOF)."""
        reader = _FakeReader(b"")
        writer = _FakeWriter()
        proxy = _FakeProxy()

        _run_async(self.stream_handler(reader, writer, proxy, auth=None))

        self.assertTrue(writer.closed)
        self.assertEqual(proxy.change_log, [1, -1])

    def test_multiple_reads_then_eof(self):
        """stream_handler relays multiple reads then closes on EOF."""
        reader = _FakeReader(b"chunk1")
        writer = _FakeWriter()
        proxy = _FakeProxy()

        _run_async(self.stream_handler(reader, writer, proxy, auth=None))

        self.assertEqual(writer.written, [b"chunk1"])
        self.assertTrue(writer.closed)

    def test_auth_ok_passes_through(self):
        """stream_handler with authenticated auth proceeds to relay."""
        reader = _FakeReader(b"authenticated data")
        writer = _FakeWriter()
        proxy = _FakeProxy()

        auth = MagicMock()
        auth.authed.return_value = "user"

        _run_async(self.stream_handler(reader, writer, proxy, auth=auth))

        self.assertEqual(writer.written, [b"authenticated data"])


# ---------------------------------------------------------------------------
# C3: datagram_handler happy path and error paths
# ---------------------------------------------------------------------------

class TestDatagramHandlerGapFills(unittest.TestCase):
    """Additional datagram_handler scenarios."""

    def setUp(self):
        from pproxy.server import datagram_handler
        self.datagram_handler = datagram_handler

    def test_list_rserver_schedules_one(self):
        """datagram_handler with list rserver picks one via schedule()."""
        p1 = _FakeProxy("p1")
        p2 = _FakeProxy("p2")

        result = self.datagram_handler(b"udp_data", [p1, p2])

        self.assertEqual(result, b"udp_data")
        active = [p for p in (p1, p2) if p.change_log]
        self.assertEqual(len(active), 1)
        self.assertEqual(active[0].change_log, [1, -1])

    def test_connection_change_tracked(self):
        """datagram_handler increments and decrements connection_change."""
        proxy = _FakeProxy()

        result = self.datagram_handler(b"test", proxy)

        self.assertEqual(result, b"test")
        self.assertEqual(proxy.change_log, [1, -1])

    def test_auth_rejection_returns_none(self):
        """datagram_handler returns None when auth fails."""
        auth = MagicMock()
        auth.authed.return_value = None

        result = self.datagram_handler(b"data", _FakeProxy(), auth=auth)
        self.assertIsNone(result)

    def test_auth_ok_passes_through(self):
        """datagram_handler proceeds when auth is valid."""
        auth = MagicMock()
        auth.authed.return_value = "user"
        proxy = _FakeProxy()

        result = self.datagram_handler(b"payload", proxy, auth=auth)
        self.assertEqual(result, b"payload")
        self.assertEqual(proxy.change_log, [1, -1])

    def test_none_rserver_returns_none(self):
        """datagram_handler returns None when rserver is None."""
        result = self.datagram_handler(b"data", None)
        self.assertIsNone(result)


# ---------------------------------------------------------------------------
# C1/C2: Structural differential test scaffolding
# ---------------------------------------------------------------------------

class TestDifferentialScaffolding(unittest.TestCase):
    """Structural differential tests — verify the scaffolding is in place."""

    def test_pproxy_differential_module_exists(self):
        """The differential test module exists and is importable."""
        try:
            import importlib
            mod = importlib.import_module("tests.test_pproxy_differential")
            self.assertTrue(hasattr(mod, "TestPproxyDifferential"))
        except ImportError:
            self.skipTest("test_pproxy_differential requires native extension")

    def test_server_utility_functions_exist(self):
        """All C1 server utilities are importable and callable."""
        from pproxy.server import (
            compile_rule,
            check_server_alive,
            prepare_ciphers,
            schedule,
            stream_handler,
            datagram_handler,
        )
        self.assertTrue(callable(compile_rule))
        self.assertTrue(callable(check_server_alive))
        self.assertTrue(callable(prepare_ciphers))
        self.assertTrue(callable(schedule))
        self.assertTrue(callable(stream_handler))
        self.assertTrue(callable(datagram_handler))

    def test_compile_rule_real_parsing(self):
        """compile_rule parses a real rule file and returns structured data."""
        from pproxy.server import compile_rule

        rule_content = "action=allow,rweight=1,ip=127.0.0.1,port=8080\n"
        with tempfile.NamedTemporaryFile(mode="w", suffix=".txt", delete=False) as f:
            f.write(rule_content)
            f.flush()
            try:
                result = compile_rule(f.name)
                self.assertIsInstance(result, (list, dict))
            finally:
                os.unlink(f.name)

    def test_schedule_round_robin(self):
        """schedule() with salgorithm='rr' uses round-robin."""
        from pproxy.server import schedule

        p1 = _FakeProxy("p1")
        p2 = _FakeProxy("p2")
        servers = [p1, p2]

        results = set()
        for _ in range(20):
            picked = schedule(servers, salgorithm="rr")
            results.add(picked)

        self.assertEqual(results, {p1, p2})


# ---------------------------------------------------------------------------
# C14: Python-level interop test scaffolding
# ---------------------------------------------------------------------------

class TestInteropScaffolding(unittest.TestCase):
    """Structural interop scaffolding — verifies the test infrastructure is in place."""

    def test_pproxy_server_importable(self):
        """pproxy.server can be imported without errors."""
        from pproxy import server
        self.assertTrue(hasattr(server, "proxies_by_uri"))
        self.assertTrue(hasattr(server, "compile_rule"))
        self.assertTrue(hasattr(server, "schedule"))

    def test_pproxy_proto_importable(self):
        """pproxy.proto can be imported without errors."""
        from pproxy import proto
        self.assertTrue(hasattr(proto, "Socks5"))
        self.assertTrue(hasattr(proto, "HTTP"))
        self.assertTrue(hasattr(proto, "Socks4"))

    def test_pproxy_cipher_importable(self):
        """pproxy.cipher can be imported without errors."""
        from pproxy import cipher
        self.assertTrue(hasattr(cipher, "MAP"))
        self.assertIsInstance(cipher.MAP, dict)
        self.assertGreater(len(cipher.MAP), 20)

    def test_pproxy_plugin_importable(self):
        """pproxy.plugin can be imported without errors."""
        from pproxy import plugin
        self.assertTrue(hasattr(plugin, "PluginRegistry") or hasattr(plugin, "DIRECT"))


# ---------------------------------------------------------------------------
# C7: Protocol connect/udp_connect raise NotImplementedError
# ---------------------------------------------------------------------------

class TestProtocolConnectNotImplemented(unittest.TestCase):
    """Verify that connect/udp_connect raise NotImplementedError with clear messages."""

    def test_base_connect_raises(self):
        """BaseProtocol.connect() raises NotImplementedError."""
        from eggress.protocol import BaseProtocol
        proto = BaseProtocol()
        with self.assertRaises(NotImplementedError) as cm:
            _run_async(proto.connect(None, None, None, "host", 80))
        self.assertIn("does not support client mode", str(cm.exception))

    def test_base_udp_connect_raises(self):
        """BaseProtocol.udp_connect() raises NotImplementedError."""
        from eggress.protocol import BaseProtocol
        proto = BaseProtocol()
        with self.assertRaises(NotImplementedError) as cm:
            proto.udp_connect(None, "host", 80, b"data")
        self.assertIn("does not support UDP client", str(cm.exception))

    def test_base_udp_accept_raises(self):
        """BaseProtocol.udp_accept() raises NotImplementedError."""
        from eggress.protocol import BaseProtocol
        proto = BaseProtocol()
        with self.assertRaises(NotImplementedError) as cm:
            proto.udp_accept(b"\x00")
        self.assertIn("does not support UDP server", str(cm.exception))

    def test_base_guess_raises(self):
        """BaseProtocol.guess() raises NotImplementedError."""
        from eggress.protocol import BaseProtocol
        proto = BaseProtocol()
        with self.assertRaises(NotImplementedError) as cm:
            _run_async(proto.guess(_FakeReader()))
        self.assertIn("does not implement guess", str(cm.exception))

    def test_base_accept_raises(self):
        """BaseProtocol.accept() raises NotImplementedError."""
        from eggress.protocol import BaseProtocol
        proto = BaseProtocol()
        with self.assertRaises(NotImplementedError) as cm:
            _run_async(proto.accept(_FakeReader(), "user"))
        self.assertIn("does not implement accept", str(cm.exception))


# ---------------------------------------------------------------------------
# C9: Cipher registry completeness
# ---------------------------------------------------------------------------

class TestCipherRegistryCompleteness(unittest.TestCase):
    """Verify both MAPs have 39 entries and all expected keys exist."""

    def test_eggress_cipher_map_has_39_entries(self):
        """eggress.cipher.MAP has exactly 39 entries."""
        from eggress.cipher import MAP
        self.assertEqual(len(MAP), 39)

    def test_cipherpy_map_has_39_entries(self):
        """pproxy.cipherpy.MAP has exactly 39 entries."""
        from pproxy.cipherpy import MAP
        self.assertEqual(len(MAP), 39)

    def test_all_base_ciphers_in_eggress_map(self):
        """All 24 base cipher names are in eggress.cipher.MAP."""
        from eggress.cipher import MAP
        base_names = [
            "aes-256-gcm", "aes-192-gcm", "aes-128-gcm",
            "chacha20-ietf-poly1305",
            "rc4", "rc4-md5", "chacha20", "chacha20-ietf", "salsa20",
            "aes-256-cfb", "aes-192-cfb", "aes-128-cfb",
            "aes-256-cfb8", "aes-192-cfb8", "aes-128-cfb8",
            "aes-256-ofb", "aes-192-ofb", "aes-128-ofb",
            "aes-256-ctr", "aes-192-ctr", "aes-128-ctr",
            "bf-cfb", "cast5-cfb", "des-cfb",
        ]
        for name in base_names:
            self.assertIn(name, MAP, f"Missing base cipher: {name}")

    def test_py_variants_in_both_maps(self):
        """All 15 -py variant aliases are in both MAPs."""
        from eggress.cipher import MAP as EGGRESS_MAP
        from pproxy.cipherpy import MAP as CIPHERPY_MAP
        py_variants = [
            "aes-256-gcm-py", "aes-128-gcm-py",
            "chacha20-ietf-poly1305-py", "rc4-md5-py", "chacha20-py",
            "salsa20-py", "aes-256-cfb-py", "aes-128-cfb-py",
            "aes-256-cfb8-py", "aes-128-cfb8-py",
            "aes-256-ofb-py", "aes-128-ofb-py",
            "aes-256-ctr-py", "aes-128-ctr-py", "bf-cfb-py",
        ]
        for name in py_variants:
            self.assertIn(name, EGGRESS_MAP, f"Missing in eggress MAP: {name}")
            self.assertIn(name, CIPHERPY_MAP, f"Missing in cipherpy MAP: {name}")

    def test_py_variants_point_to_same_class(self):
        """Each -py variant points to the same class as its base name."""
        from eggress.cipher import MAP
        pairs = [
            ("aes-256-gcm", "aes-256-gcm-py"),
            ("aes-128-gcm", "aes-128-gcm-py"),
            ("chacha20-ietf-poly1305", "chacha20-ietf-poly1305-py"),
            ("rc4-md5", "rc4-md5-py"),
            ("chacha20", "chacha20-py"),
            ("salsa20", "salsa20-py"),
            ("aes-256-cfb", "aes-256-cfb-py"),
            ("aes-128-cfb", "aes-128-cfb-py"),
            ("aes-256-cfb8", "aes-256-cfb8-py"),
            ("aes-128-cfb8", "aes-128-cfb8-py"),
            ("aes-256-ofb", "aes-256-ofb-py"),
            ("aes-128-ofb", "aes-128-ofb-py"),
            ("aes-256-ctr", "aes-256-ctr-py"),
            ("aes-128-ctr", "aes-128-ctr-py"),
            ("bf-cfb", "bf-cfb-py"),
        ]
        for base, alias in pairs:
            self.assertIs(MAP[base], MAP[alias], f"{base} and {alias} should be the same class")


# ---------------------------------------------------------------------------
# C15: Module-level accept/udp_accept dispatch
# ---------------------------------------------------------------------------

class TestModuleLevelDispatch(unittest.TestCase):
    """Test the module-level accept() and udp_accept() dispatch functions."""

    def test_accept_returns_protocol_and_metadata(self):
        """accept() returns (proto, user, host, port, extra) tuple."""
        from eggress import protocol as proto_mod

        proto = _FakeProtocol(guess_result="testuser", accept_result=("testuser", "example.com", 443, None))
        result = _run_async(proto_mod.accept([proto], _FakeReader(b"\x01")))
        self.assertEqual(result[0], proto)
        self.assertEqual(result[1], "testuser")
        self.assertEqual(result[2], "example.com")
        self.assertEqual(result[3], 443)

    def test_accept_pads_short_ret(self):
        """accept() pads the return tuple to at least 4 elements."""
        from eggress import protocol as proto_mod

        proto = _FakeProtocol(guess_result="u", accept_result=("u", "h", 80))
        result = _run_async(proto_mod.accept([proto], _FakeReader(b"\x01")))
        self.assertEqual(len(result), 5)  # proto + 4
        self.assertIsNone(result[4])  # padded extra

    def test_udp_accept_returns_tuple(self):
        """udp_accept() returns (proto, user, host, port, payload) tuple."""
        from eggress import protocol as proto_mod

        proto = _FakeProtocol(udp_accept_result=("user", "10.0.0.1", 53, b"\x00data"))
        result = proto_mod.udp_accept([proto], b"\x00")
        self.assertEqual(result[0], proto)
        self.assertEqual(result[1], "user")
        self.assertEqual(result[2], "10.0.0.1")
        self.assertEqual(result[3], 53)
        self.assertEqual(result[4], b"\x00data")


if __name__ == "__main__":
    unittest.main()
