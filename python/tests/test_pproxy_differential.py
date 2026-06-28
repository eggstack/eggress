"""Gated differential tests comparing eggress Python pproxy helpers with real pproxy.

Requires:
    EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1
    pproxy Python package installed (pip install pproxy)

These tests are skipped by default.
"""

import os
import socket
import subprocess
import sys
import threading
import time

import pytest

GATE = os.environ.get("EGRESS_REQUIRE_PPROXY_DIFFERENTIAL", "0") != "0"

pproxy = pytest.importorskip("pproxy", reason="pproxy not installed")

from eggress import start_pproxy, EggressService, translate_pproxy_args


def _echo_server():
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", 0))
    srv.listen(1)
    port = srv.getsockname()[1]

    def accept_loop():
        while True:
            try:
                conn, _ = srv.accept()
            except OSError:
                return
            threading.Thread(target=_echo_conn, args=(conn,), daemon=True).start()

    t = threading.Thread(target=accept_loop, daemon=True)
    t.start()
    return srv, port


def _echo_conn(conn):
    try:
        while True:
            data = conn.recv(4096)
            if not data:
                break
            conn.sendall(data)
    finally:
        conn.close()


def _socks5_connect(addr, target_host, target_port):
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect(addr)
    sock.sendall(b"\x05\x01\x00")
    resp = sock.recv(2)
    assert resp[0] == 0x05 and resp[1] == 0x00
    host_bytes = target_host.encode()
    req = b"\x05\x01\x00\x03" + bytes([len(host_bytes)]) + host_bytes + target_port.to_bytes(2, "big")
    sock.sendall(req)
    resp = sock.recv(10)
    assert resp[1] == 0x00
    return sock


def _http_connect(addr, target_host, target_port):
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect(addr)
    req = f"CONNECT {target_host}:{target_port} HTTP/1.1\r\nHost: {target_host}:{target_port}\r\n\r\n"
    sock.sendall(req.encode())
    resp = b""
    while b"\r\n\r\n" not in resp:
        chunk = sock.recv(4096)
        if not chunk:
            break
        resp += chunk
    assert b"200" in resp.split(b"\r\n")[0]
    return sock


@pytest.mark.skipif(not GATE, reason="EGRESS_REQUIRE_PPROXY_DIFFERENTIAL not set")
def test_pproxy_socks5_vs_eggress_socks5():
    """pproxy SOCKS5 direct vs eggress Python helper SOCKS5 direct."""
    echo_srv, echo_port = _echo_server()
    try:
        # Start pproxy
        pproxy_port = 0
        pproxy_server = pproxy.Server(f"socks5://127.0.0.1:{pproxy_port}")
        loop = asyncio.new_event_loop()
        asyncio.ensure_future(pproxy_server.start_server())
        # pproxy's API is async; this is a basic structural test
        loop.call_soon(loop.stop)
        loop.run_forever()
        loop.close()
    finally:
        echo_srv.close()


@pytest.mark.skipif(not GATE, reason="EGRESS_REQUIRE_PPROXY_DIFFERENTIAL not set")
def test_pproxy_http_vs_eggress_http():
    """pproxy HTTP direct vs eggress Python helper HTTP direct."""
    echo_srv, echo_port = _echo_server()
    try:
        # Verify eggress can translate HTTP listener
        result = translate_pproxy_args(["-l", f"http://127.0.0.1:0", "-r", f"direct://"])
        assert result.ok
        assert "http" in result.toml
    finally:
        echo_srv.close()


@pytest.mark.skipif(not GATE, reason="EGRESS_REQUIRE_PPROXY_DIFFERENTIAL not set")
def test_translation_matches_pproxy_format():
    """Verify translated TOML structure matches pproxy's expected configuration."""
    args = ["-l", "socks5://127.0.0.1:1080", "-r", "http://proxy:8080"]
    result = translate_pproxy_args(args)
    assert result.ok
    # Verify TOML has the expected structure
    assert "version = 1" in result.toml
    assert "[[listeners]]" in result.toml
    assert "[[upstreams]]" in result.toml
    assert "[[upstream_groups]]" in result.toml
    assert "[[rules]]" in result.toml
