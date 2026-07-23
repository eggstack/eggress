#!/usr/bin/env python3
"""Executable oracle fixture: UDP datagram relay through a pproxy Connection.

Starts a local UDP echo server, then sends datagrams through a pproxy
proxy (direct:// by default) and verifies responses.

Environment variables:
    PROXY_URI  - Proxy URI (default: direct://)

Provenance: Eggress-authored behavioral scenario based on pproxy 2.7.9 public API.
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


def start_udp_echo_server():
    """Start a UDP echo server, return (host, port, stop_event)."""
    stop_event = threading.Event()
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(("127.0.0.1", 0))
    host, port = sock.getsockname()

    def serve():
        sock.settimeout(0.5)
        while not stop_event.is_set():
            try:
                data, addr = sock.recvfrom(4096)
            except socket.timeout:
                continue
            except OSError:
                break
            sock.sendto(data, addr)
        sock.close()

    t = threading.Thread(target=serve, daemon=True)
    t.start()
    return host, port, t, stop_event


async def test_udp_echo_direct():
    """Test UDP through direct:// proxy."""
    import pproxy
    from pproxy import Connection

    proxy_uri = os.environ.get("PROXY_URI", "direct://")
    echo_host, echo_port, echo_thread, echo_stop = start_udp_echo_server()

    try:
        conn = Connection(proxy_uri)
        check("direct UDP connection constructed", conn is not None)

        test_data = b"UDP hello from pproxy oracle"
        rserver = (echo_host, echo_port)
        reply = await conn.udp_sendto(test_data, rserver)
        check("udp_sendto returned data", reply is not None and len(reply) > 0)
        check("UDP echo data matches", reply == test_data, f"got {reply!r}")
    finally:
        echo_stop.set()


async def test_udp_multiple_datagrams():
    """Test multiple UDP datagrams."""
    import pproxy
    from pproxy import Connection

    proxy_uri = os.environ.get("PROXY_URI", "direct://")
    echo_host, echo_port, echo_thread, echo_stop = start_udp_echo_server()

    try:
        conn = Connection(proxy_uri)
        rserver = (echo_host, echo_port)

        for i in range(5):
            test_data = f"datagram {i}".encode()
            reply = await conn.udp_sendto(test_data, rserver)
            check(f"datagram {i} echo matches", reply == test_data)
    finally:
        echo_stop.set()


async def main_async():
    print(f"pproxy {PPROXY_VERSION} udp_echo_through_proxy fixture")
    print(f"Python {sys.version}")
    print(f"Proxy URI: {os.environ.get('PROXY_URI', 'direct://')}")
    print()

    await test_udp_echo_direct()
    await test_udp_multiple_datagrams()

    print()
    print(f"Results: {passed} passed, {failed} failed")
    return 1 if failed else 0


def main():
    return asyncio.run(main_async())


if __name__ == "__main__":
    sys.exit(main())
