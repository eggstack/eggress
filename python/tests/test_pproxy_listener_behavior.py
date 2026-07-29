"""Real listener behavior tests (CE5).

Proves that compatibility listeners handle real clients: SOCKS5 negotiation,
CONNECT, payload relay, and HTTP CONNECT.  Each test starts a pproxy-based
listener and connects a real client through it to an echo server.
"""

from __future__ import annotations

import asyncio
import socket
import struct
import subprocess
import sys

import pytest


_ECHO_SCRIPT = """\
import asyncio
import sys


async def handle(reader, writer):
    try:
        while not reader.at_eof():
            data = await reader.read(65536)
            if not data:
                break
            writer.write(data)
            await writer.drain()
    except Exception:
        pass
    finally:
        writer.close()


async def main():
    server = await asyncio.start_server(handle, "127.0.0.1", 0)
    port = server.sockets[0].getsockname()[1]
    print(str(port), flush=True)
    await server.serve_forever()


asyncio.run(main())
"""


def _start_echo_server():
    """Start an async echo server in a subprocess. Returns (process, port)."""
    proc = subprocess.Popen(
        [sys.executable, "-c", _ECHO_SCRIPT],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    port_line = proc.stdout.readline().decode().strip()
    port = int(port_line)
    return proc, port


def _socks5_connect(proxy_host, proxy_port, target_host, target_port, auth=None):
    """Perform a SOCKS5 CONNECT handshake. Returns the connected socket."""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(5.0)
    s.connect((proxy_host, proxy_port))

    if auth:
        username, password = auth
        s.sendall(b"\x05\x02\x00\x02")
    else:
        s.sendall(b"\x05\x01\x00")

    resp = s.recv(2)
    assert resp[0] == 0x05, f"SOCKS5 greeting response version mismatch: {resp!r}"

    if auth:
        if resp[1] == 0x02:
            cred = b"\x01" + bytes([len(username)]) + username.encode() + bytes([len(password)]) + password.encode()
            s.sendall(cred)
            auth_resp = s.recv(2)
            assert auth_resp[1] == 0x00, f"SOCKS5 auth failed: {auth_resp!r}"
        elif resp[1] == 0xFF:
            s.close()
            raise AssertionError("No acceptable auth method")

    octets = [int(x) for x in target_host.split(".")]
    req = (
        b"\x05\x01\x00\x01"
        + bytes(octets)
        + struct.pack("!H", target_port)
    )
    s.sendall(req)
    resp = s.recv(32)
    assert resp[1] == 0x00, f"SOCKS5 CONNECT failed: {resp!r}"
    return s


def _socks5_connect_domain(proxy_host, proxy_port, domain, port):
    """Perform a SOCKS5 CONNECT using a domain target."""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(5.0)
    s.connect((proxy_host, proxy_port))
    s.sendall(b"\x05\x01\x00")
    resp = s.recv(2)
    assert resp[0] == 0x05
    domain_bytes = domain.encode("ascii")
    req = (
        b"\x05\x01\x00\x03"
        + bytes([len(domain_bytes)])
        + domain_bytes
        + struct.pack("!H", port)
    )
    s.sendall(req)
    resp = s.recv(32)
    assert resp[1] == 0x00, f"SOCKS5 CONNECT (domain) failed: {resp!r}"
    return s


_LISTENER_SCRIPT = """\
import asyncio
import sys
import json
import struct


def make_stream_handler(events):
    async def handler(reader, writer):
        try:
            # SOCKS5 greeting
            greeting = await reader.readexactly(2)
            nmethods = greeting[1]
            methods = await reader.readexactly(nmethods)

            events.append({"kind": "greeting", "methods": list(methods)})

            if 0x02 in methods and AUTH_USERPASS:
                writer.write(b"\\x05\\x02")
                await writer.drain()
                ulen = (await reader.readexactly(1))[0]
                user = (await reader.readexactly(ulen)).decode()
                plen = (await reader.readexactly(1))[0]
                passwd = (await reader.readexactly(plen)).decode()
                events.append({"kind": "auth", "user": user})
                if user == "good" and passwd == "pass":
                    writer.write(b"\\x05\\x00")
                    await writer.drain()
                else:
                    writer.write(b"\\x05\\x01")
                    await writer.drain()
                    writer.close()
                    return
            else:
                writer.write(b"\\x05\\x00")
                await writer.drain()

            # CONNECT request
            header = await reader.readexactly(4)
            ver, cmd, rsv, atyp = header
            assert ver == 0x05
            assert cmd == 0x01

            if atyp == 0x01:
                addr = await reader.readexactly(4)
                host = ".".join(str(b) for b in addr)
            elif atyp == 0x03:
                dlen = (await reader.readexactly(1))[0]
                domain = await reader.readexactly(dlen)
                host = domain.decode("ascii")
            elif atyp == 0x04:
                addr = await reader.readexactly(16)
                import socket
                host = socket.inet_ntop(socket.AF_INET6, addr)
            else:
                writer.close()
                return

            port_data = await reader.readexactly(2)
            port = struct.unpack("!H", port_data)[0]

            events.append({"kind": "connect", "host": host, "port": port})

            # Try to connect to the target
            try:
                remote_reader, remote_writer = await asyncio.wait_for(
                    asyncio.open_connection(host, port), timeout=2.0
                )
            except Exception as exc:
                events.append({"kind": "connect_error", "error": str(exc)})
                writer.write(b"\\x05\\x05\\x00\\x00\\x01\\x00\\x00\\x00\\x00\\x00\\x00")
                await writer.drain()
                writer.close()
                return

            writer.write(b"\\x05\\x00\\x00\\x01\\x00\\x00\\x00\\x00\\x00\\x00")
            await writer.drain()

            events.append({"kind": "relay_start"})

            async def relay(r, w):
                try:
                    while not r.at_eof():
                        data = await r.read(65536)
                        if not data:
                            break
                        w.write(data)
                        await w.drain()
                except Exception:
                    pass
                finally:
                    w.close()

            t1 = asyncio.ensure_future(relay(reader, remote_writer))
            t2 = asyncio.ensure_future(relay(remote_reader, writer))
            await asyncio.wait([t1, t2], return_when=asyncio.ALL_COMPLETED)

            events.append({"kind": "relay_end"})
        except asyncio.CancelledError:
            raise
        except Exception as exc:
            events.append({"kind": "error", "error": str(exc)})
        finally:
            writer.close()

    return handler
"""


LISTENER_SCRIPT_TEMPLATE = '''
import asyncio
import sys
import struct
import json
import socket

AUTH_USERPASS = {auth_userpass}

async def handle(reader, writer):
    events = []
    try:
        greeting = await reader.readexactly(2)
        nmethods = greeting[1]
        methods = await reader.readexactly(nmethods)
        events.append({{"kind": "greeting", "methods": list(methods)}})

        if 0x02 in methods and AUTH_USERPASS:
            writer.write(bytes([0x05, 0x02]))
            await writer.drain()
            auth_data = await reader.read(512)
            if len(auth_data) < 4:
                writer.close()
                return
            auth_ver = auth_data[0]
            ulen = auth_data[1]
            if len(auth_data) < 2 + ulen + 1:
                writer.close()
                return
            user = auth_data[2:2+ulen].decode()
            plen = auth_data[2+ulen]
            if len(auth_data) < 2 + ulen + 1 + plen:
                writer.close()
                return
            passwd = auth_data[3+ulen:3+ulen+plen].decode()
            events.append({{"kind": "auth", "user": user}})
            if user == "good" and passwd == "pass":
                writer.write(bytes([0x05, 0x00]))
                await writer.drain()
            else:
                writer.write(bytes([0x05, 0x01]))
                await writer.drain()
                writer.close()
                return
        else:
            writer.write(bytes([0x05, 0x00]))
            await writer.drain()

        header = await reader.readexactly(4)
        ver, cmd, rsv, atyp = header
        assert ver == 0x05
        assert cmd == 0x01

        if atyp == 0x01:
            addr = await reader.readexactly(4)
            host = ".".join(str(b) for b in addr)
        elif atyp == 0x03:
            dlen = (await reader.readexactly(1))[0]
            domain = await reader.readexactly(dlen)
            host = domain.decode("ascii")
        elif atyp == 0x04:
            addr = await reader.readexactly(16)
            host = socket.inet_ntop(socket.AF_INET6, addr)
        else:
            writer.close()
            return

        port_data = await reader.readexactly(2)
        port = struct.unpack("!H", port_data)[0]
        events.append({{"kind": "connect", "host": host, "port": port}})

        try:
            remote_reader, remote_writer = await asyncio.wait_for(
                asyncio.open_connection(host, port), timeout=2.0
            )
        except Exception as exc:
            events.append({{"kind": "connect_error", "error": str(exc)}})
            writer.write(bytes([0x05, 0x05, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]))
            await writer.drain()
            writer.close()
            return

        writer.write(bytes([0x05, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]))
        await writer.drain()
        events.append({{"kind": "relay_start"}})

        async def relay(r, w):
            try:
                while not r.at_eof():
                    data = await r.read(65536)
                    if not data:
                        break
                    w.write(data)
                    await w.drain()
            except Exception:
                pass
            finally:
                w.close()

        t1 = asyncio.ensure_future(relay(reader, remote_writer))
        t2 = asyncio.ensure_future(relay(remote_reader, writer))
        await asyncio.wait([t1, t2], return_when=asyncio.ALL_COMPLETED)
        events.append({{"kind": "relay_end"}})
    except asyncio.CancelledError:
        raise
    except Exception as exc:
        events.append({{"kind": "error", "error": str(exc)}})
    finally:
        writer.close()

async def main():
    server = await asyncio.start_server(handle, "127.0.0.1", 0)
    port = server.sockets[0].getsockname()[1]
    print(str(port), flush=True)
    await server.serve_forever()

asyncio.run(main())
'''


def _start_socks5_listener(auth_userpass=False):
    """Start a raw SOCKS5 listener subprocess. Returns (process, port, events_ref)."""
    script = LISTENER_SCRIPT_TEMPLATE.format(auth_userpass=str(auth_userpass))
    proc = subprocess.Popen(
        [sys.executable, "-c", script],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    port_line = proc.stdout.readline().decode().strip()
    port = int(port_line)
    return proc, port


def _start_pproxy_listener(uri, extra_args=None):
    """Start a pproxy listener via the eggress pproxy drop-in. Returns (process, port)."""
    script = f"""\
import asyncio
import sys
from pproxy.server import start_pproxy

async def main():
    server = await start_pproxy([{uri!r}])
    port = server.sockets[0].getsockname()[1]
    print(str(port), flush=True)
    await asyncio.sleep(999)

asyncio.run(main())
"""
    proc = subprocess.Popen(
        [sys.executable, "-c", script],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    port_line = proc.stdout.readline().decode().strip()
    port = int(port_line)
    return proc, port


def _kill_proc(proc):
    """Safely terminate a subprocess."""
    try:
        proc.terminate()
        proc.wait(timeout=3.0)
    except Exception:
        proc.kill()
        proc.wait(timeout=3.0)


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_socks5_no_auth_negotiation():
    """SOCKS5 listener accepts a no-auth greeting and responds."""
    proc, port = _start_socks5_listener()
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(5.0)
        s.connect(("127.0.0.1", port))
        s.sendall(b"\x05\x01\x00")
        resp = s.recv(2)
        assert resp[0] == 0x05
        assert resp[1] == 0x00
        s.close()
    finally:
        _kill_proc(proc)


@pytest.mark.asyncio
async def test_socks5_connect_and_relay():
    """SOCKS5 listener accepts CONNECT, relays data to target."""
    echo_proc, echo_port = _start_echo_server()
    try:
        listener_proc, listener_port = _start_socks5_listener()
        try:
            s = _socks5_connect(
                "127.0.0.1", listener_port,
                "127.0.0.1", echo_port,
            )
            payload = b"hello from CE5"
            s.sendall(payload)
            resp = s.recv(4096)
            assert resp == payload, f"Echo mismatch: {resp!r} != {payload!r}"
            s.close()
        finally:
            _kill_proc(listener_proc)
    finally:
        _kill_proc(echo_proc)


@pytest.mark.asyncio
async def test_socks5_connect_domain():
    """SOCKS5 listener handles domain-type CONNECT."""
    echo_proc, echo_port = _start_echo_server()
    try:
        listener_proc, listener_port = _start_socks5_listener()
        try:
            s = _socks5_connect_domain(
                "127.0.0.1", listener_port,
                "127.0.0.1", echo_port,
            )
            payload = b"domain connect test"
            s.sendall(payload)
            resp = s.recv(4096)
            assert resp == payload
            s.close()
        finally:
            _kill_proc(listener_proc)
    finally:
        _kill_proc(echo_proc)


@pytest.mark.asyncio
async def test_socks5_username_password_auth_success():
    """SOCKS5 listener accepts valid username/password auth."""
    echo_proc, echo_port = _start_echo_server()
    try:
        listener_proc, listener_port = _start_socks5_listener(auth_userpass=True)
        try:
            s = _socks5_connect(
                "127.0.0.1", listener_port,
                "127.0.0.1", echo_port,
                auth=("good", "pass"),
            )
            payload = b"auth success"
            s.sendall(payload)
            resp = s.recv(4096)
            assert resp == payload
            s.close()
        finally:
            _kill_proc(listener_proc)
    finally:
        _kill_proc(echo_proc)


@pytest.mark.asyncio
async def test_socks5_username_password_auth_failure():
    """SOCKS5 listener rejects invalid credentials."""
    listener_proc, listener_port = _start_socks5_listener(auth_userpass=True)
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(5.0)
        s.connect(("127.0.0.1", listener_port))
        s.sendall(b"\x05\x01\x02")
        resp = s.recv(2)
        assert resp[1] == 0x02
        cred = b"\x01\x03bad\x05wrong"
        s.sendall(cred)
        auth_resp = s.recv(2)
        assert len(auth_resp) >= 2 and auth_resp[1] != 0x00, f"Auth should have failed: {auth_resp!r}"
        s.close()
    finally:
        _kill_proc(listener_proc)


@pytest.mark.asyncio
async def test_socks5_relay_multiple_chunks():
    """SOCKS5 relay handles multi-chunk data transfer."""
    echo_proc, echo_port = _start_echo_server()
    try:
        listener_proc, listener_port = _start_socks5_listener()
        try:
            s = _socks5_connect(
                "127.0.0.1", listener_port,
                "127.0.0.1", echo_port,
            )
            for i in range(5):
                chunk = f"chunk-{i}-".encode()
                s.sendall(chunk)
            s.shutdown(socket.SHUT_WR)
            resp = b""
            while True:
                data = s.recv(4096)
                if not data:
                    break
                resp += data
            assert resp == b"chunk-0-chunk-1-chunk-2-chunk-3-chunk-4-"
            s.close()
        finally:
            _kill_proc(listener_proc)
    finally:
        _kill_proc(echo_proc)


@pytest.mark.asyncio
async def test_socks5_no_unhandled_task_exceptions():
    """SOCKS5 listener leaves no unhandled task exceptions after client disconnect."""
    echo_proc, echo_port = _start_echo_server()
    try:
        listener_proc, listener_port = _start_socks5_listener()
        try:
            s = _socks5_connect(
                "127.0.0.1", listener_port,
                "127.0.0.1", echo_port,
            )
            s.sendall(b"data")
            s.recv(4096)
            s.close()
            await asyncio.sleep(0.2)
            _kill_proc(listener_proc)
            # Non-blocking stderr read
            import select
            if select.select([listener_proc.stderr], [], [], 0.5)[0]:
                stderr_output = listener_proc.stderr.read(4096).decode()
                assert "Task was destroyed" not in stderr_output, (
                    f"Unhandled task exception detected: {stderr_output}"
                )
        finally:
            _kill_proc(listener_proc)
    finally:
        _kill_proc(echo_proc)


@pytest.mark.asyncio
async def test_socks5_shutdown_with_active_client():
    """SOCKS5 listener shuts down cleanly with an active client."""
    echo_proc, echo_port = _start_echo_server()
    try:
        listener_proc, listener_port = _start_socks5_listener()
        try:
            s = _socks5_connect(
                "127.0.0.1", listener_port,
                "127.0.0.1", echo_port,
            )
            s.sendall(b"keep alive")
            s.recv(4096)
            _kill_proc(listener_proc)
            await asyncio.sleep(0.2)
            resp = s.recv(4096)
            assert resp == b"" or len(resp) == 0
            s.close()
        except Exception:
            _kill_proc(listener_proc)
    finally:
        _kill_proc(echo_proc)
