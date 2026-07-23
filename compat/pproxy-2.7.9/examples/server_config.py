#!/usr/bin/env python3
"""Executable oracle fixture: pproxy 2.7.9 Server construction and configuration.

Tests Server construction from URI patterns, listener configuration,
and rule compilation. Does NOT start actual servers.

Provenance: Eggress-authored behavioral scenario based on pproxy 2.7.9 public API.
License: MIT (pproxy)
Tested with: pproxy==2.7.9 on Python 3.11
"""
import os
import sys
import pproxy
from pproxy import Connection, Server, DIRECT
from pproxy.server import (
    ProxyDirect, ProxySimple, ProxyBackward,
    compile_rule, SOCKET_TIMEOUT, UDP_LIMIT,
)

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

def test_server_from_uri():
    print("test_server_from_uri")
    server = Server("http://:8080/")
    check("http listener returns ProxySimple", isinstance(server, ProxySimple))
    check("http listener bind contains 8080", "8080" in (server.bind or ""))

    server_s5 = Server("socks5://:1080/")
    check("socks5 listener returns ProxySimple", isinstance(server_s5, ProxySimple))
    check("socks5 listener bind contains 1080", "1080" in (server_s5.bind or ""))

    server_s4 = Server("socks4://:1080/")
    check("socks4 listener returns ProxySimple", isinstance(server_s4, ProxySimple))

def test_server_multi_protocol():
    print("test_server_multi_protocol")
    server = Server("http+socks4+socks5://:8080/")
    check("multi-protocol construction succeeds", isinstance(server, ProxySimple))
    check("has protos list", hasattr(server, "protos"))
    check("protos is non-empty", len(server.protos) > 0)

def test_server_with_auth():
    print("test_server_with_auth")
    # pproxy uses URL fragment (#) for auth, not userinfo (@)
    server = Server("socks5://:1080/#user:pass")
    check("auth construction succeeds", isinstance(server, ProxySimple))
    check("users is set", server.users is not None)

def test_server_with_upstream():
    print("test_server_with_upstream")
    server = Server("http://:8080/__socks5://upstream:1080/")
    check("upstream chain succeeds", isinstance(server, ProxySimple))
    check("jump is ProxySimple", isinstance(server.jump, ProxySimple))

def test_server_shadowsocks():
    print("test_server_shadowsocks")
    server = Server("ss://aes-256-gcm:key@:8388/")
    check("ss construction succeeds", isinstance(server, ProxySimple))
    check("ss cipher is set", server.cipher is not None)

def test_server_tls():
    print("test_server_tls")
    # pproxy uses +ssl suffix, not https:// scheme
    server = Server("http+ssl://:8443/")
    check("http+ssl construction succeeds", isinstance(server, ProxySimple))
    check("sslcontext is set", server.sslserver is not None)

def test_compile_rule():
    print("test_compile_rule")
    check("compile_rule is callable", callable(compile_rule))
    rule_path = os.path.join(os.path.dirname(__file__), "rule_file.txt")
    if os.path.exists(rule_path):
        rule = compile_rule(rule_path)
        check("compile_rule from file succeeds", rule is not None)
        check("rule is callable", callable(rule))
    else:
        check("rule_file.txt exists", False, f"not found at {rule_path}")

def test_server_constants():
    print("test_server_constants")
    check("SOCKET_TIMEOUT is int", isinstance(SOCKET_TIMEOUT, int))
    check("SOCKET_TIMEOUT is positive", SOCKET_TIMEOUT > 0)
    check("UDP_LIMIT is int", isinstance(UDP_LIMIT, int))
    check("UDP_LIMIT is positive", UDP_LIMIT > 0)
    check("SOCKET_TIMEOUT == 60", SOCKET_TIMEOUT == 60)
    check("UDP_LIMIT == 30", UDP_LIMIT == 30)

def test_server_direct():
    print("test_server_direct")
    server = Server("direct://")
    check("direct server is ProxyDirect", isinstance(server, ProxyDirect))
    check("direct is True", server.direct is True)

def test_server_attributes():
    print("test_server_attributes")
    server = Server("http://:8080/")
    attrs = ["bind", "alive", "connections", "direct", "protos",
             "tcp_connect", "udp_open_connection", "match_rule",
             "prepare_connection", "open_connection"]
    for attr in attrs:
        check(f"server has {attr}", hasattr(server, attr))

def main():
    print(f"pproxy {PPROXY_VERSION} server_config fixture")
    print(f"Python {sys.version}")
    print()

    test_server_from_uri()
    test_server_multi_protocol()
    test_server_with_auth()
    test_server_with_upstream()
    test_server_shadowsocks()
    test_server_tls()
    test_compile_rule()
    test_server_constants()
    test_server_direct()
    test_server_attributes()

    print()
    print(f"Results: {passed} passed, {failed} failed")
    return 1 if failed else 0

if __name__ == "__main__":
    sys.exit(main())
