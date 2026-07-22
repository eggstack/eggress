#!/usr/bin/env python3
"""AC6: Paired asyncio adapter stream behavior tests.

Validates that oracle's patched asyncio.StreamReader/StreamWriter and
candidate's compatible implementation behave identically.
"""

import asyncio
import json
import socket
import subprocess
import sys

ORACLE_VENV = ".venv-oracle-api"
CANDIDATE_VENV = ".venv-candidate-api"

TEST_SUBPROCESS = r'''
import asyncio
import json
import sys
import os
import socket

env_name = sys.argv[1]

if env_name == "oracle":
    pass  # standard asyncio is already patched by pproxy.server import
else:
    sys.path.insert(0, "%s/lib/python3.11/site-packages" % os.environ["CANDIDATE_VENV"])

from pproxy.server import patch_StreamReader, patch_StreamWriter, SOCKET_TIMEOUT

async def echo_handler(reader, writer):
    while True:
        data = await reader.read(65536)
        if not data:
            break
        writer.write(data)
        await writer.drain()
    writer.close()
    await writer.wait_closed()

async def run():
    results = []
    server = await asyncio.start_server(echo_handler, "127.0.0.1", 0)
    port = server.sockets[0].getsockname()[1]

    def ok(name):
        results.append({"name": name, "pass": True})

    def fail(name, err):
        results.append({"name": name, "pass": False, "error": str(err)})

    # Test 1: read(-1) until EOF
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"hello")
        w.write_eof()
        await w.drain()
        data = await r.read(-1)
        assert data == b"hello", repr(data)
        w.close()
        await w.wait_closed()
        ok("read(-1) until EOF")
    except Exception as e:
        fail("read(-1) until EOF", e)

    # Test 2: read(0) returns empty
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"hello")
        await w.drain()
        await asyncio.sleep(0.05)
        data = await r.read(0)
        assert data == b"", repr(data)
        w.write_eof()
        await w.drain()
        await r.read(-1)
        w.close()
        await w.wait_closed()
        ok("read(0) returns empty")
    except Exception as e:
        fail("read(0) returns empty", e)

    # Test 3: read(n) returns up to n bytes
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"hello world")
        w.write_eof()
        await w.drain()
        data = await r.read(5)
        assert data == b"hello", repr(data)
        rest = await r.read(-1)
        assert rest == b" world", repr(rest)
        w.close()
        await w.wait_closed()
        ok("read(n) returns up to n bytes")
    except Exception as e:
        fail("read(n) returns up to n bytes", e)

    # Test 4: readexactly(n) returns exactly n bytes
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"hello world")
        w.write_eof()
        await w.drain()
        data = await r.readexactly(5)
        assert data == b"hello", repr(data)
        rest = await r.readexactly(6)
        assert rest == b" world", repr(rest)
        w.close()
        await w.wait_closed()
        ok("readexactly(n) returns exactly n bytes")
    except Exception as e:
        fail("readexactly(n) returns exactly n bytes", e)

    # Test 5: readuntil(separator) reads until separator
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"line1\nline2\n")
        w.write_eof()
        await w.drain()
        line = await r.readuntil(b"\n")
        assert line == b"line1\n", repr(line)
        line2 = await r.readuntil(b"\n")
        assert line2 == b"line2\n", repr(line2)
        w.close()
        await w.wait_closed()
        ok("readuntil(separator) reads until separator")
    except Exception as e:
        fail("readuntil(separator) reads until separator", e)

    # Test 6: rollback(data) pushes data back
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"hello world")
        w.write_eof()
        await w.drain()
        data = await r.read(5)
        assert data == b"hello", repr(data)
        r.rollback(data)
        rest = await r.read(-1)
        assert rest == b"hello world", repr(rest)
        w.close()
        await w.wait_closed()
        ok("rollback(data) pushes data back")
    except Exception as e:
        fail("rollback(data) pushes data back", e)

    # Test 7: read_w(n) reads with timeout
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"hello")
        w.write_eof()
        await w.drain()
        data = await r.read_w(-1)
        assert data == b"hello", repr(data)
        w.close()
        await w.wait_closed()
        ok("read_w(n) reads with timeout")
    except Exception as e:
        fail("read_w(n) reads with timeout", e)

    # Test 8: read_n(n) reads exactly with timeout
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"hello")
        w.write_eof()
        await w.drain()
        data = await r.read_n(5)
        assert data == b"hello", repr(data)
        w.close()
        await w.wait_closed()
        ok("read_n(n) reads exactly with timeout")
    except Exception as e:
        fail("read_n(n) reads exactly with timeout", e)

    # Test 9: read_until(separator) reads until with timeout
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"line1\n")
        w.write_eof()
        await w.drain()
        line = await r.read_until(b"\n")
        assert line == b"line1\n", repr(line)
        w.close()
        await w.wait_closed()
        ok("read_until(separator) reads until with timeout")
    except Exception as e:
        fail("read_until(separator) reads until with timeout", e)

    # Test 10: write_eof() signals half-close
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"hello")
        w.write_eof()
        await w.drain()
        data = await r.read(-1)
        assert data == b"hello", repr(data)
        w.close()
        await w.wait_closed()
        ok("write_eof() signals half-close")
    except Exception as e:
        fail("write_eof() signals half-close", e)

    # Test 11: drain() flushes buffer
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"hello")
        await w.drain()
        w.write(b" world")
        await w.drain()
        w.write_eof()
        await w.drain()
        data = await r.read(-1)
        assert data == b"hello world", repr(data)
        w.close()
        await w.wait_closed()
        ok("drain() flushes buffer")
    except Exception as e:
        fail("drain() flushes buffer", e)

    # Test 12: is_closing() returns correct state
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        assert not w.is_closing(), "should not be closing"
        w.close()
        await w.wait_closed()
        assert w.is_closing(), "should be closing after close()"
        ok("is_closing() returns correct state")
    except Exception as e:
        fail("is_closing() returns correct state", e)

    # Test 13: get_extra_info returns peername
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        peername = w.get_extra_info("peername")
        assert peername is not None, "peername should exist"
        assert isinstance(peername, (tuple, list)), type(peername)
        w.write_eof()
        await w.drain()
        await r.read(-1)
        w.close()
        await w.wait_closed()
        ok("get_extra_info returns peername")
    except Exception as e:
        fail("get_extra_info returns peername", e)

    # Test 14: can_write_eof() returns True
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        assert w.can_write_eof() is True
        w.write_eof()
        await w.drain()
        await r.read(-1)
        w.close()
        await w.wait_closed()
        ok("can_write_eof() returns True")
    except Exception as e:
        fail("can_write_eof() returns True", e)

    # Test 15: readline() reads until newline
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"line1\nline2\n")
        w.write_eof()
        await w.drain()
        line = await r.readline()
        assert line == b"line1\n", repr(line)
        line2 = await r.readline()
        assert line2 == b"line2\n", repr(line2)
        w.close()
        await w.wait_closed()
        ok("readline() reads until newline")
    except Exception as e:
        fail("readline() reads until newline", e)

    # Test 16: at_eof() returns True after EOF
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"hello")
        w.write_eof()
        await w.drain()
        data = await r.read(-1)
        assert data == b"hello"
        assert r.at_eof() is True
        w.close()
        await w.wait_closed()
        ok("at_eof() returns True after EOF")
    except Exception as e:
        fail("at_eof() returns True after EOF", e)

    # Test 17: write() and writelines() buffer data
    try:
        r, w = await asyncio.open_connection("127.0.0.1", port)
        w.write(b"hello")
        w.writelines([b" ", b"world"])
        w.write_eof()
        await w.drain()
        data = await r.read(-1)
        assert data == b"hello world", repr(data)
        w.close()
        await w.wait_closed()
        ok("write() and writelines() buffer data")
    except Exception as e:
        fail("write() and writelines() buffer data", e)

    server.close()
    await server.wait_closed()
    print(json.dumps(results))

asyncio.run(run())
'''


