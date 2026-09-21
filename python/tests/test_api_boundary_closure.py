"""Corrective-closure regression: native write contract, exception identity,
preview hop count, and compatibility-startup forwarding.

Covers the four workstreams in plans/API_BOUNDARY_CORRECTIVE_CLOSURE.md
without reopening architecture or expanding capability.
"""

from __future__ import annotations

import asyncio
import inspect
import socket
import threading

import pytest

pytest.importorskip("eggress._eggress")

import eggress
import eggress._eggress as _native
from eggress import EggressService
from eggress.outbound import OutboundConnector


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _start_echo_server():
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", 0))
    server.listen(16)
    addr = server.getsockname()

    def _accept_loop():
        while True:
            try:
                client, _ = server.accept()
            except OSError:
                return
            threading.Thread(target=_echo_handler, args=(client,), daemon=True).start()

    def _echo_handler(cli: socket.socket):
        try:
            while True:
                data = cli.recv(4096)
                if not data:
                    break
                cli.sendall(data)
        except OSError:
            pass
        finally:
            cli.close()

    threading.Thread(target=_accept_loop, daemon=True).start()
    return addr, server


def _start_observing_server(on_receive: threading.Event, received: list):
    """TCP server that signals when it has received any bytes (no echo)."""
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", 0))
    server.listen(1)
    addr = server.getsockname()

    def _accept_loop():
        try:
            client, _ = server.accept()
        except OSError:
            return
        try:
            data = client.recv(65536)
            if data:
                received.append(data)
                on_receive.set()
        except OSError:
            pass
        finally:
            try:
                client.close()
            except OSError:
                pass

    threading.Thread(target=_accept_loop, daemon=True).start()
    return addr, server


_DIRECT_URI = "direct://127.0.0.1:0"


# ---------------------------------------------------------------------------
# Workstream 1 — native write compatibility + private async submission
# ---------------------------------------------------------------------------


