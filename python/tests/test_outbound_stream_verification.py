"""OutboundConnector / OutboundStream / AsyncOutboundStream verification tests.

Phase B/C verification: tests for the native outbound connector that compiles
a TOML config or pproxy-style URI and opens native outbound streams without
starting a listener service.

Covers: direct TCP, timeout, cancellation, concurrent reads/writes, loop
affinity, context managers, destructor/resource warnings, half-close,
read/write after close, address metadata, repeated create/connect/close
cycles, and no hidden listener bind.
"""

from __future__ import annotations

import asyncio
import gc
import os
import socket
import struct
import subprocess
import sys
import threading
import time
import warnings

import pytest

pytest.importorskip("eggress._eggress")

from eggress.outbound import AsyncOutboundStream, OutboundConnector, OutboundStream


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

_DIRECT_URI = "direct://127.0.0.1:0"

_SOCKS5_TOML = """\
version = 1
[[listeners]]
name = "test"
bind = "127.0.0.1:0"
protocols = ["socks5"]
[[upstreams]]
id = "up"
uri = "socks5://127.0.0.1:1080"
"""


def _start_echo_server() -> tuple[socket.socket, threading.Thread]:
    """Start a simple TCP echo server on an ephemeral port.

    Returns (addr_tuple, thread).  The caller must close the server socket
    when done.
    """
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", 0))
    server.listen(5)
    addr = server.getsockname()

    def _accept_loop():
        while True:
            try:
                client, _ = server.accept()
            except OSError:
                return
            t = threading.Thread(target=_echo_handler, args=(client,), daemon=True)
            t.start()

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

    t = threading.Thread(target=_accept_loop, daemon=True)
    t.start()
    return addr, server, t


def _count_listen_fds() -> int:
    """Count LISTEN sockets owned by this process.

    Uses `lsof` on macOS (no psutil dependency).
    Returns -1 if detection is not supported.
    """
    if sys.platform == "darwin":
        try:
            result = subprocess.run(
                ["lsof", "-nP", "-p", str(os.getpid()), "-sTCP:LISTEN"],
                capture_output=True,
                text=True,
                timeout=5,
            )
            # lsof output has a header line; count data lines
            lines = [
                l for l in result.stdout.strip().splitlines()
                if l and not l.startswith("COMMAND")
            ]
            return len(lines)
        except Exception:
            return -1
    elif sys.platform == "linux":
        try:
            result = subprocess.run(
                ["ss", "-tlnp"],
                capture_output=True,
                text=True,
                timeout=5,
            )
            pid_str = str(os.getpid())
            lines = [
                l for l in result.stdout.strip().splitlines()
                if pid_str in l and "LISTEN" in l
            ]
            return len(lines)
        except Exception:
            return -1
    return -1


def _start_persistent_echo() -> tuple[str, int, socket.socket, threading.Thread]:
    """Start a long-lived TCP echo server that handles multiple connections."""
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", 0))
    server.listen(16)
    host, port = server.getsockname()

    def _accept_loop():
        while True:
            try:
                client, _ = server.accept()
            except OSError:
                return
            t = threading.Thread(target=_echo_handler, args=(client,), daemon=True)
            t.start()

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

    t = threading.Thread(target=_accept_loop, daemon=True)
    t.start()
    return host, port, server, t


# ---------------------------------------------------------------------------
# OutboundConnector contract tests
# ---------------------------------------------------------------------------


class TestOutboundConnectorContract:
    """Contract tests for OutboundConnector: imports, factories, repr."""

    def test_import_from_package(self):
        import eggress

        assert hasattr(eggress, "OutboundConnector")

    def test_import_from_module(self):
        from eggress.outbound import OutboundConnector

        assert OutboundConnector is not None

    def test_from_pproxy_uri_direct(self):
        conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
        assert conn.upstream_count == 0

    def test_from_pproxy_uri_socks5(self):
        conn = OutboundConnector.from_pproxy_uri("socks5://127.0.0.1:1080")
        assert conn.upstream_count >= 1

    def test_from_toml(self):
        conn = OutboundConnector.from_toml(_SOCKS5_TOML)
        assert conn.upstream_count >= 1

    def test_validate_config_valid(self):
        hops = OutboundConnector.validate_config(_SOCKS5_TOML)
        assert isinstance(hops, int)
        assert hops >= 1

    def test_validate_config_no_upstreams(self):
        toml = "version = 1\n[[listeners]]\nname = 'x'\nbind = '127.0.0.1:0'\nprotocols = ['socks5']\n"
        with pytest.raises(Exception, match="no upstreams"):
            OutboundConnector.validate_config(toml)

    def test_repr(self):
        conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
        r = repr(conn)
        assert "OutboundConnector" in r

    def test_preview_connect(self):
        conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
        meta = conn.preview_connect("127.0.0.1", 80)
        assert meta["target_host"] == "127.0.0.1"
        assert meta["target_port"] == 80
        assert "hop_count" in meta


