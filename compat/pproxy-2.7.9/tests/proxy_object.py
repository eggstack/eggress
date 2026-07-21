#!/usr/bin/env python3
"""Executable oracle fixture: pproxy 2.7.9 proxy object hierarchy.

Tests ProxyDirect, ProxySimple, ProxyBackward, ProxyH2 construction,
attributes, identity, and repr format.

Provenance: Derived from pproxy API contract and upstream source.
License: MIT (pproxy)
Tested with: pproxy==2.7.9 on Python 3.11
"""
import sys
import pproxy
from pproxy import Connection, DIRECT
from pproxy.server import (
    ProxyDirect, ProxySimple, ProxyBackward, ProxyH2, ProxySSH,
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

def test_proxy_direct():
    print("test_proxy_direct")
    d = ProxyDirect()
    check("ProxyDirect direct is True", d.direct is True)
    check("ProxyDirect alive is True", d.alive is True)
    check("ProxyDirect connections is 0", d.connections == 0)
    check("ProxyDirect has logtext", hasattr(d, "logtext"))
    check("ProxyDirect has match_rule", hasattr(d, "match_rule"))
    check("ProxyDirect has tcp_connect", hasattr(d, "tcp_connect"))
    check("ProxyDirect has udp_open_connection", hasattr(d, "udp_open_connection"))
    check("ProxyDirect has prepare_connection", hasattr(d, "prepare_connection"))
    check("ProxyDirect has open_connection", hasattr(d, "open_connection"))
    check("ProxyDirect has wait_open_connection", hasattr(d, "wait_open_connection"))
    check("ProxyDirect has destination", hasattr(d, "destination"))

def test_proxy_direct_lbind():
    print("test_proxy_direct_lbind")
    d1 = ProxyDirect()
    d2 = ProxyDirect(lbind=None)
    check("default lbind is None", d1.lbind is None)
    check("explicit lbind=None matches default", d2.lbind is d1.lbind)

def test_proxy_simple():
    print("test_proxy_simple")
    s = ProxySimple(
        jump=None, protos=[], cipher=None, users=[], rule=None,
        bind="test:8080", host_name="test", port=8080,
        unix=True, lbind=None, sslclient=None, sslserver=None,
    )
    check("ProxySimple direct is False", s.direct is False)
    check("ProxySimple jump is None (terminal)", s.jump is None)
    check("ProxySimple bind is set", s.bind == "test:8080")
    check("ProxySimple host_name is set", s.host_name == "test")
    check("ProxySimple port is set", s.port == 8080)
    check("ProxySimple protos is list", isinstance(s.protos, list))
    check("ProxySimple has cipher attr", hasattr(s, "cipher"))
    check("ProxySimple has users attr", hasattr(s, "users"))
    check("ProxySimple has rule attr", hasattr(s, "rule"))
    check("ProxySimple has sslclient attr", hasattr(s, "sslclient"))
    check("ProxySimple has sslserver attr", hasattr(s, "sslserver"))

def test_proxy_simple_jump():
    print("test_proxy_simple_jump")
    inner = ProxyDirect()
    outer = ProxySimple(
        jump=inner, protos=[], cipher=None, users=[], rule=None,
        bind="outer:8080", host_name="outer", port=8080,
        unix=True, lbind=None, sslclient=None, sslserver=None,
    )
    check("ProxySimple.jump is set to inner proxy", outer.jump is inner)
    check("inner is ProxyDirect", isinstance(outer.jump, ProxyDirect))

def test_proxy_backward():
    print("test_proxy_backward")
    inner = ProxySimple(
        jump=ProxyDirect(), protos=[], cipher=None, users=[], rule=None,
        bind=None, host_name=None, port=None, unix=True, lbind=None,
        sslclient=None, sslserver=None,
    )
    b = ProxyBackward(
        inner, 1,
        jump=None, protos=[], cipher=None, users=[], rule=None,
        bind=None, host_name=None, port=None, unix=True, lbind=None,
        sslclient=None, sslserver=None,
    )
    check("ProxyBackward has backward attr", hasattr(b, "backward"))
    check("ProxyBackward backward is the inner proxy", b.backward is inner)
    check("ProxyBackward server is the inner proxy", b.server is inner)

def test_proxy_h2():
    print("test_proxy_h2")
    try:
        h2 = ProxyH2(sslserver=None, sslclient=None,
                      jump=None, protos=[], cipher=None, users=[],
                      rule=None, bind=None, host_name=None, port=None,
                      unix=True, lbind=None)
        check("ProxyH2 construction succeeds", True)
    except Exception as e:
        check("ProxyH2 construction succeeds", False, str(e))

def test_proxy_ssh():
    print("test_proxy_ssh")
    try:
        ssh = ProxySSH(jump=None, protos=[], cipher=None, users=[],
                       rule=None, bind=None, host_name=None, port=None,
                       unix=True, lbind=None, sslclient=None, sslserver=None)
        check("ProxySSH construction succeeds", True)
    except Exception as e:
        check("ProxySSH construction succeeds", False, str(e))

def test_direct_identity():
    print("test_direct_identity")
    d = Connection("direct://")
    check("DIRECT is ProxyDirect", isinstance(DIRECT, ProxyDirect))
    check("Connection('direct://') is ProxyDirect", isinstance(d, ProxyDirect))
    check("DIRECT.direct is True", DIRECT.direct is True)
    check("DIRECT is pproxy.DIRECT", DIRECT is pproxy.DIRECT)
    check("DIRECT is pproxy.server.DIRECT", DIRECT is pproxy.server.DIRECT)

def test_direct_repr():
    print("test_direct_repr")
    d = ProxyDirect()
    r = repr(d)
    check("repr is string", isinstance(r, str))
    check("repr contains ProxyDirect", "ProxyDirect" in r)

def test_proxy_simple_repr():
    print("test_proxy_simple_repr")
    s = ProxySimple(
        jump=None, protos=[], cipher=None, users=[], rule=None,
        bind="test:8080", host_name="test", port=8080,
        unix=True, lbind=None, sslclient=None, sslserver=None,
    )
    r = repr(s)
    check("repr is string", isinstance(r, str))
    check("repr contains ProxySimple", "ProxySimple" in r)

def test_direct_type_consistency():
    print("test_direct_type_consistency")
    d1 = Connection("direct://")
    d2 = Connection("direct://")
    check("all direct connections are same type", type(d1) is type(d2) is ProxyDirect)
    check("all direct connections have same class", d1.__class__ is d2.__class__)

def main():
    print(f"pproxy {PPROXY_VERSION} proxy_object fixture")
    print(f"Python {sys.version}")
    print()

    test_proxy_direct()
    test_proxy_direct_lbind()
    test_proxy_simple()
    test_proxy_simple_jump()
    test_proxy_backward()
    test_proxy_h2()
    test_proxy_ssh()
    test_direct_identity()
    test_direct_repr()
    test_proxy_simple_repr()
    test_direct_type_consistency()

    print()
    print(f"Results: {passed} passed, {failed} failed")
    return 1 if failed else 0

if __name__ == "__main__":
    sys.exit(main())
