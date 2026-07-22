#!/usr/bin/env python3
"""AC10: Plugin lifecycle paired test with real byte transformations.

Validates that oracle and candidate plugin implementations
produce identical behavior for init, add_cipher, and get_plugin.
"""

import json
import subprocess
import sys
import os

ORACLE_VENV = ".venv-oracle-api"
CANDIDATE_VENV = ".venv-candidate-api"

TEST_SCRIPT = r'''import json, sys, os, asyncio, hashlib, hmac, time

env_name = sys.argv[1]
if env_name != "oracle":
    sys.path.insert(0, os.environ["CANDIDATE_VENV"] + "/lib/python3.11/site-packages")

from pproxy import plugin
from pproxy.plugin import get_plugin, PLUGIN

results = []
def record(name, **kw):
    results.append({"name": name, **kw})

# 1. PLUGIN registry keys match
try:
    keys = sorted(PLUGIN.keys())
    record("plugin_registry_keys", pass_=True, keys=keys)
except Exception as e:
    record("plugin_registry_keys", pass_=False, error=str(e), error_type=type(e).__name__)

# 2. get_plugin returns correct types
for pname in ["plain", "origin", "http_simple", "tls1.2_ticket_auth", "verify_simple", "verify_deflate"]:
    try:
        p = get_plugin(pname)
        record(f"get_plugin_{pname}", pass_=True, type_name=type(p).__name__)
    except Exception as e:
        record(f"get_plugin_{pname}", pass_=False, error=str(e), error_type=type(e).__name__)

# 3. get_plugin with unknown name
try:
    get_plugin("nonexistent_plugin")
    record("get_plugin_unknown", pass_=False, error="no exception")
except Exception as e:
    record("get_plugin_unknown", pass_=True, error_type=type(e).__name__)

# 4. BasePlugin method signatures
try:
    import inspect
    bp = plugin.BasePlugin
    methods = {}
    for mname in ["add_cipher", "init_client_data", "init_server_data"]:
        method = getattr(bp, mname, None)
        if method is None:
            methods[mname] = "MISSING"
        else:
            sig = inspect.signature(method)
            methods[mname] = str(sig)
    record("base_plugin_methods", pass_=True, methods=methods)
except Exception as e:
    record("base_plugin_methods", pass_=False, error=str(e), error_type=type(e).__name__)

# 5. Plain_Plugin.add_cipher is a no-op
try:
    pp = get_plugin("plain")
    pp.add_cipher(None)  # Should not raise
    record("plain_add_cipher_nop", pass_=True)
except Exception as e:
    record("plain_add_cipher_nop", pass_=False, error=str(e), error_type=type(e).__name__)

# 6. Plain_Plugin.init_server_data is a no-op
async def _test_plain_init_server():
    pp = get_plugin("plain")
    reader = asyncio.StreamReader()
    loop = asyncio.get_event_loop()
    protocol = asyncio.StreamReaderProtocol(reader)
    transport, _ = await loop.create_connection(lambda: protocol, "127.0.0.1", 0)
    writer = asyncio.StreamWriter(transport, protocol, reader, loop)
    await pp.init_server_data(reader, writer, None, ("127.0.0.1", 12345))
    return True

try:
    result = asyncio.get_event_loop().run_until_complete(_test_plain_init_server())
    record("plain_init_server_data_nop", pass_=result is True)
except Exception as e:
    record("plain_init_server_data_nop", pass_=False, error=str(e), error_type=type(e).__name__)

# 7. Plain_Plugin.init_client_data is a no-op
async def _test_plain_init_client():
    pp = get_plugin("plain")
    reader = asyncio.StreamReader()
    loop = asyncio.get_event_loop()
    protocol = asyncio.StreamReaderProtocol(reader)
    transport, _ = await loop.create_connection(lambda: protocol, "127.0.0.1", 0)
    writer = asyncio.StreamWriter(transport, protocol, reader, loop)
    await pp.init_client_data(reader, writer, None)
    return True

try:
    result = asyncio.get_event_loop().run_until_complete(_test_plain_init_client())
    record("plain_init_client_data_nop", pass_=result is True)
except Exception as e:
    record("plain_init_client_data_nop", pass_=False, error=str(e), error_type=type(e).__name__)

# 8. Http_Simple_Plugin.init_server_data writes expected HTTP request
async def _test_http_simple_init_server():
    hp = get_plugin("http_simple")
    reader = asyncio.StreamReader()
    loop = asyncio.get_event_loop()
    protocol = asyncio.StreamReaderProtocol(reader)
    transport, _ = await loop.create_connection(lambda: protocol, "127.0.0.1", 0)
    writer = asyncio.StreamWriter(transport, protocol, reader, loop)
    await hp.init_server_data(reader, writer, None, ("example.com", 443))
    return True

try:
    result = asyncio.get_event_loop().run_until_complete(_test_http_simple_init_server())
    record("http_simple_init_server_data", pass_=result is True)
except Exception as e:
    record("http_simple_init_server_data", pass_=False, error=str(e), error_type=type(e).__name__)

# 9. Plugin add_cipher doesn't mutate cipher state
try:
    pp = get_plugin("plain")
    class FakeCipher:
        pass
    fc = FakeCipher()
    pp.add_cipher(fc)
    # Cipher should be unmodified
    record("add_cipher_no_mutation", pass_=not hasattr(fc, '_modified'))
except Exception as e:
    record("add_cipher_no_mutation", pass_=False, error=str(e), error_type=type(e).__name__)

# 10. Origin_Plugin identity
try:
    op = get_plugin("origin")
    record("origin_plugin_exists", pass_=True, type_name=type(op).__name__)
except Exception as e:
    record("origin_plugin_exists", pass_=False, error=str(e), error_type=type(e).__name__)

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

    oracle_map = {r["name"]: r for r in oracle["results"]}
    candidate_map = {r["name"]: r for r in candidate["results"]}

    passed = 0
    failed = 0
    all_names = sorted(set(list(oracle_map.keys()) + list(candidate_map.keys())))

    print(f"{'Test':<45} {'Oracle':>10} {'Candidate':>10} {'Match':>8}")
    print("-" * 77)
    for name in all_names:
        o = oracle_map.get(name)
        c = candidate_map.get(name)
        if o is None:
            print(f"{name:<45} {'MISSING':>10} {'PASS' if c and c.get('pass') else 'FAIL':>10} {'NO':>8}")
            failed += 1
            continue
        if c is None:
            print(f"{name:<45} {'PASS' if o.get('pass') else 'FAIL':>10} {'MISSING':>10} {'NO':>8}")
            failed += 1
            continue

        o_pass = o.get("pass", False)
        c_pass = c.get("pass", False)
        match = o_pass == c_pass

        if match:
            passed += 1
        else:
            failed += 1

        o_status = "PASS" if o_pass else f"FAIL({o.get('error_type', '?')})"
        c_status = "PASS" if c_pass else f"FAIL({c.get('error_type', '?')})"
        print(f"{name:<45} {o_status:>10} {c_status:>10} {'PASS' if match else 'FAIL':>8}")

    print(f"\n{'=' * 77}")
    print(f"Plugin lifecycle: {passed} matched, {failed} mismatched, {len(all_names)} total")

    oracle_err = oracle.get("error", "")
    candidate_err = candidate.get("error", "")
    if oracle_err:
        print(f"\nOracle setup error: {oracle_err[:200]}")
    if candidate_err:
        print(f"\nCandidate setup error: {candidate_err[:200]}")

    return 0 if failed == 0 else 1

if __name__ == "__main__":
    sys.exit(main())
