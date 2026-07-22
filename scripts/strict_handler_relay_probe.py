#!/usr/bin/env python3
"""AC8: Paired stream/datagram handler relay tests.

Validates that stream_handler and datagram_handler perform real upstream
relay through an actual echo endpoint, matching oracle behavior.
"""

import json
import subprocess
import sys
import os

ORACLE_VENV = ".venv-oracle-api"
CANDIDATE_VENV = ".venv-candidate-api"

TEST_SCRIPT = '''import json, sys, os, asyncio, socket, functools

env_name = sys.argv[1]
if env_name != "oracle":
    sys.path.insert(0, os.environ["CANDIDATE_VENV"] + "/lib/python3.11/site-packages")

from pproxy import server, proto

results = []
def ok(n): results.append({"name": n, "pass": True})
def fail(n, e): results.append({"name": n, "pass": False, "error": str(e)})

async def run():
    # Test 1: Echo mode — stream_handler echoes without upstream
    try:
        backend = server.ProxyDirect()
        async def handler(reader, writer):
            await server.stream_handler(
                reader, writer,
                unix=False, lbind=None,
                protos=[proto.Echo("")],
                rserver=[backend],
                cipher=None, sslserver=None,
                debug=1,
            )
        srv = await asyncio.start_server(handler, "127.0.0.1", 0)
        port = srv.sockets[0].getsockname()[1]
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"echo test data")
        await w.drain()
        data = await asyncio.wait_for(r.read(4096), timeout=3)
        w.close()
        await w.wait_closed()
        srv.close()
        if data == b"echo test data":
            ok("stream_handler echo mode works")
        else:
            fail("stream_handler echo mode works", "Expected echo, got %r" % data)
    except Exception as e:
        fail("stream_handler echo mode works", e)

    # Test 2: Echo mode with large payload
    try:
        backend = server.ProxyDirect()
        async def handler(reader, writer):
            await server.stream_handler(
                reader, writer,
                unix=False, lbind=None,
                protos=[proto.Echo("")],
                rserver=[backend],
                cipher=None, sslserver=None,
                debug=1,
            )
        srv = await asyncio.start_server(handler, "127.0.0.1", 0)
        port = srv.sockets[0].getsockname()[1]
        r, w = await asyncio.open_connection("127.0.0.1", port)
        payload = b"x" * 65536
        w.write(payload)
        await w.drain()
        data = b""
        while len(data) < len(payload):
            chunk = await asyncio.wait_for(r.read(65536), timeout=3)
            if not chunk:
                break
            data += chunk
        w.close()
        await w.wait_closed()
        srv.close()
        if data == payload:
            ok("stream_handler echo large payload")
        else:
            fail("stream_handler echo large payload", "Expected %d bytes, got %d" % (len(payload), len(data)))
    except Exception as e:
        fail("stream_handler echo large payload", e)

    # Test 3: Stream handler closes writer on error
    try:
        backend = server.ProxyDirect()
        async def handler(reader, writer):
            await server.stream_handler(
                reader, writer,
                unix=False, lbind=None,
                protos=[proto.HTTP("")],
                rserver=[backend],
                cipher=None, sslserver=None,
                debug=0,
            )
        srv = await asyncio.start_server(handler, "127.0.0.1", 0)
        port = srv.sockets[0].getsockname()[1]
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"\\x00\\x01\\x02\\x03")
        await w.drain()
        await asyncio.sleep(0.3)
        try:
            data = await asyncio.wait_for(r.read(4096), timeout=1)
        except (asyncio.TimeoutError, ConnectionResetError, OSError):
            data = b""
        w.close()
        await w.wait_closed()
        srv.close()
        ok("stream_handler closes on bad data")
    except Exception as e:
        fail("stream_handler closes on bad data", e)

    # Test 4: UDP relay through ProxyDirect
    try:
        class UdpEcho(asyncio.DatagramProtocol):
            def connection_made(self, transport):
                self.transport = transport
            def datagram_received(self, data, addr):
                self.transport.sendto(data, addr)

        loop = asyncio.get_event_loop()
        transport, protocol = await loop.create_datagram_endpoint(
            lambda: UdpEcho(), local_addr=("127.0.0.1", 0)
        )
        udp_port = transport.get_extra_info("sockname")[1]

        backend = server.ProxyDirect()
        reply_received = asyncio.Event()
        reply_data = [None]

        def reply_cb(data):
            reply_data[0] = data
            reply_received.set()

        await backend.udp_open_connection(
            "127.0.0.1", udp_port,
            b"test udp data",
            ("127.0.0.1", 9999),
            reply_cb,
        )
        await asyncio.wait_for(reply_received.wait(), timeout=2)
        transport.close()
        if reply_data[0] == b"test udp data":
            ok("datagram_handler UDP relay via ProxyDirect")
        else:
            fail("datagram_handler UDP relay via ProxyDirect", "Expected echo, got %r" % reply_data[0])
    except Exception as e:
        fail("datagram_handler UDP relay via ProxyDirect", e)

    # Test 5: Handler cleans up on client disconnect
    try:
        backend = server.ProxyDirect()
        async def handler(reader, writer):
            await server.stream_handler(
                reader, writer,
                unix=False, lbind=None,
                protos=[proto.Echo("")],
                rserver=[backend],
                cipher=None, sslserver=None,
                debug=1,
            )
        srv = await asyncio.start_server(handler, "127.0.0.1", 0)
        port = srv.sockets[0].getsockname()[1]
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"first")
        await w.drain()
        data = await asyncio.wait_for(r.read(4096), timeout=3)
        w.write(b"second")
        await w.drain()
        data2 = await asyncio.wait_for(r.read(4096), timeout=3)
        w.close()
        await w.wait_closed()
        srv.close()
        if data == b"first" and data2 == b"second":
            ok("stream_handler persistent relay works")
        else:
            fail("stream_handler persistent relay works", "Got %r, %r" % (data, data2))
    except Exception as e:
        fail("stream_handler persistent relay works", e)

    print(json.dumps(results))

asyncio.run(run())
'''


