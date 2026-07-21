#!/usr/bin/env python3
"""Executable oracle fixture: pproxy 2.7.9 comprehensive namespace tests.

Verifies all public modules, key symbols, type correctness, constants,
and behavioral invariants of the pproxy 2.7.9 namespace.

Provenance: Derived from pproxy API contract and namespace-baseline.json.
License: MIT (pproxy)
Tested with: pproxy==2.7.9 on Python 3.11
"""
import sys
import types
import pproxy
from pproxy import Connection, Server, DIRECT

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

def test_top_level_imports():
    print("test_top_level_imports")
    check("pproxy is module", isinstance(pproxy, types.ModuleType))
    check("pproxy.Connection exists", hasattr(pproxy, "Connection"))
    check("pproxy.Server exists", hasattr(pproxy, "Server"))
    check("pproxy.DIRECT exists", hasattr(pproxy, "DIRECT"))
    check("pproxy.Rule exists", hasattr(pproxy, "Rule"))

def test_connection_server_alias():
    print("test_connection_server_alias")
    check("Connection is Server", Connection is Server)
    check("Connection is pproxy.Connection", Connection is pproxy.Connection)
    check("Server is pproxy.Server", Server is pproxy.Server)

def test_direct_identity():
    print("test_direct_identity")
    from pproxy.server import DIRECT as SERVER_DIRECT
    check("pproxy.DIRECT is pproxy.server.DIRECT", DIRECT is SERVER_DIRECT)
    check("DIRECT is ProxyDirect", pproxy.DIRECT.__class__.__name__ == "ProxyDirect")

def test_pproxy_server_module():
    print("test_pproxy_server_module")
    import pproxy.server as srv
    check("pproxy.server is module", isinstance(srv, types.ModuleType))
    required = [
        "ProxyDirect", "ProxySimple", "ProxyBackward", "ProxyH2",
        "ProxyH3", "ProxyQUIC", "ProxySSH",
        "compile_rule", "check_server_alive",
        "stream_handler", "datagram_handler",
        "prepare_ciphers", "schedule", "main",
        "AuthTable", "SOCKET_TIMEOUT", "UDP_LIMIT",
        "DIRECT", "proxies_by_uri", "proxy_by_uri",
    ]
    for name in required:
        check(f"pproxy.server.{name} exists", hasattr(srv, name))

def test_pproxy_server_constants():
    print("test_pproxy_server_constants")
    import pproxy.server as srv
    check("SOCKET_TIMEOUT is int", isinstance(srv.SOCKET_TIMEOUT, int))
    check("UDP_LIMIT is int", isinstance(srv.UDP_LIMIT, int))
    check("SOCKET_TIMEOUT == 60", srv.SOCKET_TIMEOUT == 60)
    check("UDP_LIMIT == 30", srv.UDP_LIMIT == 30)

def test_pproxy_proto_module():
    print("test_pproxy_proto_module")
    import pproxy.proto as proto
    check("pproxy.proto is module", isinstance(proto, types.ModuleType))
    check("MAPPINGS exists", hasattr(proto, "MAPPINGS"))
    check("MAPPINGS is dict", isinstance(proto.MAPPINGS, dict))
    check("get_protos exists", hasattr(proto, "get_protos"))
    check("get_protos callable", callable(proto.get_protos))
    check("netloc_split exists", hasattr(proto, "netloc_split"))
    check("netloc_split callable", callable(proto.netloc_split))
    check("packstr exists", hasattr(proto, "packstr"))
    check("accept exists", hasattr(proto, "accept"))
    check("udp_accept exists", hasattr(proto, "udp_accept"))
    check("sslwrap exists", hasattr(proto, "sslwrap"))

def test_pproxy_proto_types():
    print("test_pproxy_proto_types")
    import pproxy.proto as proto
    for name in ["HTTP", "Socks5", "Socks4", "SS", "Trojan", "H2",
                  "Direct", "Echo", "Tunnel", "Transparent", "Redir", "Pf"]:
        check(f"proto.{name} exists", hasattr(proto, name))

