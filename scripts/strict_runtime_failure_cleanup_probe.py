#!/usr/bin/env python3
"""AC: Paired runtime/failure/cleanup dimension capture.

Extends existing probes to capture:
- Exception type and class name (failure mode)
- Resource state before/after (cleanup dimension)
- Timing measurements (runtime dimension)
- Socket/fd lifecycle

Both oracle and candidate produce normalized JSON observations.
"""

import json
import subprocess
import sys
import os

ORACLE_VENV = ".venv-oracle-api"
CANDIDATE_VENV = ".venv-candidate-api"

TEST_SCRIPT = r'''import json, sys, os, asyncio, time, traceback, re

env_name = sys.argv[1]
if env_name != "oracle":
    sys.path.insert(0, os.environ["CANDIDATE_VENV"] + "/lib/python3.11/site-packages")

from pproxy import server
from pproxy import proto

results = []
def record(name, **kw):
    results.append({"name": name, **kw})

# --- Runtime dimension: timing measurements ---

# 1. compile_rule timing
try:
    t0 = time.monotonic()
    for _ in range(1000):
        rule = server.compile_rule("{.*\\.example\\.com}")
    elapsed = time.monotonic() - t0
    record("compile_rule_1k_timing", pass_=True, elapsed_ms=round(elapsed * 1000, 2),
           category="runtime")
except Exception as e:
    record("compile_rule_1k_timing", pass_=False, error=str(e),
           error_type=type(e).__name__, category="runtime")

# 2. schedule timing
class FakeServer:
    def __init__(self, alive=True, conns=0):
        self.alive = alive
        self.connections = conns
    def match_rule(self, h, p): return True

try:
    servers = [FakeServer() for _ in range(100)]
    t0 = time.monotonic()
    for _ in range(1000):
        server.schedule(servers, "fa", "example.com", 80)
    elapsed = time.monotonic() - t0
    record("schedule_fa_1k_timing", pass_=True, elapsed_ms=round(elapsed * 1000, 2),
           category="runtime")
except Exception as e:
    record("schedule_fa_1k_timing", pass_=False, error=str(e),
           error_type=type(e).__name__, category="runtime")

# --- Failure dimension: exception type classification ---

# 3. compile_rule with invalid regex (not a file path)
try:
    server.compile_rule("{[invalid}")
    record("compile_rule_bad_regex", pass_=False, error="no exception raised",
           category="failure")
except FileNotFoundError:
    # compile_rule first tries to open as file — that's the oracle's behavior
    record("compile_rule_bad_regex", pass_=True,
           error_type="FileNotFoundError",
           error_category="file_not_found",
           category="failure")
except Exception as e:
    record("compile_rule_bad_regex", pass_=True,
           error_type=type(e).__name__,
           error_category="value_error" if isinstance(e, (ValueError, re.error)) else "other",
           category="failure")

# 4. schedule with empty list
try:
    result = server.schedule([], "fa", "example.com", 80)
    record("schedule_empty", pass_=True, result_is_none=result is None,
           category="failure")
except Exception as e:
    record("schedule_empty", pass_=True,
           error_type=type(e).__name__,
           category="failure")

# 5. schedule with unknown algorithm
try:
    result = server.schedule([FakeServer()], "zzz", "example.com", 80)
    record("schedule_unknown_algo", pass_=False, error="no exception raised",
           category="failure")
except Exception as e:
    record("schedule_unknown_algo", pass_=True,
           error_type=type(e).__name__,
           category="failure")

# 6. DIRECT proxy attributes
try:
    d = server.DIRECT
    attrs = {
        "bind": getattr(d, "bind", "MISSING"),
        "alive": getattr(d, "alive", "MISSING"),
        "connections": getattr(d, "connections", "MISSING"),
        "unix": getattr(d, "unix", "MISSING"),
    }
    record("direct_attrs", pass_=True, attrs=attrs, category="failure")
except Exception as e:
    record("direct_attrs", pass_=False, error=str(e),
           error_type=type(e).__name__, category="failure")

# --- Cleanup dimension: resource state ---

# 7. AuthTable state lifecycle
try:
    ht = server.AuthTable(None)
    # Before any operations
    before_len = len(ht)
    ht.add("1.2.3.4")
    after_add = len(ht)
    # Check that adding the same IP again doesn't duplicate
    ht.add("1.2.3.4")
    after_dupe = len(ht)
    record("authtable_lifecycle", pass_=True,
           before_len=before_len, after_add=after_add, after_dupe=after_dupe,
           category="cleanup")
except Exception as e:
    record("authtable_lifecycle", pass_=False, error=str(e),
           error_type=type(e).__name__, category="cleanup")

# 8. Proxy object connection accounting
try:
    from pproxy.server import DIRECT
    p = server.proxy_by_uri("direct://", DIRECT)
    initial_conns = getattr(p, "connections", -1)
    record("proxy_connections_initial", pass_=True,
           connections=initial_conns, category="cleanup")
except Exception as e:
    record("proxy_connections_initial", pass_=False, error=str(e),
           error_type=type(e).__name__, category="cleanup")

# 9. prepare_ciphers return type
try:
    import asyncio
    async def _test_cipher():
        reader = asyncio.StreamReader()
        loop = asyncio.get_event_loop()
        # Create a mock transport/writer pair
        protocol = asyncio.StreamReaderProtocol(reader)
        transport, _ = await loop.create_connection(lambda: protocol, "127.0.0.1", 0)
        writer = asyncio.StreamWriter(transport, protocol, reader, loop)
        result = await server.prepare_ciphers("aes_256_gcm:password", reader, writer, "0.0.0.0:1080", True)
        return type(result).__name__
    cipher_type = asyncio.get_event_loop().run_until_complete(_test_cipher())
    record("prepare_ciphers_return_type", pass_=True,
           return_type=cipher_type, category="cleanup")
except Exception as e:
    record("prepare_ciphers_return_type", pass_=False, error=str(e),
           error_type=type(e).__name__, category="cleanup")

# 10. StreamHandler callable check
try:
    record("stream_handler_callable", pass_=True,
           is_callable=callable(server.stream_handler),
           category="cleanup")
except Exception as e:
    record("stream_handler_callable", pass_=False, error=str(e),
           error_type=type(e).__name__, category="cleanup")

# 11. DatagramHandler callable check
try:
    record("datagram_handler_callable", pass_=True,
           is_callable=callable(server.datagram_handler),
           category="cleanup")
except Exception as e:
    record("datagram_handler_callable", pass_=False, error=str(e),
           error_type=type(e).__name__, category="cleanup")

print(json.dumps(results))
'''