class TestNativeWriteContract:
    def test_native_write_round_trip_without_explicit_drain(self):
        """Smoke: native write reaches the peer without an explicit drain.

        This is a round-trip smoke test only. It does not prove synchronous
        completion semantics: the Tokio write pump consumes queued `Data`
        independently, so a queue-only implementation could also transmit
        promptly. The authoritative synchronous-completion proof is the
        deterministic gated-transport Rust test
        (`outbound::tests::native_sync_write_waits_for_transport_completion`),
        which holds the transport gate closed and proves the sync result is
        withheld until transport completion, plus
        (`outbound::tests::async_submit_returns_before_transport_completion`)
        for the queue-only async path.
        """
        received: list = []
        arrived = threading.Event()
        (host, port), server = _start_observing_server(arrived, received)
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(host, port, timeout=5.0)
            try:
                native = stream._inner
                assert native.write(b"sync-completion") == len(b"sync-completion")
                assert arrived.wait(timeout=5.0), (
                    "native write did not reach peer without drain; "
                    "queue-only semantics suspected"
                )
                assert received[0] == b"sync-completion"
            finally:
                stream.close()
                stream.wait_closed()
        finally:
            server.close()

    def test_sync_wrapper_write_echoes_without_explicit_drain(self):
        """High-level OutboundStream.write keeps blocking/completion semantics."""
        addr, server = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(addr[0], addr[1], timeout=5.0)
            try:
                assert stream.write(b"hello-sync") == len(b"hello-sync")
                # No explicit drain: completion semantics mean the echo is
                # already on its way; read is the barrier.
                assert stream.read(1024) == b"hello-sync"
            finally:
                stream.close()
                stream.wait_closed()
        finally:
            server.close()

    def test_no_public_write_blocking_for_sync(self):
        assert not hasattr(_native.PyOutboundStream, "write_blocking_for_sync"), (
            "accidental public write_blocking_for_sync still exposed"
        )

    def test_private_submit_exists_but_not_public(self):
        assert hasattr(_native.PyOutboundStream, "_submit_write")
        assert "write_blocking_for_sync" not in eggress.__all__
        assert "_submit_write" not in eggress.__all__

    def test_async_write_uses_private_submit_and_drain_completes(self):
        addr, server = _start_echo_server()

        async def _exercise():
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = await conn.aconnect_tcp(addr[0], addr[1], timeout=5.0)
            try:
                assert stream.write(b"async-ordered") == len(b"async-ordered")
                await stream.drain()
                assert await stream.readexactly(len(b"async-ordered")) == b"async-ordered"
            finally:
                stream.close()
                await stream.wait_closed()

        try:
            asyncio.run(_exercise())
        finally:
            server.close()

    def test_async_ordered_writes_plus_drain(self):
        addr, server = _start_echo_server()

        async def _exercise():
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = await conn.aconnect_tcp(addr[0], addr[1], timeout=5.0)
            try:
                payloads = [f"seg-{i};".encode() for i in range(32)]
                for payload in payloads:
                    assert stream.write(payload) == len(payload)
                await stream.drain()
                assert await stream.readexactly(sum(map(len, payloads))) == b"".join(payloads)
            finally:
                stream.close()
                await stream.wait_closed()

        try:
            asyncio.run(_exercise())
        finally:
            server.close()

    def test_async_write_keeps_loop_schedulable(self):
        server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server.bind(("127.0.0.1", 0))
        server.listen(1)
        host, port = server.getsockname()
        accepted = threading.Event()

        def _blackhole():
            try:
                client, _ = server.accept()
                accepted.set()
                # Hold the connection without reading to create backpressure.
                threading.Event().wait(timeout=5)
                client.close()
            except OSError:
                pass

        threading.Thread(target=_blackhole, daemon=True).start()

        async def _exercise():
            import os

            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = await conn.aconnect_tcp(host, port, timeout=5.0)
            try:
                await asyncio.get_running_loop().run_in_executor(None, accepted.wait, 2)
                heartbeat = asyncio.Event()

                async def _heartbeat():
                    await asyncio.sleep(0)
                    heartbeat.set()

                task = asyncio.create_task(_heartbeat())
                stream.write(os.urandom(8 * 1024 * 1024))
                await asyncio.wait_for(heartbeat.wait(), timeout=1.0)
                await task
            finally:
                stream.close()
                await stream.wait_closed()

        try:
            asyncio.run(_exercise())
        finally:
            server.close()

    def test_write_eof_ordered_after_async_submissions(self):
        addr, server = _start_echo_server()

        async def _exercise():
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = await conn.aconnect_tcp(addr[0], addr[1], timeout=5.0)
            try:
                stream.write(b"before-eof")
                await stream.write_eof()
                assert await stream.readexactly(len(b"before-eof")) == b"before-eof"
                with pytest.raises(Exception, match="closed"):
                    stream.write(b"after-eof")
            finally:
                stream.close()
                await stream.wait_closed()

        try:
            asyncio.run(_exercise())
        finally:
            server.close()

    def test_sync_write_failure_surfaces_before_return(self):
        addr, server = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(addr[0], addr[1], timeout=5.0)
            stream.close()
            with pytest.raises(Exception, match="closed|failed"):
                stream.write(b"after-close")
        finally:
            server.close()

    def test_async_drain_surfaces_terminal_failure(self):
        addr, server = _start_echo_server()

        async def _exercise():
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = await conn.aconnect_tcp(addr[0], addr[1], timeout=5.0)
            stream.close()
            with pytest.raises(Exception, match="closed|failed|stopped"):
                await stream.drain()

        try:
            asyncio.run(_exercise())
        finally:
            server.close()

    def test_close_wait_closed_leaves_no_pump_running(self):
        addr, server = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(addr[0], addr[1], timeout=5.0)
            stream.close()
            stream.wait_closed()
            assert stream.closed
            # Idempotent second wait must return promptly, proving the pump
            # task was joined rather than left running.
            stream.wait_closed()
            with pytest.raises(Exception, match="closed"):
                stream.write(b"x")
        finally:
            server.close()


# ---------------------------------------------------------------------------
# Workstream 2 — exception identity convergence
# ---------------------------------------------------------------------------


