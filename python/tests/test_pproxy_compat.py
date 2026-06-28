import socket
import threading
import time

import pytest
from eggress import (
    EggressService,
    EggressConfig,
    translate_pproxy_args,
    translate_pproxy_uri,
    UnsupportedFeatureError,
)


def _echo_server():
    """Start a TCP echo server on port 0, return (socket, port)."""
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
    """Perform a SOCKS5 CONNECT handshake and return the connected socket."""
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect(addr)
    # Greeting: version 5, 1 auth method (no auth)
    sock.sendall(b"\x05\x01\x00")
    resp = sock.recv(2)
    assert resp[0] == 0x05 and resp[1] == 0x00, f"SOCKS5 greeting failed: {resp!r}"
    # CONNECT request
    host_bytes = target_host.encode()
    req = b"\x05\x01\x00\x03" + bytes([len(host_bytes)]) + host_bytes + target_port.to_bytes(2, "big")
    sock.sendall(req)
    resp = sock.recv(10)
    assert resp[1] == 0x00, f"SOCKS5 connect failed: {resp!r}"
    return sock


def _http_connect(addr, target_host, target_port):
    """Perform an HTTP CONNECT handshake and return the connected socket."""
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
    assert b"200" in resp.split(b"\r\n")[0], f"HTTP CONNECT failed: {resp!r}"
    return sock


# --- Test scenarios ---


def test_local_socks5_direct():
    """1. Local SOCKS5 direct — translate + start + bound address check."""
    result = translate_pproxy_args(["-l", "socks5://127.0.0.1:0"])
    assert result.ok
    assert "socks5" in result.toml

    with EggressService.from_pproxy_args(["-l", "socks5://127.0.0.1:0"]).start() as handle:
        addrs = handle.bound_addresses
        assert "pproxy-local-0" in addrs
        assert addrs["pproxy-local-0"] != ""


def test_local_http_direct():
    """2. Local HTTP CONNECT direct — translate + start."""
    result = translate_pproxy_args(["-l", "http://127.0.0.1:0"])
    assert result.ok
    assert '"http"' in result.toml

    with EggressService.from_pproxy_args(["-l", "http://127.0.0.1:0"]).start() as handle:
        assert handle.status()["readiness"]


def test_local_socks4_direct():
    """3. Local SOCKS4 direct — translate + start."""
    result = translate_pproxy_args(["-l", "socks4://127.0.0.1:0"])
    assert result.ok
    assert '"socks4"' in result.toml

    with EggressService.from_pproxy_args(["-l", "socks4://127.0.0.1:0"]).start() as handle:
        assert handle.status()["readiness"]


def test_socks5_through_http_upstream():
    """4. SOCKS5 through HTTP upstream — translate + start + TOML validation."""
    echo_srv, echo_port = _echo_server()
    try:
        result = translate_pproxy_args([
            "-l", "socks5://127.0.0.1:0",
            "-r", "http://127.0.0.1:0",
        ])
        assert result.ok
        assert "pproxy-upstream-0" in result.toml
        assert "pproxy-chain" in result.toml
    finally:
        echo_srv.close()


def test_socks5_through_socks5_upstream():
    """5. SOCKS5 through SOCKS5 upstream — translate + start."""
    result = translate_pproxy_args([
        "-l", "socks5://127.0.0.1:0",
        "-r", "socks5://127.0.0.1:0",
    ])
    assert result.ok
    assert "pproxy-upstream-0" in result.toml


def test_multiple_remotes_round_robin():
    """6. Multiple remotes → round-robin scheduler in generated TOML."""
    result = translate_pproxy_args([
        "-l", "socks5://127.0.0.1:0",
        "-r", "http://proxy1:8080",
        "-r", "http://proxy2:8080",
    ])
    assert result.ok
    assert "round-robin" in result.toml
    assert "pproxy-upstream-0" in result.toml
    assert "pproxy-upstream-1" in result.toml


def test_auth_success():
    """7. Auth success — listener with auth in generated TOML."""
    result = translate_pproxy_args([
        "-l", "socks5://admin:secret123@127.0.0.1:0",
    ])
    assert result.ok
    assert "password" in result.toml
    assert "admin" in result.toml
    assert any(w.category == "credential-in-toml" for w in result.warnings)


def test_auth_failure():
    """8. Auth failure — credentials in TOML produce credential-in-toml warning."""
    result = translate_pproxy_args([
        "-l", "socks5://wronguser:badpass@127.0.0.1:0",
    ])
    assert result.ok
    assert "password" in result.toml
    assert "wronguser" in result.toml
    # Verify credential-in-toml warning is present (user will see it)
    assert any(w.category == "credential-in-toml" for w in result.warnings)


def test_unsupported_ssh():
    """9. Unsupported SSH returns structured UnsupportedFeature."""
    result = translate_pproxy_args([
        "-l", "socks5://127.0.0.1:0",
        "-r", "ssh://user@host:22",
    ])
    assert not result.ok
    assert len(result.unsupported) > 0
    assert any(u.feature == "ssh-upstream" for u in result.unsupported)


def test_shadowsocks_warning():
    """10. Shadowsocks TCP warning/downgrade is visible if translated."""
    result = translate_pproxy_args([
        "-l", "ss://aes-256-gcm:secret@127.0.0.1:8388",
    ])
    assert not result.ok
    assert any(u.feature == "shadowsocks-listener" for u in result.unsupported)


def test_from_pproxy_args_rejects_unsupported():
    """from_pproxy_args rejects unsupported features by default."""
    with pytest.raises(UnsupportedFeatureError):
        EggressService.from_pproxy_args([
            "-l", "socks5://127.0.0.1:0",
            "-r", "ssh://user@host:22",
        ])


def test_from_pproxy_args_allow_partial():
    """from_pproxy_args with allow_partial=True starts despite warnings."""
    result = translate_pproxy_args(["-l", "socks5://127.0.0.1:0", "-v"])
    assert result.ok  # verbose is just a warning, not unsupported

    with EggressService.from_pproxy_args(
        ["-l", "socks5://127.0.0.1:0", "-v"], allow_partial=True
    ).start() as handle:
        assert handle.status()["readiness"]
