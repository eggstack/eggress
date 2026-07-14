#!/usr/bin/env python3
"""Behavioral probes for pproxy API surfaces.

Runs controlled probes that static inspection cannot capture. Each probe
executes in an isolated subprocess with bounded timeout.

Usage:
    python3.11 python/compat/behavioral_probes.py
"""
from __future__ import annotations

import json
import subprocess
import sys
import textwrap
from pathlib import Path

# ---------------------------------------------------------------------------
# Probe definitions: (name, code_string)
# Each probe is self-contained Python that imports pproxy, tests behavior,
# and prints a JSON result to stdout.  No network sockets are opened.
# ---------------------------------------------------------------------------

PROBES: list[tuple[str, str]] = [
    # ── 1. Constructor-created attributes ──────────────────────────────────
    (
        "connection_attributes",
        textwrap.dedent("""\
            import json, pproxy
            result = {"probe": "connection_attributes"}
            try:
                conn = pproxy.Connection
                result["is_callable"] = callable(conn)
                # It's a function (proxies_by_uri), not a class
                result["type"] = type(conn).__name__
                result["module"] = getattr(conn, '__module__', None)
                result["qualname"] = getattr(conn, '__qualname__', None)
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "server_attributes",
        textwrap.dedent("""\
            import json, pproxy
            result = {"probe": "server_attributes"}
            try:
                srv = pproxy.Server
                result["is_callable"] = callable(srv)
                result["type"] = type(srv).__name__
                result["qualname"] = getattr(srv, '__qualname__', None)
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "rule_attributes",
        textwrap.dedent("""\
            import json, pproxy
            result = {"probe": "rule_attributes"}
            try:
                rule = pproxy.Rule
                result["is_callable"] = callable(rule)
                result["type"] = type(rule).__name__
                result["qualname"] = getattr(rule, '__qualname__', None)
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),

    # ── 2. Default object state ────────────────────────────────────────────
    (
        "direct_singleton_type",
        textwrap.dedent("""\
            import json, pproxy
            result = {"probe": "direct_singleton_type"}
            try:
                d = pproxy.DIRECT
                result["type"] = type(d).__name__
                result["mro"] = [c.__name__ for c in type(d).__mro__]
                result["module"] = type(d).__module__
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "direct_properties",
        textwrap.dedent("""\
            import json, pproxy
            result = {"probe": "direct_properties"}
            try:
                d = pproxy.DIRECT
                # ProxyDirect has a 'direct' property
                result["direct_prop"] = type(d.direct).__name__
                result["direct_value"] = bool(d.direct)
                # Check connection_change is callable
                result["has_connection_change"] = callable(d.connection_change)
                result["has_destination"] = callable(d.destination)
                result["has_logtext"] = callable(d.logtext)
                result["has_match_rule"] = callable(d.match_rule)
                result["has_open_connection"] = callable(d.open_connection)
                result["has_tcp_connect"] = callable(d.tcp_connect)
                result["has_wait_open_connection"] = callable(d.wait_open_connection)
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "socket_timeout_value",
        textwrap.dedent("""\
            import json, pproxy.server
            result = {"probe": "socket_timeout_value"}
            try:
                result["SOCKET_TIMEOUT"] = pproxy.server.SOCKET_TIMEOUT
                result["type"] = type(pproxy.server.SOCKET_TIMEOUT).__name__
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "udp_limit_value",
        textwrap.dedent("""\
            import json, pproxy.server
            result = {"probe": "udp_limit_value"}
            try:
                result["UDP_LIMIT"] = pproxy.server.UDP_LIMIT
                result["type"] = type(pproxy.server.UDP_LIMIT).__name__
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "sslcontexts_default",
        textwrap.dedent("""\
            import json, pproxy.server
            result = {"probe": "sslcontexts_default"}
            try:
                result["sslcontexts"] = pproxy.server.sslcontexts
                result["type"] = type(pproxy.server.sslcontexts).__name__
                result["len"] = len(pproxy.server.sslcontexts)
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),

    # ── 3. Exceptions for invalid arguments ────────────────────────────────
    (
        "connection_no_args",
        textwrap.dedent("""\
            import json, pproxy
            result = {"probe": "connection_no_args"}
            try:
                # proxies_by_uri(uri_jumps) — calling with no args
                pproxy.Connection()
                result["error"] = "no exception raised"
                result["pass"] = False
            except TypeError as e:
                result["exception_type"] = "TypeError"
                result["error"] = str(e)
                result["pass"] = True
            except Exception as e:
                result["exception_type"] = type(e).__name__
                result["error"] = str(e)
                result["pass"] = True
            print(json.dumps(result))
        """),
    ),
    (
        "connection_invalid_type",
        textwrap.dedent("""\
            import json, pproxy
            result = {"probe": "connection_invalid_type"}
            try:
                pproxy.Connection(12345)
                result["error"] = "no exception raised"
                result["pass"] = False
            except TypeError as e:
                result["exception_type"] = "TypeError"
                result["error"] = str(e)
                result["pass"] = True
            except Exception as e:
                result["exception_type"] = type(e).__name__
                result["error"] = str(e)
                result["pass"] = True
            print(json.dumps(result))
        """),
    ),
    (
        "rule_no_args",
        textwrap.dedent("""\
            import json, pproxy
            result = {"probe": "rule_no_args"}
            try:
                pproxy.Rule()
                result["error"] = "no exception raised"
                result["pass"] = False
            except TypeError as e:
                result["exception_type"] = "TypeError"
                result["error"] = str(e)
                result["pass"] = True
            except Exception as e:
                result["exception_type"] = type(e).__name__
                result["error"] = str(e)
                result["pass"] = True
            print(json.dumps(result))
        """),
    ),
    (
        "baseprotocol_no_args",
        textwrap.dedent("""\
            import json, pproxy.proto
            result = {"probe": "baseprotocol_no_args"}
            try:
                pproxy.proto.BaseProtocol()
                result["error"] = "no exception raised"
                result["pass"] = False
            except TypeError as e:
                result["exception_type"] = "TypeError"
                result["error"] = str(e)
                result["pass"] = True
            except Exception as e:
                result["exception_type"] = type(e).__name__
                result["error"] = str(e)
                result["pass"] = True
            print(json.dumps(result))
        """),
    ),
    (
        "basecipher_no_args",
        textwrap.dedent("""\
            import json, pproxy.cipher
            result = {"probe": "basecipher_no_args"}
            try:
                pproxy.cipher.BaseCipher()
                result["error"] = "no exception raised"
                result["pass"] = False
            except TypeError as e:
                result["exception_type"] = "TypeError"
                result["error"] = str(e)
                result["pass"] = True
            except Exception as e:
                result["exception_type"] = type(e).__name__
                result["error"] = str(e)
                result["pass"] = True
            print(json.dumps(result))
        """),
    ),
    (
        "auth_table_no_args",
        textwrap.dedent("""\
            import json, pproxy.server
            result = {"probe": "auth_table_no_args"}
            try:
                pproxy.server.AuthTable()
                result["error"] = "no exception raised"
                result["pass"] = False
            except TypeError as e:
                result["exception_type"] = "TypeError"
                result["error"] = str(e)
                result["pass"] = True
            except Exception as e:
                result["exception_type"] = type(e).__name__
                result["error"] = str(e)
                result["pass"] = True
            print(json.dumps(result))
        """),
    ),

    # ── 4. Registry lookup and aliases ─────────────────────────────────────
    (
        "connection_server_same_object",
        textwrap.dedent("""\
            import json, pproxy
            result = {"probe": "connection_server_same_object"}
            try:
                # Both Connection and Server should be aliases for proxies_by_uri
                result["same"] = pproxy.Connection is pproxy.Server
                result["connection_is_server"] = pproxy.Connection is pproxy.server.proxies_by_uri
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "direct_is_proxydirect_instance",
        textwrap.dedent("""\
            import json, pproxy, pproxy.server
            result = {"probe": "direct_is_proxydirect_instance"}
            try:
                result["is_proxydirect"] = isinstance(pproxy.DIRECT, pproxy.server.ProxyDirect)
                result["direct_server_same"] = pproxy.DIRECT is pproxy.server.DIRECT
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "proto_module_accessible",
        textwrap.dedent("""\
            import json, pproxy, pproxy.proto
            result = {"probe": "proto_module_accessible"}
            try:
                result["pproxy_proto_is_pproxy_proto"] = pproxy.proto is pproxy.proto
                result["has_direct"] = hasattr(pproxy.proto, 'Direct')
                result["has_socks5"] = hasattr(pproxy.proto, 'Socks5')
                result["has_http"] = hasattr(pproxy.proto, 'HTTP')
                result["has_ss"] = hasattr(pproxy.proto, 'SS')
                result["has_trojan"] = hasattr(pproxy.proto, 'Trojan')
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "server_module_accessible",
        textwrap.dedent("""\
            import json, pproxy, pproxy.server
            result = {"probe": "server_module_accessible"}
            try:
                result["has_ProxyDirect"] = hasattr(pproxy.server, 'ProxyDirect')
                result["has_ProxySimple"] = hasattr(pproxy.server, 'ProxySimple')
                result["has_ProxyBackward"] = hasattr(pproxy.server, 'ProxyBackward')
                result["has_ProxyH2"] = hasattr(pproxy.server, 'ProxyH2')
                result["has_ProxyH3"] = hasattr(pproxy.server, 'ProxyH3')
                result["has_ProxyQUIC"] = hasattr(pproxy.server, 'ProxyQUIC')
                result["has_ProxySSH"] = hasattr(pproxy.server, 'ProxySSH')
                result["has_AuthTable"] = hasattr(pproxy.server, 'AuthTable')
                result["has_stream_handler"] = hasattr(pproxy.server, 'stream_handler')
                result["has_main"] = hasattr(pproxy.server, 'main')
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),

    # ── 5. Object representations ──────────────────────────────────────────
    (
        "direct_repr",
        textwrap.dedent("""\
            import json, pproxy
            result = {"probe": "direct_repr"}
            try:
                r = repr(pproxy.DIRECT)
                s = str(pproxy.DIRECT)
                result["repr"] = r
                result["str"] = s
                result["repr_type"] = type(r).__name__
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "baseprotocol_repr",
        textwrap.dedent("""\
            import json, pproxy.proto
            result = {"probe": "baseprotocol_repr"}
            try:
                # BaseProtocol requires a param arg; pass a dummy
                bp = pproxy.proto.BaseProtocol({})
                result["repr"] = repr(bp)
                result["str"] = str(bp)
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "direct_class_repr",
        textwrap.dedent("""\
            import json, pproxy.proto
            result = {"probe": "direct_class_repr"}
            try:
                r = repr(pproxy.proto.Direct)
                s = str(pproxy.proto.Direct)
                result["repr"] = r
                result["str"] = s
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),

    # ── 6. Truthiness and equality ─────────────────────────────────────────
    (
        "direct_bool",
        textwrap.dedent("""\
            import json, pproxy
            result = {"probe": "direct_bool"}
            try:
                result["bool_d"] = bool(pproxy.DIRECT)
                result["is_not_none"] = pproxy.DIRECT is not None
                result["is_not_false"] = pproxy.DIRECT is not False
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "direct_identity",
        textwrap.dedent("""\
            import json, pproxy
            result = {"probe": "direct_identity"}
            try:
                d1 = pproxy.DIRECT
                d2 = pproxy.DIRECT
                result["same_object"] = d1 is d2
                result["eq"] = d1 == d2
                result["ne"] = d1 != d2
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "different_proxydirect_not_equal",
        textwrap.dedent("""\
            import json, pproxy.server
            result = {"probe": "different_proxydirect_not_equal"}
            try:
                d1 = pproxy.server.ProxyDirect()
                d2 = pproxy.server.ProxyDirect()
                result["same_object"] = d1 is d2
                result["eq"] = d1 == d2
                result["ne"] = d1 != d2
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),

    # ── 7. Protocol/cipher factory objects ─────────────────────────────────
    (
        "mappings_keys",
        textwrap.dedent("""\
            import json, pproxy.proto
            result = {"probe": "mappings_keys"}
            try:
                m = pproxy.proto.MAPPINGS
                result["type"] = type(m).__name__
                result["keys"] = sorted(m.keys())
                result["count"] = len(m)
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "mappings_class_values",
        textwrap.dedent("""\
            import json, pproxy.proto
            result = {"probe": "mappings_class_values"}
            try:
                m = pproxy.proto.MAPPINGS
                result["value_types"] = {
                    k: type(v).__name__ for k, v in m.items()
                }
                # Most values are classes, but some are strings (ssl, secure, quic, in)
                class_keys = [k for k, v in m.items() if isinstance(v, type)]
                string_keys = [k for k, v in m.items() if isinstance(v, str)]
                result["class_keys"] = sorted(class_keys)
                result["string_keys"] = sorted(string_keys)
                result["all_classes_or_strings"] = all(
                    isinstance(v, (type, str)) for v in m.values()
                )
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "cipher_map_keys",
        textwrap.dedent("""\
            import json, pproxy.cipher
            result = {"probe": "cipher_map_keys"}
            try:
                m = pproxy.cipher.MAP
                result["type"] = type(m).__name__
                result["keys"] = sorted(m.keys())
                result["count"] = len(m)
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "cipher_map_class_values",
        textwrap.dedent("""\
            import json, pproxy.cipher
            result = {"probe": "cipher_map_class_values"}
            try:
                m = pproxy.cipher.MAP
                result["all_classes"] = all(
                    isinstance(v, type) for v in m.values()
                )
                result["value_names"] = {k: v.__name__ for k, v in m.items()}
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "get_cipher_returns_instance",
        textwrap.dedent("""\
            import json, pproxy.cipher
            result = {"probe": "get_cipher_returns_instance"}
            try:
                # get_cipher returns a tuple (cipher_class_or_None, apply_function)
                ret = pproxy.cipher.get_cipher("rc4:password")
                result["return_type"] = type(ret).__name__
                result["return_len"] = len(ret)
                result["first_element"] = type(ret[0]).__name__ if ret[0] is not None else None
                result["second_element"] = type(ret[1]).__name__
                result["second_callable"] = callable(ret[1])
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "protocol_class_mro",
        textwrap.dedent("""\
            import json, pproxy.proto
            result = {"probe": "protocol_class_mro"}
            try:
                protos = {
                    "Direct": pproxy.proto.Direct,
                    "HTTP": pproxy.proto.HTTP,
                    "HTTPOnly": pproxy.proto.HTTPOnly,
                    "H2": pproxy.proto.H2,
                    "H3": pproxy.proto.H3,
                    "Socks4": pproxy.proto.Socks4,
                    "Socks5": pproxy.proto.Socks5,
                    "SS": pproxy.proto.SS,
                    "SSR": pproxy.proto.SSR,
                    "SSH": pproxy.proto.SSH,
                    "Trojan": pproxy.proto.Trojan,
                    "WS": pproxy.proto.WS,
                    "Transparent": pproxy.proto.Transparent,
                    "Echo": pproxy.proto.Echo,
                    "Redir": pproxy.proto.Redir,
                    "Pf": pproxy.proto.Pf,
                    "Tunnel": pproxy.proto.Tunnel,
                }
                result["mro_map"] = {
                    name: [c.__name__ for c in cls.__mro__]
                    for name, cls in protos.items()
                }
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),

    # ── 8. Constant values ─────────────────────────────────────────────────
    (
        "http_line_value",
        textwrap.dedent("""\
            import json, pproxy.proto
            result = {"probe": "http_line_value"}
            try:
                h = pproxy.proto.HTTP_LINE
                result["type"] = type(h).__name__
                result["pattern"] = h.pattern
                result["is_compiled_regex"] = hasattr(h, 'match')
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "sol_ipv6_value",
        textwrap.dedent("""\
            import json, pproxy.proto
            result = {"probe": "sol_ipv6_value"}
            try:
                result["SOL_IPV6"] = pproxy.proto.SOL_IPV6
                result["type"] = type(pproxy.proto.SOL_IPV6).__name__
                result["is_int"] = isinstance(pproxy.proto.SOL_IPV6, int)
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "so_original_dst_value",
        textwrap.dedent("""\
            import json, pproxy.proto
            result = {"probe": "so_original_dst_value"}
            try:
                result["SO_ORIGINAL_DST"] = pproxy.proto.SO_ORIGINAL_DST
                result["type"] = type(pproxy.proto.SO_ORIGINAL_DST).__name__
                result["is_int"] = isinstance(pproxy.proto.SO_ORIGINAL_DST, int)
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "direct_constant_value",
        textwrap.dedent("""\
            import json, pproxy
            result = {"probe": "direct_constant_value"}
            try:
                result["DIRECT_type"] = type(pproxy.DIRECT).__name__
                result["DIRECT_module"] = type(pproxy.DIRECT).__module__
                result["DIRECT_mro"] = [c.__name__ for c in type(pproxy.DIRECT).__mro__]
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),

    # ── 9. Additional behavioral probes ────────────────────────────────────
    (
        "cipher_instance_behavior",
        textwrap.dedent("""\
            import json, pproxy.cipher
            result = {"probe": "cipher_instance_behavior"}
            try:
                # get_cipher returns a tuple (cipher_class_or_None, apply_function)
                ret = pproxy.cipher.get_cipher("rc4:password")
                result["return_type"] = type(ret).__name__
                result["return_len"] = len(ret)
                result["first_element_type"] = type(ret[0]).__name__ if ret[0] is not None else "NoneType"
                result["second_element_type"] = type(ret[1]).__name__
                result["second_callable"] = callable(ret[1])
                # Instantiate RC4 cipher — test API surface, not crypto
                cipher_cls = pproxy.cipher.RC4_Cipher
                c = cipher_cls(b"password")
                result["has_encrypt"] = callable(c.encrypt)
                result["has_decrypt"] = callable(c.decrypt)
                result["has_setup"] = callable(c.setup)
                result["has_setup_iv"] = callable(c.setup_iv)
                result["has_name"] = callable(c.name)
                # setup() may fail if PyCryptodome is absent — that's ok
                try:
                    c.setup()
                    result["setup_works"] = True
                except ModuleNotFoundError:
                    result["setup_works"] = False
                    result["crypto_unavailable"] = True
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "aead_cipher_nonce",
        textwrap.dedent("""\
            import json, pproxy.cipher
            result = {"probe": "aead_cipher_nonce"}
            try:
                c = pproxy.cipher.AES_256_GCM_Cipher(b"password")
                result["type"] = type(c).__name__
                result["has_nonce"] = "nonce" in dir(c)
                result["has_encrypt_and_digest"] = callable(c.encrypt_and_digest)
                result["has_decrypt_and_verify"] = callable(c.decrypt_and_verify)
                # nonce access may fail before setup (no _nonce attribute)
                try:
                    _ = c.nonce
                    result["nonce_accessible"] = True
                except AttributeError:
                    result["nonce_accessible"] = False
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "auth_table_behavior",
        textwrap.dedent("""\
            import json, pproxy.server
            result = {"probe": "auth_table_behavior"}
            try:
                at = pproxy.server.AuthTable("127.0.0.1", 3600)
                result["type"] = type(at).__name__
                result["has_authed"] = callable(at.authed)
                result["has_set_authed"] = callable(at.set_authed)
                result["initially_not_authed"] = not at.authed()
                at.set_authed("testuser")
                result["after_set_authed"] = at.authed()
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "dummy_function",
        textwrap.dedent("""\
            import json, pproxy.server
            result = {"probe": "dummy_function"}
            try:
                d = pproxy.server.DUMMY
                result["type"] = type(d).__name__
                result["is_callable"] = callable(d)
                # DUMMY is a lambda (s) that returns s
                r = d("test")
                result["returns_input"] = r == "test"
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "packstr_behavior",
        textwrap.dedent("""\
            import json, pproxy.proto
            result = {"probe": "packstr_behavior"}
            try:
                ps = pproxy.proto.packstr
                result["type"] = type(ps).__name__
                result["is_callable"] = callable(ps)
                # packstr expects bytes, prepends length byte(s)
                r1 = ps(b"hello")
                result["result_type"] = type(r1).__name__
                result["result"] = repr(r1)
                result["default_n1"] = r1 == b"\\x05hello"
                r2 = ps(b"hello", 2)
                result["result_n2"] = repr(r2)
                result["n2_len"] = len(r2)
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "socks_address_exists",
        textwrap.dedent("""\
            import json, pproxy.proto
            result = {"probe": "socks_address_exists"}
            try:
                result["has_socks_address"] = callable(pproxy.proto.socks_address)
                result["has_socks_address_stream"] = callable(pproxy.proto.socks_address_stream)
                result["has_get_protos"] = callable(pproxy.proto.get_protos)
                result["has_netloc_split"] = callable(pproxy.proto.netloc_split)
                result["has_sslwrap"] = callable(pproxy.proto.sslwrap)
                result["has_udp_accept"] = callable(pproxy.proto.udp_accept)
                result["has_accept"] = callable(pproxy.proto.accept)
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "cipher_baseclass_hierarchy",
        textwrap.dedent("""\
            import json, pproxy.cipher
            result = {"probe": "cipher_baseclass_hierarchy"}
            try:
                # Verify the cipher class hierarchy
                result["AEADCipher_bases"] = [c.__name__ for c in pproxy.cipher.AEADCipher.__bases__]
                result["BaseCipher_bases"] = [c.__name__ for c in pproxy.cipher.BaseCipher.__bases__]
                result["PacketCipher_bases"] = [c.__name__ for c in pproxy.cipher.PacketCipher.__bases__]
                # Verify AES_256_GCM inherits from AEADCipher
                result["aes256gcm_bases"] = [c.__name__ for c in pproxy.cipher.AES_256_GCM_Cipher.__bases__]
                # Verify RC4 inherits from BaseCipher
                result["rc4_bases"] = [c.__name__ for c in pproxy.cipher.RC4_Cipher.__bases__]
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "proxies_by_uri_signature",
        textwrap.dedent("""\
            import json, pproxy, inspect
            result = {"probe": "proxies_by_uri_signature"}
            try:
                sig = inspect.signature(pproxy.Connection)
                result["params"] = list(sig.parameters.keys())
                result["Connection_params"] = list(inspect.signature(pproxy.Connection).parameters.keys())
                result["Server_params"] = list(inspect.signature(pproxy.Server).parameters.keys())
                result["same_sig"] = (
                    list(inspect.signature(pproxy.Connection).parameters.keys())
                    == list(inspect.signature(pproxy.Server).parameters.keys())
                )
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "mappings_lookup_specific",
        textwrap.dedent("""\
            import json, pproxy.proto
            result = {"probe": "mappings_lookup_specific"}
            try:
                m = pproxy.proto.MAPPINGS
                # Verify specific lookups
                checks = {
                    "direct": "Direct",
                    "http": "HTTP",
                    "socks5": "Socks5",
                    "socks4": "Socks4",
                    "ss": "SS",
                    "ssr": "SSR",
                    "trojan": "Trojan",
                    "ssh": "SSH",
                    "ws": "WS",
                    "h2": "H2",
                    "h3": "H3",
                }
                results = {}
                for key, expected_class in checks.items():
                    cls = m.get(key)
                    if cls is None:
                        results[key] = {"found": False}
                    else:
                        results[key] = {
                            "found": True,
                            "class_name": cls.__name__,
                            "matches_expected": cls.__name__ == expected_class,
                        }
                result["lookups"] = results
                # Also check aliases
                result["socks_alias"] = m.get("socks") is m.get("socks5")
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "cipher_map_lookup_specific",
        textwrap.dedent("""\
            import json, pproxy.cipher
            result = {"probe": "cipher_map_lookup_specific"}
            try:
                m = pproxy.cipher.MAP
                checks = [
                    "rc4", "rc4-md5", "chacha20", "chacha20-ietf",
                    "salsa20", "aes-256-cfb", "aes-128-cfb", "aes-192-cfb",
                    "aes-256-cfb8", "aes-128-cfb8", "aes-192-cfb8",
                    "aes-256-gcm", "aes-128-gcm", "aes-192-gcm",
                    "aes-256-ctr", "aes-128-ctr", "aes-192-ctr",
                    "aes-256-ofb", "aes-128-ofb", "aes-192-ofb",
                    "bf-cfb", "cast5-cfb", "des-cfb",
                    "chacha20-ietf-poly1305",
                ]
                results = {}
                for name in checks:
                    cls = m.get(name)
                    results[name] = {
                        "found": cls is not None,
                        "class_name": cls.__name__ if cls else None,
                    }
                result["lookups"] = results
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "direct_method_signatures",
        textwrap.dedent("""\
            import json, pproxy.server, inspect
            result = {"probe": "direct_method_signatures"}
            try:
                d = pproxy.server.ProxyDirect
                methods = {}
                for name in ["connection_change", "destination", "logtext",
                             "match_rule", "open_connection", "prepare_connection",
                             "tcp_connect", "udp_open_connection",
                             "udp_packet_unpack", "udp_prepare_connection",
                             "udp_sendto", "wait_open_connection"]:
                    method = getattr(d, name, None)
                    if method:
                        sig = inspect.signature(method)
                        methods[name] = list(sig.parameters.keys())
                result["methods"] = methods
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
    (
        "compile_rule_signature",
        textwrap.dedent("""\
            import json, pproxy.server, inspect
            result = {"probe": "compile_rule_signature"}
            try:
                sig = inspect.signature(pproxy.server.compile_rule)
                result["params"] = list(sig.parameters.keys())
                result["Rule_params"] = list(inspect.signature(pproxy.Rule).parameters.keys())
                result["pass"] = True
            except Exception as e:
                result["error"] = str(e)
                result["pass"] = False
            print(json.dumps(result))
        """),
    ),
]


def run_probe(name: str, code: str, timeout: float = 5.0) -> dict:
    """Run a single probe in an isolated subprocess with timeout."""
    try:
        proc = subprocess.run(
            [sys.executable, "-c", code],
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        stdout = proc.stdout.strip()
        stderr = proc.stderr.strip()
        if proc.returncode != 0 and not stdout:
            return {
                "probe": name,
                "pass": False,
                "error": f"exit code {proc.returncode}: {stderr[:500]}",
            }
        try:
            result = json.loads(stdout)
            result.setdefault("probe", name)
            if proc.returncode != 0:
                result["stderr_warning"] = stderr[:300] if stderr else None
            return result
        except json.JSONDecodeError:
            return {
                "probe": name,
                "pass": False,
                "error": f"invalid JSON output: {stdout[:300]}",
                "stderr": stderr[:300] if stderr else None,
            }
    except subprocess.TimeoutExpired:
        return {
            "probe": name,
            "pass": False,
            "error": f"timed out after {timeout}s",
        }
    except Exception as e:
        return {
            "probe": name,
            "pass": False,
            "error": f"subprocess error: {e}",
        }


def main() -> None:
    results: list[dict] = []
    passed = 0
    failed = 0

    for name, code in PROBES:
        result = run_probe(name, code)
        results.append(result)
        status = "PASS" if result.get("pass") else "FAIL"
        if result.get("pass"):
            passed += 1
        else:
            failed += 1
        print(f"  [{status}] {name}: {result.get('error', 'ok')}")

    summary = {
        "total": len(PROBES),
        "passed": passed,
        "failed": failed,
        "results": results,
    }

    out_path = Path(__file__).parent / "behavioral_probe_results.json"
    out_path.write_text(json.dumps(summary, indent=2))
    print(f"\n{passed}/{len(PROBES)} probes passed, {failed} failed")
    print(f"Results written to {out_path}")


if __name__ == "__main__":
    main()