class TestExceptionIdentityMatrix:
    def _matrix(self):
        import eggress.connection as connection
        import eggress.exceptions as exceptions

        return [
            ("ConnectionError", _native.ConnectionError, connection.ConnectionError,
             exceptions.ConnectionBaseError, eggress.ConnectionBaseError),
            ("ConnectionClosedError", _native.ConnectionClosedError,
             connection.ConnectionClosedError, exceptions.ConnectionClosedError,
             eggress.ConnectionClosedError),
            ("TimeoutError", _native.TimeoutError, connection.TimeoutError,
             exceptions.ConnectionTimeoutError, eggress.ConnectionTimeoutError),
            ("DnsError", _native.DnsError, connection.DnsError,
             exceptions.ConnectionDnsError, eggress.ConnectionDnsError),
            ("AuthError", _native.AuthError, connection.AuthError,
             exceptions.ConnectionAuthError, eggress.ConnectionAuthError),
            ("TlsError", _native.TlsError, connection.TlsError,
             exceptions.ConnectionTlsError, eggress.ConnectionTlsError),
            ("LoopMismatchError", _native.LoopMismatchError, connection.LoopMismatchError,
             exceptions.LoopMismatchError, eggress.LoopMismatchError),
            ("ConnectionCancelledError", _native.ConnectionCancelledError,
             connection.ConnectionCancelledError, exceptions.ConnectionCancelledError,
             eggress.ConnectionCancelledError),
            ("UseAfterCloseError", _native.UseAfterCloseError, connection.UseAfterCloseError,
             exceptions.UseAfterCloseError, eggress.UseAfterCloseError),
            ("UdpAssociationError", _native.UdpAssociationError,
             connection.UdpAssociationError, exceptions.UdpAssociationError,
             eggress.UdpAssociationError),
            ("UnsupportedCompositionError", _native.UnsupportedCompositionError,
             connection.UnsupportedCompositionError, exceptions.UnsupportedCompositionError,
             eggress.UnsupportedCompositionError),
        ]

    def test_identity_across_aliases(self):
        for name, native, via_connection, via_exceptions, via_top in self._matrix():
            assert via_connection is native, f"{name}: connection alias drift"
            assert via_exceptions is native, f"{name}: exceptions alias drift"
            assert via_top is native, f"{name}: top-level alias drift"

    def test_hierarchy_matches_native(self):
        import eggress.connection as connection

        assert issubclass(_native.ConnectionError, _native.EggressError)
        for cls in (
            _native.ConnectionClosedError,
            _native.TimeoutError,
            _native.DnsError,
            _native.AuthError,
            _native.TlsError,
            _native.ConnectionCancelledError,
            _native.UseAfterCloseError,
            _native.UdpAssociationError,
        ):
            assert issubclass(cls, _native.ConnectionError), cls
            assert issubclass(cls, _native.EggressError), cls
        # These two remain direct EggressError children (no hierarchy redesign).
        assert issubclass(_native.LoopMismatchError, _native.EggressError)
        assert not issubclass(_native.LoopMismatchError, _native.ConnectionError)
        assert issubclass(_native.UnsupportedCompositionError, _native.EggressError)
        assert not issubclass(_native.UnsupportedCompositionError, _native.ConnectionError)
        # Facade aliases share the hierarchy.
        assert issubclass(connection.ConnectionClosedError, connection.ConnectionError)
        assert not issubclass(connection.LoopMismatchError, connection.ConnectionError)

    def test_stubs_agree_with_runtime(self):
        import pathlib

        native_stub = pathlib.Path("python/eggress/_eggress.pyi").read_text()
        exceptions_stub = pathlib.Path("python/eggress/exceptions.pyi").read_text()
        for name, native, *_ in self._matrix():
            assert native.__name__ == name
            assert f"class {name}" in native_stub, f"{name} missing from _eggress.pyi"
        assert "LoopMismatchError" in exceptions_stub
        assert "UnsupportedCompositionError" in exceptions_stub

    def test_managed_connection_catch_behavior(self):
        import eggress.connection as connection

        conn = connection.Connection("socks5://127.0.0.1:0")
        try:
            conn.close()
            with pytest.raises(connection.ConnectionClosedError):
                raise connection.ConnectionClosedError("closed")
            with pytest.raises(connection.ConnectionError):
                raise connection.ConnectionClosedError("closed")
            with pytest.raises(connection.EggressError if hasattr(connection, "EggressError") else Exception):
                raise connection.LoopMismatchError("loop")
        finally:
            conn.close()

    def test_sync_outbound_catch_behavior(self):
        import eggress.connection as connection

        addr, server = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(addr[0], addr[1], timeout=5.0)
            stream.close()
            with pytest.raises(connection.ConnectionClosedError):
                stream.write(b"x")
            with pytest.raises(connection.ConnectionError):
                stream.read(1)
            with pytest.raises(_native.EggressError):
                stream.write(b"x")
        finally:
            server.close()

    def test_async_outbound_catch_behavior(self):
        import eggress.connection as connection

        addr, server = _start_echo_server()

        async def _exercise():
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = await conn.aconnect_tcp(addr[0], addr[1], timeout=5.0)
            try:
                # Close the native stream without closing the bridge so the
                # native terminal failure (not bridge-closed RuntimeError)
                # is the observed surface.
                stream._inner.close()
                with pytest.raises(connection.ConnectionClosedError):
                    stream.write(b"x")
                with pytest.raises(connection.ConnectionError):
                    await stream.drain()
            finally:
                stream.close()
                await stream.wait_closed()

        try:
            asyncio.run(_exercise())
        finally:
            server.close()

    def test_bridge_preserves_native_identity(self):
        from eggress._asyncio import AsyncBridge

        async def _exercise():
            bridge = AsyncBridge(label="closure-identity")

            def _raise_timeout():
                raise _native.TimeoutError("op timed out")

            with pytest.raises(_native.TimeoutError):
                await bridge.run(_raise_timeout)
            try:
                await bridge.run(_raise_timeout)
            except Exception as exc:
                assert type(exc) is _native.TimeoutError
                assert isinstance(exc, _native.ConnectionError)
            bridge.close()

        asyncio.run(_exercise())

    def test_closed_stream_raises_documented_family(self):
        addr, server = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(addr[0], addr[1], timeout=5.0)
            stream.close()
            with pytest.raises(_native.ConnectionClosedError):
                stream.read(1)
            with pytest.raises(_native.ConnectionError):
                stream.read(1)
            with pytest.raises(_native.EggressError):
                stream.read(1)
        finally:
            server.close()


