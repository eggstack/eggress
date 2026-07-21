#!/usr/bin/env python3
"""Executable oracle fixture: pproxy 2.7.9 chain (__ separator) syntax.

Tests the double-underscore chain syntax and .jump topology.

Provenance: Derived from pproxy API contract and upstream examples.
License: MIT (pproxy)
Tested with: pproxy==2.7.9 on Python 3.11
"""
import sys
import pproxy
from pproxy import Connection, DIRECT
from pproxy.server import ProxyDirect, ProxySimple

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

def test_single_hop():
    print("test_single_hop")
    conn = Connection("http://proxy:8080/")
    check("single hop returns ProxySimple", isinstance(conn, ProxySimple))
    check("single hop jump is ProxyDirect (terminal)", isinstance(conn.jump, ProxyDirect))

def test_two_hops():
    print("test_two_hops")
    conn = Connection("http://proxy1:8080/__socks5://proxy2:1080/")
    check("two-hop returns ProxySimple", isinstance(conn, ProxySimple))
    check("hop 1 is ProxySimple", isinstance(conn, ProxySimple))
    check("hop 2 (jump) is ProxySimple", isinstance(conn.jump, ProxySimple))
    check("hop 2 jump.jump is ProxyDirect (terminal)", isinstance(conn.jump.jump, ProxyDirect))

def test_three_hops():
    print("test_three_hops")
    conn = Connection("http://a/__socks5://b/__ss://c:8388/")
    check("three-hop returns ProxySimple", isinstance(conn, ProxySimple))
    check("hop 1 is ProxySimple", isinstance(conn, ProxySimple))
    check("hop 2 (jump) is ProxySimple", isinstance(conn.jump, ProxySimple))
    check("hop 3 (jump.jump) is ProxySimple", isinstance(conn.jump.jump, ProxySimple))
    check("hop 3 jump.jump.jump is ProxyDirect (terminal)", isinstance(conn.jump.jump.jump, ProxyDirect))

def test_jump_is_nested_not_list():
    print("test_jump_is_nested_not_list")
    conn = Connection("http://proxy1:8080/__socks5://proxy2:1080/")
    check("jump is not a list", not isinstance(conn.jump, list))
    check("jump is not a tuple", not isinstance(conn.jump, tuple))
    check("jump is not a dict", not isinstance(conn.jump, dict))
    check("jump is ProxySimple (nested object)", isinstance(conn.jump, ProxySimple))
    check("jump.jump is ProxyDirect (terminal)", isinstance(conn.jump.jump, ProxyDirect))

def test_direct_sentinel():
    print("test_direct_sentinel")
    check("DIRECT is not None", DIRECT is not None)
    check("DIRECT is ProxyDirect", isinstance(DIRECT, ProxyDirect))
    check("DIRECT.direct is True", DIRECT.direct is True)
    check("DIRECT is pproxy.DIRECT", DIRECT is pproxy.DIRECT)
    check("DIRECT is pproxy.server.DIRECT", DIRECT is pproxy.server.DIRECT)

def test_direct_as_single_hop():
    print("test_direct_as_single_hop")
    conn = Connection("direct://")
    check("direct:// returns ProxyDirect", isinstance(conn, ProxyDirect))
    check("direct is True", conn.direct is True)
    check("is same type as DIRECT sentinel", type(conn) is type(DIRECT))

def test_chain_bind_is_first_hop():
    print("test_chain_bind_is_first_hop")
    conn = Connection("http://first:8080/__socks5://second:1080/")
    check("bind is first hop host:port", conn.bind == "first:8080")

def test_chain_jump_bind_is_second_hop():
    print("test_chain_jump_bind_is_second_hop")
    conn = Connection("http://first:8080/__socks5://second:1080/")
    check("jump.bind is second hop host:port", conn.jump.bind == "second:1080")

def main():
    print(f"pproxy {PPROXY_VERSION} chain_example fixture")
    print(f"Python {sys.version}")
    print()

    test_single_hop()
    test_two_hops()
    test_three_hops()
    test_jump_is_nested_not_list()
    test_direct_sentinel()
    test_direct_as_single_hop()
    test_chain_bind_is_first_hop()
    test_chain_jump_bind_is_second_hop()

    print()
    print(f"Results: {passed} passed, {failed} failed")
    return 1 if failed else 0

if __name__ == "__main__":
    sys.exit(main())