def test_pproxy_cipher_module():
    print("test_pproxy_cipher_module")
    import pproxy.cipher as cipher
    check("pproxy.cipher is module", isinstance(cipher, types.ModuleType))
    check("MAP exists", hasattr(cipher, "MAP"))
    check("MAP is dict", isinstance(cipher.MAP, dict))
    check("get_cipher exists", hasattr(cipher, "get_cipher"))
    check("get_cipher callable", callable(cipher.get_cipher))
    check("BaseCipher exists", hasattr(cipher, "BaseCipher"))
    check("AEADCipher exists", hasattr(cipher, "AEADCipher"))
    check("PacketCipher exists", hasattr(cipher, "PacketCipher"))
    aead_names = [
        "AES_128_GCM_Cipher", "AES_192_GCM_Cipher", "AES_256_GCM_Cipher",
        "ChaCha20_IETF_POLY1305_Cipher",
    ]
    for name in aead_names:
        check(f"cipher.{name} exists", hasattr(cipher, name))

def test_pproxy_cipherpy_module():
    print("test_pproxy_cipherpy_module")
    import pproxy.cipherpy as cipherpy
    check("pproxy.cipherpy is module", isinstance(cipherpy, types.ModuleType))
    check("MAP exists", hasattr(cipherpy, "MAP"))
    check("MAP is dict", isinstance(cipherpy.MAP, dict))
    check("AEADCipher exists", hasattr(cipherpy, "AEADCipher"))
    check("AES_128_GCM_Cipher exists", hasattr(cipherpy, "AES_128_GCM_Cipher"))

def test_pproxy_plugin_module():
    print("test_pproxy_plugin_module")
    import pproxy.plugin as plugin
    check("pproxy.plugin is module", isinstance(plugin, types.ModuleType))
    check("PLUGIN exists", hasattr(plugin, "PLUGIN"))
    check("PLUGIN is dict", isinstance(plugin.PLUGIN, dict))
    check("get_plugin exists", hasattr(plugin, "get_plugin"))
    check("get_plugin callable", callable(plugin.get_plugin))
    check("BasePlugin exists", hasattr(plugin, "BasePlugin"))
    check("TIMESTAMP_TOLERANCE exists", hasattr(plugin, "TIMESTAMP_TOLERANCE"))
    check("TIMESTAMP_TOLERANCE is int", isinstance(plugin.TIMESTAMP_TOLERANCE, int))

def test_pproxy_verbose_module():
    print("test_pproxy_verbose_module")
    import pproxy.verbose as verbose
    check("pproxy.verbose is module", isinstance(verbose, types.ModuleType))
    check("all_stat exists", hasattr(verbose, "all_stat"))
    check("b2s exists", hasattr(verbose, "b2s"))
    check("realtime_stat exists", hasattr(verbose, "realtime_stat"))

def test_pproxy_sysproxy_module():
    print("test_pproxy_sysproxy_module")
    import pproxy.sysproxy as sysproxy
    check("pproxy.sysproxy is module", isinstance(sysproxy, types.ModuleType))
    check("setup exists", hasattr(sysproxy, "setup"))

def test_compile_rule():
    print("test_compile_rule")
    from pproxy.server import compile_rule
    check("compile_rule is callable", callable(compile_rule))

def test_version():
    print("test_version")
    check("version is string", isinstance(PPROXY_VERSION, str))
    check("version is 2.7.9", PPROXY_VERSION == "2.7.9")

def main():
    print(f"pproxy {PPROXY_VERSION} api_namespace fixture")
    print(f"Python {sys.version}")
    print()

    test_top_level_imports()
    test_connection_server_alias()
    test_direct_identity()
    test_pproxy_server_module()
    test_pproxy_server_constants()
    test_pproxy_proto_module()
    test_pproxy_proto_types()
    test_pproxy_cipher_module()
    test_pproxy_cipherpy_module()
    test_pproxy_plugin_module()
    test_pproxy_verbose_module()
    test_pproxy_sysproxy_module()
    test_compile_rule()
    test_version()

    print()
    print(f"Results: {passed} passed, {failed} failed")
    return 1 if failed else 0

if __name__ == "__main__":
    sys.exit(main())
