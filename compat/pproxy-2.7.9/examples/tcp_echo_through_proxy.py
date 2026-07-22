#!/usr/bin/env python3
"""Executable oracle fixture: TCP data relay through a pproxy Connection.

Starts a local TCP echo server, then connects through a pproxy proxy
(direct:// by default) and verifies data round-trips correctly.

Environment variables:
    PROXY_URI  - Proxy URI (default: direct://)
    ECHO_PORT  - Echo server port (default: 0, auto-assigned)

Provenance: Derived from pproxy 2.7.9 API patterns.
License: MIT (pproxy)
Tested with: pproxy==2.7.9 on Python 3.11
"""
import asyncio
import os
import socket
import sys
import threading

try:
    from importlib.metadata import version as _get_version
    PPROXY_VERSION = _get_version("pproxy")
except Exception:
    PPROXY_VERSION = "unknown"

passed = 0
failed = 0


def check(name, condition, detail=""):
    global passed, failed
    if condition:
        print(f"  PASS: {name}")
        passed += 1
    else:
        msg = f"  FAIL: {name}"
        if detail:
            msg += f" -- {detail}"
        print(msg)
        failed += 1


def start_echo_server():
    """Start a TCP echo server on localhost, return (host, port, server_thread, stop_event)."""
    stop_event = threading.Event()
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind(("127.0.0.1", 0))
    sock.listen(5)
    host, port = sock.getsockname()

    def serve():
        sock.settimeout(0.5)
        while not stop_event.is_set():
            try:
                conn, _ = sock.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            t = threading.Thread(target=_echo_client, args=(conn,), daemon=True)
            t.start()
        sock.close()

    def _echo_client(conn):
        try:
            conn.settimeout(1.0)
            while True:
                data = conn.recv(4096)
                if not data:
                    break
                conn.sendall(data)
        except (socket.timeout, OSError):
            pass
        finally:
            conn.close()

    t = threading.Thread(target=serve, daemon=True)
    t.start()
    return host, port, t, stop_event


async def test_tcp_echo_direct():
    """Test TCP data through direct:// proxy."""
    import pproxy
    from pproxy import Connection

    proxy_uri = os.environ.get("PROXY_URI", "direct://")
    echo_host, echo_port, echo_thread, echo_stop = start_echo_server()

    try:
        conn = Connection(proxy_uri)
        check("direct connection constructed", conn is not None)

        reader, writer = await conn.tcp_connect(echo_host, echo_port)
        check("tcp_connect returned reader/writer", reader is not None and writer is not None)

        test_data = b"Hello from pproxy oracle fixture\n"
        writer.write(test_data)
        await writer.drain()

        received = await asyncio.wait_for(reader.read(4096), timeout=2.0)
        check("echo data matches", received == test_data, f"got {received!r}")

        writer.close()
        await writer.wait_closed()
        check("writer closed cleanly", True)
    finally:
        echo_stop.set()


async def test_tcp_large_payload():
    """Test large payload through proxy."""
    import pproxy
    from pproxy import Connection

    proxy_uri = os.environ.get("PROXY_URI", "direct://")
    echo_host, echo_port, echo_thread, echo_stop = start_echo_server()

    try:
        conn = Connection(proxy_uri)
        reader, writer = await conn.tcp_connect(echo_host, echo_port)

        test_data = b"X" * 65536
        writer.write(test_data)
        await writer.drain()

        received = b""
        while len(received) < len(test_data):
            chunk = await asyncio.wait_for(reader.read(4096), timeout=2.0)
            if not chunk:
                break
            received += chunk
        check("large payload matches", received == test_data, f"sent {len(test_data)}, got {len(received)}")

        writer.close()
        await writer.wait_closed()
    finally:
        echo_stop.set()


async def main_async():
    print(f"pproxy {PPROXY_VERSION} tcp_echo_through_proxy fixture")
    print(f"Python {sys.version}")
    print(f"Proxy URI: {os.environ.get('PROXY_URI', 'direct://')}")
    print()

    await test_tcp_echo_direct()
    await test_tcp_large_payload()

    print()
    print(f"Results: {passed} passed, {failed} failed")
    return 1 if failed else 0


def main():
    return asyncio.run(main_async())


if __name__ == "__main__":
    sys.exit(main())
