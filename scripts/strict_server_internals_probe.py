#!/usr/bin/env python3
"""AC8: Paired server internals tests.

Validates that oracle and candidate pproxy.server implementations
produce identical behavior for constants, helpers, and core functions.
"""

import json
import subprocess
import sys
import os

ORACLE_VENV = ".venv-oracle-api"
CANDIDATE_VENV = ".venv-candidate-api"

# This script is run as: python3 -c <script> <env_name>
TEST_SCRIPT = '''import json, sys, os, asyncio, re, time, random

env_name = sys.argv[1]
if env_name != "oracle":
    sys.path.insert(0, os.environ["CANDIDATE_VENV"] + "/lib/python3.11/site-packages")

from pproxy import server
from pproxy import proto

results = []
def ok(n): results.append({"name": n, "pass": True})
def fail(n, e): results.append({"name": n, "pass": False, "error": str(e)})

# Constants
try:
    assert server.SOCKET_TIMEOUT == 60
    ok("SOCKET_TIMEOUT == 60")
except Exception as e: fail("SOCKET_TIMEOUT == 60", e)

try:
    assert server.UDP_LIMIT == 30
    ok("UDP_LIMIT == 30")
except Exception as e: fail("UDP_LIMIT == 30", e)

try:
    assert callable(server.DUMMY) and server.DUMMY("x") == "x" and server.DUMMY(1) == 1
    ok("DUMMY is identity")
except Exception as e: fail("DUMMY is identity", e)

try:
    from pproxy.server import DIRECT, ProxyDirect
    assert isinstance(DIRECT, ProxyDirect) and DIRECT.bind == "DIRECT" and DIRECT.alive is True and DIRECT.connections == 0 and DIRECT.udpmap == {}
    ok("DIRECT correct defaults")
except Exception as e: fail("DIRECT correct defaults", e)

try:
    assert isinstance(server.sslcontexts, list)
    ok("sslcontexts is list")
except Exception as e: fail("sslcontexts is list", e)

# compile_rule
try:
    rule = server.compile_rule("{.*\\\\.example\\\\.com}")
    assert callable(rule) and rule("www.example.com") is not None and rule("evil.com") is None
    ok("compile_rule inline regex")
except Exception as e: fail("compile_rule inline regex", e)

try:
    rule = server.compile_rule("{.*\\\\.com}")
    m = rule("test.com")
    assert m is not None and type(m).__name__ == "Match"
    ok("compile_rule returns re.Match")
except Exception as e: fail("compile_rule returns re.Match", e)

# schedule
class FS:
    def __init__(self, alive=True, conns=0):
        self.alive = alive
        self.connections = conns
    def match_rule(self, h, p): return True

try:
    r = server.schedule([FS(), FS(), FS()], "fa", "example.com", 80)
    assert r is not None and r.alive
    ok("schedule fa first alive")
except Exception as e: fail("schedule fa first alive", e)

try:
    r = server.schedule([FS(False), FS(True)], "fa", "example.com", 80)
    assert r is not None and r.alive
    ok("schedule fa skips dead")
except Exception as e: fail("schedule fa skips dead", e)

try:
    r = server.schedule([FS(False), FS(False)], "fa", "example.com", 80)
    assert r is None
    ok("schedule fa None when all dead")
except Exception as e: fail("schedule fa None when all dead", e)

try:
    class NAMED:
        def __init__(self, n):
            self.name = n; self.alive = True; self.connections = 0
        def match_rule(self, h, p): return True
    s = [NAMED("a"), NAMED("b"), NAMED("c")]
    r1 = server.schedule(s, "rr", "e", 80)
    # After first rr: a moved to end, list = [b, c, a]
    assert r1.name == "a" and s[0].name == "b"
    ok("schedule rr mutates")
except Exception as e: fail("schedule rr mutates", e)

try:
    r = server.schedule([FS(), FS(), FS()], "rc", "e", 80)
    assert r is not None
    ok("schedule rc")
except Exception as e: fail("schedule rc", e)

try:
    r = server.schedule([FS(conns=5), FS(conns=1), FS(conns=3)], "lc", "e", 80)
    assert r is not None and r.connections == 1
    ok("schedule lc")
except Exception as e: fail("schedule lc", e)

try:
    server.schedule([], "unknown", "e", 80)
    assert False
except Exception:
    ok("schedule unknown raises")

# patch_StreamReader / patch_StreamWriter
try:
    import asyncio
    assert hasattr(asyncio.StreamReader, "read_w")
    assert hasattr(asyncio.StreamReader, "read_n")
    assert hasattr(asyncio.StreamReader, "read_until")
    assert hasattr(asyncio.StreamReader, "rollback")
    assert hasattr(asyncio.StreamWriter, "is_closing")
    ok("class-level patching")
except Exception as e: fail("class-level patching", e)

# AuthTable
try:
    from pproxy.server import AuthTable
    at = AuthTable("1.2.3.4", 3600)
    assert at.remote_ip == "1.2.3.4" and at.authtime == 3600
    assert at.authed() is None
    at.set_authed("user1")
    assert at.authed() == "user1"
    ok("AuthTable basic")
except Exception as e: fail("AuthTable basic", e)

try:
    from pproxy.server import AuthTable
    at = AuthTable("1.2.3.4", 0)
    at.set_authed("user1")
    time.sleep(0.01)
    assert at.authed() is None
    ok("AuthTable expiry")
except Exception as e: fail("AuthTable expiry", e)

# proxies_by_uri / proxy_by_uri
try:
    from pproxy.server import proxies_by_uri, ProxyDirect
    r = proxies_by_uri("direct://")
    assert isinstance(r, ProxyDirect)
    ok("proxies_by_uri direct")
except Exception as e: fail("proxies_by_uri direct", e)

try:
    from pproxy.server import proxies_by_uri, ProxySimple, ProxyDirect
    r = proxies_by_uri("socks5://h:1080#u:p__direct://")
    assert isinstance(r, ProxySimple) and isinstance(r.jump, ProxyDirect)
    ok("proxies_by_uri chain")
except Exception as e: fail("proxies_by_uri chain", e)

# ProxyDirect
try:
    from pproxy.server import ProxyDirect
    a, b = ProxyDirect(), ProxyDirect()
    assert a != b and hash(a) != hash(b)
    ok("ProxyDirect identity equality")
except Exception as e: fail("ProxyDirect identity equality", e)

try:
    from pproxy.server import ProxyDirect
    d = ProxyDirect()
    assert d.direct is True and d.match_rule("x", 80) is True
    assert d.logtext("ex.com", 80) == " -> ex.com:80" and d.logtext("tunnel", 80) == ""
    assert d.destination("h", 80) == ("h", 80)
    d.connection_change(1); assert d.connections == 1
    d.connection_change(-1); assert d.connections == 0
    ok("ProxyDirect properties")
except Exception as e: fail("ProxyDirect properties", e)

# prepare_ciphers
try:
    async def t():
        from pproxy.server import prepare_ciphers
        r = await prepare_ciphers(None, None, None)
        assert r == (None, None)
    asyncio.run(t())
    ok("prepare_ciphers no cipher")
except Exception as e: fail("prepare_ciphers no cipher", e)

# async functions
try:
    import inspect
    from pproxy.server import check_server_alive, stream_handler, datagram_handler
    assert inspect.iscoroutinefunction(check_server_alive)
    assert inspect.iscoroutinefunction(stream_handler)
    assert inspect.iscoroutinefunction(datagram_handler)
    ok("handler functions are async")
except Exception as e: fail("handler functions are async", e)

try:
    from pproxy.server import main
    assert callable(main)
    ok("main is callable")
except Exception as e: fail("main is callable", e)

# ProxySimple
try:
    from pproxy.server import proxies_by_uri, ProxySimple
    ps = proxies_by_uri("socks5://h:1080#user:pass")
    assert isinstance(ps, ProxySimple)
    assert ps.bind == "h:1080" and ps.host_name == "h" and ps.port == 1080
    assert ps.users == [b"user:pass"] and ps.direct is False
    assert ps.rproto.name == "socks5" and ps.auth == b"user:pass"
    ok("ProxySimple attributes")
except Exception as e: fail("ProxySimple attributes", e)

try:
    from pproxy.server import proxies_by_uri
    ps = proxies_by_uri("socks5://h:1080?{.*example\\.com}")
    assert ps.match_rule("www.example.com", 80), "should match"
    assert not ps.match_rule("evil.com", 80), "should not match"
    ok("ProxySimple match_rule")
except Exception as e: fail("ProxySimple match_rule", e)

try:
    from pproxy.server import proxies_by_uri
    ps = proxies_by_uri("socks5://h:1080")
    assert ps.match_rule("any", 80) is True
    ok("ProxySimple no rule always True")
except Exception as e: fail("ProxySimple no rule always True", e)

print(json.dumps(results))
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
    print("Running AC8 paired server internals tests...")
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