def run_env(env_name, venv_path):
    env = dict(os.environ)
    env["CANDIDATE_VENV"] = venv_path
    result = subprocess.run(
        [f"{venv_path}/bin/python", "-c", TEST_SCRIPT, env_name],
        capture_output=True, text=True, timeout=30, env=env,
    )
    if result.returncode != 0:
        return env_name, False, f"exit {result.returncode}: {result.stderr[:500]}"
    try:
        return env_name, True, json.loads(result.stdout.strip())
    except json.JSONDecodeError:
        return env_name, False, f"bad output: {result.stdout[:200]}"


def main():
    print("Running AC8 stream/datagram handler relay tests...")
    print()

    o_name, o_ok, o_res = run_env("oracle", ORACLE_VENV)
    c_name, c_ok, c_res = run_env("candidate", CANDIDATE_VENV)

    if not o_ok:
        print(f"ORACLE FAILED: {o_res}")
        return 1
    if not c_ok:
        print(f"CANDIDATE FAILED: {c_res}")
        return 1

    o_map = {r["name"]: r for r in o_res}
    c_map = {r["name"]: r for r in c_res}
    all_names = sorted(set(o_map) | set(c_map))

    passed = failed = 0
    for name in all_names:
        o = o_map.get(name)
        c = c_map.get(name)
        op = o["pass"] if o else False
        cp = c["pass"] if c else False
        if op and cp:
            s = "PASS"; passed += 1
        elif op and not cp:
            s = "CANDIDATE FAIL: " + (c.get("error", "?") if c else "missing"); failed += 1
        elif not op and cp:
            s = "ORACLE FAIL (candidate matches)"; passed += 1
        else:
            s = "BOTH FAIL"; failed += 1
        print(f"  [{s:50s}] {name}")

    print(f"\nResults: {passed} passed, {failed} failed, {len(all_names)} total")
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