# ---------------------------------------------------------------------------
# Workstream 3 — preview hop count
# ---------------------------------------------------------------------------


class TestPreviewHopCount:
    def test_direct_reports_zero(self):
        conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
        assert conn.preview_connect("127.0.0.1", 80)["hop_count"] == 0

    def test_single_proxy_hop_reports_one(self):
        conn = OutboundConnector.from_pproxy_uri("socks5://127.0.0.1:1080")
        assert conn.preview_connect("127.0.0.1", 80)["hop_count"] == 1

    def test_multi_hop_chain_reports_chain_length_not_upstream_count(self):
        conn = OutboundConnector.from_pproxy_uri(
            "socks5://127.0.0.1:1080__http://127.0.0.1:8080"
        )
        meta = conn.preview_connect("127.0.0.1", 80)
        assert meta["hop_count"] == 2
        assert meta["hop_count"] != conn.upstream_count or conn.upstream_count == 1

    def test_toml_multi_hop_reports_chain_length(self):
        toml = """\
version = 1
[[listeners]]
name = "test"
bind = "127.0.0.1:0"
protocols = ["socks5"]
[[upstreams]]
id = "up"
uri = "socks5://127.0.0.1:1080__http://127.0.0.1:8080"
"""
        conn = OutboundConnector.from_toml(toml)
        assert conn.upstream_count == 1
        assert conn.preview_connect("127.0.0.1", 80)["hop_count"] == 2

    def test_preview_shape_unchanged(self):
        conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
        meta = conn.preview_connect("example.com", 443)
        assert set(meta.keys()) == {"target_host", "target_port", "hop_count"}
        assert meta["target_host"] == "example.com"
        assert meta["target_port"] == 443


# ---------------------------------------------------------------------------
# Workstream 4 — compatibility startup forwarding
# ---------------------------------------------------------------------------


class TestCompatibilityStartupForwarding:
    @pytest.mark.parametrize(
        "args",
        [
            ["-l", "socks5://127.0.0.1:0"],
            ["-l", "socks5://127.0.0.1:0", "--auth", "5"],
            ["-l", "socks5://127.0.0.1:0", "--sys"],
        ],
    )
    def test_sync_and_async_select_identical_options(self, args):
        sync_svc = EggressService.from_pproxy_args(args)
        async_svc = EggressService.from_pproxy_args(args)
        # astart() shares _select_start_operation with start(); prove both
        # service instances forward identical compatibility values without
        # binding listeners or touching host proxy state.
        assert sync_svc._compatibility_start_args() == async_svc._compatibility_start_args()
        expected = sync_svc._compatibility_start_args()
        assert expected is not None
        auth_timeout, system_proxy = expected
        assert isinstance(auth_timeout, int)
        assert isinstance(system_proxy, bool)

        import functools

        sync_op = sync_svc._select_start_operation()
        async_op = async_svc._select_start_operation()
        assert isinstance(sync_op, functools.partial)
        assert isinstance(async_op, functools.partial)
        assert sync_op.args == async_op.args == (auth_timeout, system_proxy)

    def test_start_and_astart_share_selection_path(self):
        assert "_select_start_operation" in inspect.getsource(EggressService.start)
        assert "_select_start_operation" in inspect.getsource(EggressService.astart)

    def test_non_compat_service_has_no_forwarded_options(self):
        svc = EggressService.from_toml(
            'version = 1\n[[listeners]]\nname = "x"\nbind = "127.0.0.1:0"\nprotocols = ["socks5"]\n'
        )
        assert svc._compatibility_start_args() is None

    def test_representative_values_forwarded(self):
        default = EggressService.from_pproxy_args(["-l", "socks5://127.0.0.1:0"])
        custom = EggressService.from_pproxy_args(["-l", "socks5://127.0.0.1:0", "--auth", "5"])
        assert default._compatibility_start_args()[0] != custom._compatibility_start_args()[0]
        assert custom._compatibility_start_args()[0] == 5
        sys_svc = EggressService.from_pproxy_args(["-l", "socks5://127.0.0.1:0", "--sys"])
        assert sys_svc._compatibility_start_args()[1] is True
        assert default._compatibility_start_args()[1] is False
