#!/usr/bin/env python3
"""Executable oracle fixture: pproxy 2.7.9 Connection object construction.

Tests object construction, attribute existence, and chain topology
without requiring network access. Endpoint configuration via environment
variables where applicable.

Provenance: Derived from pproxy API contract and upstream examples.
License: MIT (pproxy)
Tested with: pproxy==2.7.9 on Python 3.11
"""
import os
import sys
import pproxy
from pproxy import Connection, Server, DIRECT
from pproxy.server import ProxyDirect, ProxySimple, ProxyBackward

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

def test_basic_construction():
    print("test_basic_construction")
    host = os.environ.get("PROXY_HOST", "proxy.example.com")
    port = os.environ.get("PROXY_PORT", "8080")

    conn = Connection(f"http://{host}:{port}/")
    check("http returns ProxySimple", isinstance(conn, ProxySimple))
    check("http bind matches host:port", conn.bind == f"{host}:{port}")
    check("http not direct", conn.direct is False)
    check("http alive is True", conn.alive is True)
    check("http connections is 0", conn.connections == 0)

    conn_s5 = Connection(f"socks5://{host}:1080/")
    check("socks5 returns ProxySimple", isinstance(conn_s5, ProxySimple))
    check("socks5 bind matches", conn_s5.bind == f"{host}:1080")

    conn_s4 = Connection(f"socks4://{host}:1080/")
    check("socks4 returns ProxySimple", isinstance(conn_s4, ProxySimple))

def test_direct_construction():
    print("test_direct_construction")
    conn = Connection("direct://")
    check("direct:// returns ProxyDirect", isinstance(conn, ProxyDirect))
    check("direct is True", conn.direct is True)
    check("direct alive is True", conn.alive is True)
    check("direct connections is 0", conn.connections == 0)
    check("direct bind is 'DIRECT'", conn.bind == "DIRECT")

def test_direct_sentinel():
    print("test_direct_sentinel")
    check("DIRECT is ProxyDirect instance", isinstance(DIRECT, ProxyDirect))
    check("DIRECT.direct is True", DIRECT.direct is True)
    check("DIRECT is pproxy.DIRECT", DIRECT is pproxy.DIRECT)
    check("DIRECT is pproxy.server.DIRECT", DIRECT is pproxy.server.DIRECT)

def test_connection_attributes():
    print("test_connection_attributes")
    conn = Connection("http://proxy:8080/")
    required_attrs = [
        "bind", "alive", "connections", "direct", "destination",
        "tcp_connect", "udp_open_connection", "match_rule",
        "prepare_connection", "open_connection", "wait_open_connection",
        "logtext", "unix",
    ]
    for attr in required_attrs:
        check(f"has attribute {attr}", hasattr(conn, attr))

    check("bind is str", isinstance(conn.bind, str))
    check("alive is bool", isinstance(conn.alive, bool))
    check("connections is int", isinstance(conn.connections, int))
    check("tcp_connect is callable", callable(conn.tcp_connect))
    check("udp_open_connection is callable", callable(conn.udp_open_connection))

def test_auth_construction():
    print("test_auth_construction")
    user = os.environ.get("PROXY_USER", "testuser")
    password = os.environ.get("PROXY_PASS", "testpass")
    # pproxy uses URL fragment (#) for auth, not userinfo (@)
    conn = Connection(f"socks5://proxy:1080/#{user}:{password}")
    check("auth construction succeeds", isinstance(conn, ProxySimple))
    check("auth users is set", conn.users is not None)
    check("auth users is non-empty", len(conn.users) > 0)

def test_shadowsocks_construction():
    print("test_shadowsocks_construction")
    conn = Connection("ss://aes-256-gcm:key@server:8388/")
    check("ss construction succeeds", isinstance(conn, ProxySimple))
    check("ss cipher is set", conn.cipher is not None)

def test_tls_construction():
    print("test_tls_construction")
    # pproxy uses +ssl suffix, not https:// scheme
    conn = Connection("http+ssl://proxy:8443/")
    check("http+ssl construction succeeds", isinstance(conn, ProxySimple))
    check("sslclient is set", conn.sslclient is not None)
    check("sslserver is set", conn.sslserver is not None)

def test_jump_topology():
    print("test_jump_topology")
    conn = Connection("http://proxy1:8080/__socks5://proxy2:1080/")
    check("two-hop chain returns ProxySimple", isinstance(conn, ProxySimple))
    check("first hop jump is ProxySimple", isinstance(conn.jump, ProxySimple))
    check("second hop is terminal", isinstance(conn.jump.jump, ProxyDirect))

    conn3 = Connection("http://a/__socks5://b/__ss://c:8388/")
    check("three-hop chain returns ProxySimple", isinstance(conn3, ProxySimple))
    check("hop 1 jump is ProxySimple", isinstance(conn3.jump, ProxySimple))
    check("hop 2 jump is ProxySimple", isinstance(conn3.jump.jump, ProxySimple))
    check("hop 3 is terminal ProxyDirect", isinstance(conn3.jump.jump.jump, ProxyDirect))

def test_jump_is_not_list():
    print("test_jump_is_not_list")
    conn = Connection("http://proxy1:8080/__socks5://proxy2:1080/")
    check("jump is not a list", not isinstance(conn.jump, list))
    check("jump is not a tuple", not isinstance(conn.jump, tuple))
    check("jump is a ProxySimple", isinstance(conn.jump, ProxySimple))

def test_server_construction():
    print("test_server_construction")
    server = Server("http://:8080/")
    check("Server construction succeeds", server is not None)
    check("Server returns ProxySimple", isinstance(server, ProxySimple))

def test_rule_file():
    print("test_rule_file")
    rule_path = os.path.join(os.path.dirname(__file__), "rule_file.txt")
    if os.path.exists(rule_path):
        rule = pproxy.Rule(rule_path)
        check("Rule construction succeeds", rule is not None)
    else:
        check("rule_file.txt exists", False, f"not found at {rule_path}")

def main():
    print(f"pproxy {PPROXY_VERSION} client_connection fixture")
    print(f"Python {sys.version}")
    print()

    test_basic_construction()
    test_direct_construction()
    test_direct_sentinel()
    test_connection_attributes()
    test_auth_construction()
    test_shadowsocks_construction()
    test_tls_construction()
    test_jump_topology()
    test_jump_is_not_list()
    test_server_construction()
    test_rule_file()

    print()
    print(f"Results: {passed} passed, {failed} failed")
    return 1 if failed else 0

if __name__ == "__main__":
    sys.exit(main())