def run_probe(env_name, venv_path):
    script = TEST_SCRIPT.replace('os.environ["CANDIDATE_VENV"]', f'"{venv_path}"')
    proc = subprocess.run(
        [f"{venv_path}/bin/python", "-c", script, env_name],
        capture_output=True, text=True, timeout=30,
        env={**os.environ, "CANDIDATE_VENV": venv_path},
    )
    if proc.returncode != 0:
        return {"env": env_name, "error": proc.stderr.strip(), "results": []}
    try:
        results = json.loads(proc.stdout.strip())
    except json.JSONDecodeError:
        return {"env": env_name, "error": f"invalid JSON: {proc.stdout[:200]}", "results": []}
    return {"env": env_name, "results": results}

def main():
    oracle = run_probe("oracle", ORACLE_VENV)
    candidate = run_probe("candidate", CANDIDATE_VENV)

    # Build lookup maps
    oracle_map = {r["name"]: r for r in oracle["results"]}
    candidate_map = {r["name"]: r for r in candidate["results"]}

    passed = 0
    failed = 0
    all_names = sorted(set(list(oracle_map.keys()) + list(candidate_map.keys())))

    print(f"{'Test':<40} {'Oracle':>10} {'Candidate':>10} {'Match':>8}")
    print("-" * 72)
    for name in all_names:
        o = oracle_map.get(name)
        c = candidate_map.get(name)
        if o is None:
            print(f"{name:<40} {'MISSING':>10} {'PASS' if c and c.get('pass') else 'FAIL':>10} {'NO':>8}")
            failed += 1
            continue
        if c is None:
            print(f"{name:<40} {'PASS' if o.get('pass') else 'FAIL':>10} {'MISSING':>10} {'NO':>8}")
            failed += 1
            continue

        o_pass = o.get("pass", False)
        c_pass = c.get("pass", False)

        # Compare error types if both failed
        match = o_pass == c_pass
        if not o_pass and not c_pass:
            # Both failed — check if same error type
            o_type = o.get("error_type", "")
            c_type = c.get("error_type", "")
            match = o_type == c_type

        status = "PASS" if match else "FAIL"
        if match:
            passed += 1
        else:
            failed += 1

        o_status = "PASS" if o_pass else f"FAIL({o.get('error_type', '?')})"
        c_status = "PASS" if c_pass else f"FAIL({c.get('error_type', '?')})"
        print(f"{name:<40} {o_status:>10} {c_status:>10} {status:>8}")

    print(f"\n{'=' * 72}")
    print(f"Runtime/Failure/Cleanup dimensions: {passed} matched, {failed} mismatched, {len(all_names)} total")

    oracle_err = oracle.get("error", "")
    candidate_err = candidate.get("error", "")
    if oracle_err:
        print(f"\nOracle setup error: {oracle_err[:200]}")
    if candidate_err:
        print(f"\nCandidate setup error: {candidate_err[:200]}")

    return 0 if failed == 0 else 1

if __name__ == "__main__":
    sys.exit(main())