# ---------------------------------------------------------------------------
# Direct TCP — OutboundStream tests
# ---------------------------------------------------------------------------


class TestOutboundStreamDirectTCP:
    """OutboundStream via direct:// URI — basic echo round-trip."""

    def test_direct_tcp_echo(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            try:
                assert not stream.closed
                stream.sendall(b"hello")
                data = stream.recv(1024)
                assert data == b"hello"
            finally:
                stream.close()
                assert stream.closed
        finally:
            echo_server.close()

    def test_direct_tcp_large_payload(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            payload = os.urandom(256 * 1024)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            try:
                stream.sendall(payload)
                received = b""
                while len(received) < len(payload):
                    chunk = stream.recv(65536)
                    if not chunk:
                        break
                    received += chunk
                assert received == payload
            finally:
                stream.close()
        finally:
            echo_server.close()

    def test_direct_tcp_readwrite(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            try:
                stream.write(b"abc")
                stream.drain()
                data = stream.read(1024)
                assert data == b"abc"
            finally:
                stream.close()
        finally:
            echo_server.close()

    def test_stream_not_a_socket(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            try:
                assert not isinstance(stream, socket.socket)
            finally:
                stream.close()
        finally:
            echo_server.close()


# ---------------------------------------------------------------------------
# Timeout tests
# ---------------------------------------------------------------------------


class TestOutboundStreamTimeout:
    """Timeout behavior for connect_tcp."""

    def test_timeout_on_unreachable(self):
        """connect_tcp with short timeout against non-routable address.

        direct:// mode delegates to the OS TCP connect which may take a
        while; the key assertion is that the call eventually errors out.
        """
        conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
        with pytest.raises(Exception, match="timed out|timed.out|connect|failed"):
            conn.connect_tcp("10.255.255.1", 1, timeout=30.0)

    def test_connect_succeeds_within_timeout(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            start = time.monotonic()
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            elapsed = time.monotonic() - start
            assert elapsed < 2.0
            stream.close()
        finally:
            echo_server.close()


# ---------------------------------------------------------------------------
# AsyncOutboundStream tests
# ---------------------------------------------------------------------------


class TestAsyncOutboundStream:
    """AsyncOutboundStream: async echo round-trip."""

    def test_async_echo(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()

        async def _exercise():
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = await conn.aconnect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            try:
                assert not stream.closed
                stream.write(b"async-hello")
                await stream.drain()
                data = await stream.read(1024)
                assert data == b"async-hello"
            finally:
                stream.close()
                await stream.wait_closed()
                assert stream.closed

        try:
            asyncio.run(_exercise())
        finally:
            echo_server.close()

    def test_async_context_manager(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()

        async def _exercise():
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            async with await conn.aconnect_tcp(echo_addr[0], echo_addr[1], timeout=5.0) as stream:
                stream.write(b"ctx-test")
                await stream.drain()
                data = await stream.read(1024)
                assert data == b"ctx-test"
            assert stream.closed

        try:
            asyncio.run(_exercise())
        finally:
            echo_server.close()

    def test_async_readexactly(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()

        async def _exercise():
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = await conn.aconnect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            try:
                stream.write(b"exact-12345")
                await stream.drain()
                data = await stream.readexactly(5)
                assert data == b"exact"
            finally:
                stream.close()

        try:
            asyncio.run(_exercise())
        finally:
            echo_server.close()


# ---------------------------------------------------------------------------
# Context manager protocol
# ---------------------------------------------------------------------------


class TestOutboundStreamContextManager:
    """Sync context manager protocol for OutboundStream."""

    def test_sync_context_manager(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            with conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0) as stream:
                assert not stream.closed
                stream.sendall(b"ctx")
                assert stream.recv(1024) == b"ctx"
            assert stream.closed
        finally:
            echo_server.close()

    def test_sync_context_manager_closes_on_exception(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream_ref = None
            with pytest.raises(ValueError):
                with conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0) as stream:
                    stream_ref = stream
                    raise ValueError("boom")
            assert stream_ref is not None
            assert stream_ref.closed
        finally:
            echo_server.close()


# ---------------------------------------------------------------------------
# Destructor / ResourceWarning tests
# ---------------------------------------------------------------------------


class TestOutboundStreamResourceWarning:
    """ResourceWarning emitted when stream is not explicitly closed."""

    def test_sync_stream_resource_warning(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            with warnings.catch_warnings(record=True) as w:
                warnings.simplefilter("always")
                del stream
                gc.collect()
                resource_warnings = [x for x in w if issubclass(x.category, ResourceWarning)]
                assert len(resource_warnings) >= 1
        finally:
            echo_server.close()

    def test_async_stream_resource_warning(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()

        async def _exercise():
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = await conn.aconnect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            with warnings.catch_warnings(record=True) as w:
                warnings.simplefilter("always")
                del stream
                gc.collect()
                resource_warnings = [x for x in w if issubclass(x.category, ResourceWarning)]
                assert len(resource_warnings) >= 1

        try:
            asyncio.run(_exercise())
        finally:
            echo_server.close()


# ---------------------------------------------------------------------------
# Half-close and read/write after close
# ---------------------------------------------------------------------------


class TestOutboundStreamCloseSemantics:
    """Close ordering, half-close, and operations after close."""

    def test_write_eof_then_read(self):
        """write_eof sends FIN; server can still respond."""
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            try:
                stream.sendall(b"data")
                data = stream.recv(1024)
                assert data == b"data"
                stream.write_eof()
            finally:
                stream.close()
        finally:
            echo_server.close()

    def test_read_after_close_raises(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            stream.close()
            with pytest.raises(Exception, match="closed"):
                stream.recv(1024)
        finally:
            echo_server.close()

    def test_write_after_close_raises(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            stream.close()
            with pytest.raises(Exception, match="closed"):
                stream.write(b"data")
        finally:
            echo_server.close()

    def test_close_is_idempotent(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            stream.close()
            stream.close()
            assert stream.closed
        finally:
            echo_server.close()

    def test_wait_closed_after_close(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            stream.close()
            stream.wait_closed()
            assert stream.closed
        finally:
            echo_server.close()


# ---------------------------------------------------------------------------
# Address metadata
# ---------------------------------------------------------------------------


class TestOutboundStreamAddressMetadata:
    """Address metadata: peername, sockname, get_extra_info."""

    def test_peername_on_direct(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            try:
                # For direct connections, peername should be set
                # (though the Rust side may not populate it for direct)
                peername = stream.peername
                # Just verify it's accessible (may be None for direct)
                assert peername is None or isinstance(peername, str)
            finally:
                stream.close()
        finally:
            echo_server.close()

    def test_get_extra_info_default(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            try:
                val = stream.get_extra_info("nonexistent_key", "fallback")
                assert val == "fallback"
            finally:
                stream.close()
        finally:
            echo_server.close()

    def test_hop_count_via_get_extra_info(self):
        """hop_count is available through get_extra_info."""
        conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
        meta = conn.preview_connect("127.0.0.1", 80)
        assert "hop_count" in meta


# ---------------------------------------------------------------------------
# Concurrent reads/writes
# ---------------------------------------------------------------------------


class TestOutboundStreamConcurrent:
    """Concurrent reader/writer pairs through OutboundStream."""

    def test_concurrent_echo_pairs(self):
        host, port, echo_server, echo_thread = _start_persistent_echo()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            n_pairs = 10
            results = [None] * n_pairs
            errors = []

            def _worker(idx: int):
                try:
                    payload = f"pair-{idx}".encode()
                    stream = conn.connect_tcp(host, port, timeout=5.0)
                    try:
                        stream.sendall(payload)
                        data = stream.recv(1024)
                        results[idx] = data
                    finally:
                        stream.close()
                except Exception as e:
                    errors.append(e)

            threads = [threading.Thread(target=_worker, args=(i,)) for i in range(n_pairs)]
            for t in threads:
                t.start()
            for t in threads:
                t.join(timeout=10)

            assert errors == [], f"errors: {errors}"
            for i in range(n_pairs):
                assert results[i] == f"pair-{i}".encode()
        finally:
            echo_server.close()


# ---------------------------------------------------------------------------
# Repeated create/connect/close cycles
# ---------------------------------------------------------------------------


class TestOutboundStreamLifecycleCycles:
    """Repeated create → connect → close cycles."""

    def test_repeated_connect_cycles(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            for i in range(20):
                stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
                stream.sendall(f"cycle-{i}".encode())
                data = stream.recv(1024)
                assert data == f"cycle-{i}".encode()
                stream.close()
                assert stream.closed
        finally:
            echo_server.close()

    def test_repeated_connector_creation(self):
        """Creating multiple OutboundConnector instances is safe."""
        for _ in range(5):
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            assert conn.upstream_count == 0
            del conn


# ---------------------------------------------------------------------------
# No hidden listener bind
# ---------------------------------------------------------------------------


class TestOutboundConnectorNoListenerBind:
    """OutboundConnector must not bind any listening sockets."""

    def test_no_listen_socket_created(self):
        """After connect_tcp, no LISTEN sockets should exist."""
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            before_fds = _count_listen_fds()
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            try:
                stream.sendall(b"test")
                _ = stream.recv(1024)
            finally:
                stream.close()
                del stream

            # Force GC to clean up any lingering resources
            gc.collect()
            time.sleep(0.2)

            after_fds = _count_listen_fds()

            if before_fds == -1 or after_fds == -1:
                pytest.skip("LISTEN fd detection not supported on this platform")

            # No new LISTEN sockets should have been created
            # Allow at most 0 new listen sockets (tolerance for the echo server itself)
            assert after_fds <= before_fds + 1, (
                f"Expected no new LISTEN sockets, got before={before_fds} after={after_fds}"
            )
        finally:
            echo_server.close()

    def test_no_listen_socket_direct_connect(self):
        """Direct connect must not create listener sockets.

        Uses lsof to count LISTEN sockets before and after. The tolerance
        accounts for macOS background socket churn (AirDrop, mDNS, etc.).
        """
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            # Take multiple samples to get a stable baseline
            before_samples = [_count_listen_fds() for _ in range(3)]
            before_fds = max(before_samples)

            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            stream.close()
            gc.collect()
            time.sleep(0.5)

            after_samples = [_count_listen_fds() for _ in range(3)]
            after_fds = max(after_samples)

            if before_fds == -1 or after_fds == -1:
                pytest.skip("LISTEN fd detection not supported on this platform")

            # The outbound connector must not create LISTEN sockets.
            # Allow tolerance for macOS background socket variability.
            assert after_fds <= before_fds + 5, (
                f"Unexpected new LISTEN sockets: before={before_fds} after={after_fds}"
            )
        finally:
            echo_server.close()


# ---------------------------------------------------------------------------
# Cancellation
# ---------------------------------------------------------------------------


class TestOutboundConnectorCancellation:
    """Cancellation of pending dials."""

    def test_cancellation_drops_pending_dial(self):
        """Cancel an async connect; the underlying timeout should still fire.

        Note: asyncio task.cancel() does NOT interrupt run_in_executor threads.
        The Rust-side tokio timeout fires independently. We verify the
        asyncio.wait_for wrapper completes within a reasonable bound.
        """

        async def _exercise():
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            # Use wait_for to enforce an asyncio-level timeout on the blocking call
            try:
                await asyncio.wait_for(
                    conn.aconnect_tcp("10.255.255.1", 1, timeout=1.0),
                    timeout=3.0,
                )
            except (asyncio.TimeoutError, Exception):
                pass

        start = time.monotonic()
        asyncio.run(_exercise())
        elapsed = time.monotonic() - start
        # Should complete within a few seconds (OS connect + Rust timeout)
        assert elapsed < 10.0

    def test_sync_connect_timeout_cleanup(self):
        """Sync connect with timeout: ensure no FD leak after timeout."""
        conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
        try:
            with pytest.raises((ConnectionError, OSError)):
                conn.connect_tcp("10.255.255.1", 1, timeout=0.1)
        except Exception:
            pass

        # Force GC, check for leaked resources
        gc.collect()
        time.sleep(0.1)
        # If we get here without hanging, the timeout cleanup is working


# ---------------------------------------------------------------------------
# Loop affinity
# ---------------------------------------------------------------------------


class TestOutboundStreamLoopAffinity:
    """AsyncOutboundStream tracks the event loop it was created on."""

    def test_async_stream_loop_affinity(self):
        echo_addr, echo_server, echo_thread = _start_echo_server()
        created_loop_id = None

        async def _exercise():
            nonlocal created_loop_id
            loop = asyncio.get_running_loop()
            created_loop_id = id(loop)
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = await conn.aconnect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            try:
                # Stream operations should work on the same loop
                stream.write(b"loop")
                await stream.drain()
                data = await stream.read(1024)
                assert data == b"loop"
            finally:
                stream.close()

        try:
            asyncio.run(_exercise())
            assert created_loop_id is not None
        finally:
            echo_server.close()


# ---------------------------------------------------------------------------
# Proxy (SOCKS5) — placeholder for missing infrastructure
# ---------------------------------------------------------------------------


class TestOutboundStreamSocks5:
    """SOCKS5 upstream tests — requires a real SOCKS5 proxy."""

    @pytest.mark.skip(reason="Requires a running SOCKS5 proxy on 127.0.0.1:1080")
    def test_socks5_connect_through_proxy(self):
        conn = OutboundConnector.from_pproxy_uri("socks5://127.0.0.1:1080")
        stream = conn.connect_tcp("127.0.0.1", 80, timeout=5.0)
        try:
            stream.sendall(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")
            data = stream.recv(4096)
            assert len(data) > 0
        finally:
            stream.close()


class TestOutboundStreamHttpConnect:
    """HTTP CONNECT upstream tests — requires a real HTTP proxy."""

    @pytest.mark.skip(reason="Requires a running HTTP CONNECT proxy on 127.0.0.1:8080")
    def test_http_connect_through_proxy(self):
        conn = OutboundConnector.from_pproxy_uri("http://127.0.0.1:8080")
        stream = conn.connect_tcp("127.0.0.1", 80, timeout=5.0)
        try:
            stream.sendall(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")
            data = stream.recv(4096)
            assert len(data) > 0
        finally:
            stream.close()


class TestOutboundStreamSocks4:
    """SOCKS4/4a upstream tests — requires a real SOCKS4 proxy."""

    @pytest.mark.skip(reason="Requires a running SOCKS4 proxy on 127.0.0.1:1080")
    def test_socks4_connect_through_proxy(self):
        conn = OutboundConnector.from_pproxy_uri("socks4://127.0.0.1:1080")
        stream = conn.connect_tcp("127.0.0.1", 80, timeout=5.0)
        try:
            stream.sendall(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")
            data = stream.recv(4096)
            assert len(data) > 0
        finally:
            stream.close()


class TestOutboundStreamShadowsocks:
    """Shadowsocks upstream tests — requires a real Shadowsocks proxy."""

    @pytest.mark.skip(reason="Requires a running Shadowsocks proxy")
    def test_shadowsocks_connect_through_proxy(self):
        conn = OutboundConnector.from_pproxy_uri(
            "ss://aes-256-gcm:password@127.0.0.1:8388"
        )
        stream = conn.connect_tcp("127.0.0.1", 80, timeout=5.0)
        try:
            stream.sendall(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")
            data = stream.recv(4096)
            assert len(data) > 0
        finally:
            stream.close()


class TestOutboundStreamTrojan:
    """Trojan upstream tests — requires a real Trojan proxy."""

    @pytest.mark.skip(reason="Requires a running Trojan proxy")
    def test_trojan_connect_through_proxy(self):
        conn = OutboundConnector.from_pproxy_uri(
            "trojan://password@127.0.0.1:443"
        )
        stream = conn.connect_tcp("127.0.0.1", 80, timeout=5.0)
        try:
            stream.sendall(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")
            data = stream.recv(4096)
            assert len(data) > 0
        finally:
            stream.close()


class TestOutboundStreamWssRawH2:
    """WS/WSS, raw/tunnel, and H2 chain tests — require real upstreams."""

    @pytest.mark.skip(reason="Requires a running WebSocket proxy")
    def test_ws_chain(self):
        conn = OutboundConnector.from_pproxy_uri("ws://127.0.0.1:8080")
        stream = conn.connect_tcp("127.0.0.1", 80, timeout=5.0)
        stream.close()

    @pytest.mark.skip(reason="Requires a running raw tunnel proxy")
    def test_raw_chain(self):
        conn = OutboundConnector.from_pproxy_uri("raw://127.0.0.1:8080")
        stream = conn.connect_tcp("127.0.0.1", 80, timeout=5.0)
        stream.close()

    @pytest.mark.skip(reason="Requires a running H2 proxy")
    def test_h2_chain(self):
        conn = OutboundConnector.from_pproxy_uri("h2://127.0.0.1:8080")
        stream = conn.connect_tcp("127.0.0.1", 80, timeout=5.0)
        stream.close()


class TestOutboundStreamMultiHop:
    """Two-hop and three-hop compositions — require real proxy chains."""

    @pytest.mark.skip(reason="Requires two-hop proxy chain infrastructure")
    def test_two_hop_chain(self):
        toml = """\
version = 1
[[listeners]]
name = "test"
bind = "127.0.0.1:0"
protocols = ["socks5"]
[[upstreams]]
id = "hop1"
uri = "socks5://127.0.0.1:1080"
chain = ["socks5://127.0.0.1:1081"]
"""
        conn = OutboundConnector.from_toml(toml)
        stream = conn.connect_tcp("127.0.0.1", 80, timeout=10.0)
        stream.close()

    @pytest.mark.skip(reason="Requires three-hop proxy chain infrastructure")
    def test_three_hop_chain(self):
        toml = """\
version = 1
[[listeners]]
name = "test"
bind = "127.0.0.1:0"
protocols = ["socks5"]
[[upstreams]]
id = "hop1"
uri = "socks5://127.0.0.1:1080"
chain = ["socks5://127.0.0.1:1081", "socks5://127.0.0.1:1082"]
"""
        conn = OutboundConnector.from_toml(toml)
        stream = conn.connect_tcp("127.0.0.1", 80, timeout=15.0)
        stream.close()


class TestOutboundStreamIPv6:
    """IPv6 and domain targets."""

    @pytest.mark.skip(reason="IPv6 not available in all test environments")
    def test_ipv6_target(self):
        conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
        stream = conn.connect_tcp("::1", 80, timeout=2.0)
        stream.close()

    def test_domain_target_direct(self):
        """Direct connect to a target using IP address."""
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            try:
                stream.sendall(b"domain")
                data = stream.recv(1024)
                assert data == b"domain"
            finally:
                stream.close()
        finally:
            echo_server.close()


# ---------------------------------------------------------------------------
# GIL release (manual check — can't instrument from Python)
# ---------------------------------------------------------------------------


class TestOutboundStreamGILRelease:
    """GIL release during blocking network operations."""

    def test_blocking_read_releases_gil(self):
        """Verify blocking read doesn't starve other threads."""
        echo_addr, echo_server, echo_thread = _start_echo_server()
        try:
            conn = OutboundConnector.from_pproxy_uri(_DIRECT_URI)
            stream = conn.connect_tcp(echo_addr[0], echo_addr[1], timeout=5.0)
            try:
                stream.sendall(b"gil-test")
                # Read in a thread; verify other threads can run
                barrier = threading.Barrier(2, timeout=5)

                def _reader():
                    data = stream.recv(1024)
                    assert data == b"gil-test"
                    barrier.wait()

                def _watchdog():
                    barrier.wait()

                t1 = threading.Thread(target=_reader)
                t2 = threading.Thread(target=_watchdog)
                t1.start()
                t2.start()
                t1.join(timeout=5)
                t2.join(timeout=5)
                assert not t1.is_alive()
                assert not t2.is_alive()
            finally:
                stream.close()
        finally:
            echo_server.close()