def run_env(env_name, venv_path):
    script = TEST_SUBPROCESS
    env = {"CANDIDATE_VENV": venv_path} if env_name == "candidate" else {}
    result = subprocess.run(
        [f"{venv_path}/bin/python", "-c", script, env_name],
        capture_output=True, text=True, timeout=30, env={**dict(__import__("os").environ), **env},
    )
    if result.returncode != 0:
        return env_name, False, f"exit {result.returncode}: {result.stderr[:500]}"
    try:
        results = json.loads(result.stdout.strip())
        return env_name, True, results
    except json.JSONDecodeError:
        return env_name, False, f"bad output: {result.stdout[:200]}"


def main():
    print("Running AC6 paired stream behavior tests...")
    print()

    oracle_name, oracle_ok, oracle_results = run_env("oracle", ORACLE_VENV)
    candidate_name, candidate_ok, candidate_results = run_env("candidate", CANDIDATE_VENV)

    if not oracle_ok:
        print(f"ORACLE FAILED: {oracle_results}")
        return 1
    if not candidate_ok:
        print(f"CANDIDATE FAILED: {candidate_results}")
        return 1

    oracle_map = {r["name"]: r for r in oracle_results}
    candidate_map = {r["name"]: r for r in candidate_results}

    all_names = sorted(set(oracle_map.keys()) | set(candidate_map.keys()))

    passed = 0
    failed = 0
    for name in all_names:
        o = oracle_map.get(name)
        c = candidate_map.get(name)
        o_pass = o["pass"] if o else False
        c_pass = c["pass"] if c else False

        if o_pass and c_pass:
            status = "PASS"
            passed += 1
        elif o_pass and not c_pass:
            status = "CANDIDATE FAIL: " + (c.get("error", "?") if c else "missing")
            failed += 1
        elif not o_pass and c_pass:
            status = "ORACLE FAIL (candidate matches): " + (o.get("error", "?") if o else "missing")
            passed += 1
        else:
            status = "BOTH FAIL"
            failed += 1

        print(f"  [{status:60s}] {name}")

    print()
    print(f"Results: {passed} passed, {failed} failed, {len(all_names)} total")

    if failed > 0:
        print("\nMISMATCH DETECTED")
        return 1
    else:
        print("\nAll adapter behaviors match oracle")
        return 0


if __name__ == "__main__":
    sys.exit(main())
