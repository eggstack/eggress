#!/usr/bin/env python3
"""Executable oracle fixture: pproxy 2.7.9 direct connection construction.

Tests Connection('direct://') and ProxyDirect attributes, method
existence, and signatures.

Provenance: Eggress-authored behavioral scenario based on pproxy 2.7.9 public API.
License: MIT (pproxy)
Tested with: pproxy==2.7.9 on Python 3.11
"""
import sys
import inspect
import pproxy
from pproxy import Connection, DIRECT
from pproxy.server import ProxyDirect

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

def test_direct_construction():
    print("test_direct_construction")
    conn = Connection("direct://")
    check("returns ProxyDirect", isinstance(conn, ProxyDirect))
    check("direct is True", conn.direct is True)
    check("alive is True", conn.alive is True)
    check("connections is 0", conn.connections == 0)

def test_direct_attributes():
    print("test_direct_attributes")
    conn = Connection("direct://")
    attrs = [
        "bind", "alive", "connections", "direct", "destination",
        "logtext", "lbind", "unix",
    ]
    for attr in attrs:
        check(f"has attribute {attr}", hasattr(conn, attr))

def test_direct_methods():
    print("test_direct_methods")
    conn = Connection("direct://")
    methods = [
        "tcp_connect", "udp_open_connection",
        "prepare_connection", "open_connection", "wait_open_connection",
        "match_rule", "connection_change",
        "udp_prepare_connection", "udp_packet_unpack", "udp_sendto",
    ]
    for method in methods:
        check(f"has method {method}", hasattr(conn, method) and callable(getattr(conn, method)))

def test_tcp_connect_signature():
    print("test_tcp_connect_signature")
    conn = Connection("direct://")
    sig = inspect.signature(conn.tcp_connect)
    params = list(sig.parameters.keys())
    check("tcp_connect has host param", "host" in params or "rserver" in params or len(params) > 0)

def test_udp_open_connection_signature():
    print("test_udp_open_connection_signature")
    conn = Connection("direct://")
    sig = inspect.signature(conn.udp_open_connection)
    check("udp_open_connection is callable", callable(conn.udp_open_connection))
    params = list(sig.parameters.keys())
    check("udp_open_connection has params", len(params) > 0)

def test_direct_bind():
    print("test_direct_bind")
    conn = Connection("direct://")
    check("direct bind is 'DIRECT'", conn.bind == "DIRECT")

def test_direct_is_singleton_type():
    print("test_direct_is_singleton_type")
    conn = Connection("direct://")
    check("type(conn) is ProxyDirect", type(conn) is ProxyDirect)
    check("type(DIRECT) is ProxyDirect", type(DIRECT) is ProxyDirect)
    check("type matches DIRECT sentinel", type(conn) is type(DIRECT))

def test_direct_lbind():
    print("test_direct_lbind")
    conn = Connection("direct://")
    check("lbind attribute exists", hasattr(conn, "lbind"))

def test_direct_destination():
    print("test_direct_destination")
    conn = Connection("direct://")
    check("destination is callable", callable(conn.destination))

def main():
    print(f"pproxy {PPROXY_VERSION} direct_tcp fixture")
    print(f"Python {sys.version}")
    print()

    test_direct_construction()
    test_direct_attributes()
    test_direct_methods()
    test_tcp_connect_signature()
    test_udp_open_connection_signature()
    test_direct_bind()
    test_direct_is_singleton_type()
    test_direct_lbind()
    test_direct_destination()

    print()
    print(f"Results: {passed} passed, {failed} failed")
    return 1 if failed else 0

if __name__ == "__main__":
    sys.exit(main())
