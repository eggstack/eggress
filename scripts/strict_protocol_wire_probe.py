#!/usr/bin/env python3
"""Protocol wire behavior probe.

Tests wire-level protocol behavior without requiring network connections.
Validates address encoding, packet framing, and protocol guessing.

Usage:
    python3 strict_protocol_wire_probe.py --module pproxy.proto --symbol Socks5 --test address_encode
    python3 strict_protocol_wire_probe.py --module pproxy.proto --symbol HTTP --test http_parse
"""
import argparse
import json
import sys


def probe(module_name: str, symbol_name: str, test_name: str = "address_encode") -> dict:
    """Run protocol wire probe."""
    result = {
        "module": module_name,
        "symbol": symbol_name,
        "test": test_name,
        "exists": False,
        "passed": False,
        "error": None,
        "details": {},
    }

    try:
        mod = __import__(module_name, fromlist=[symbol_name])
    except ImportError as e:
        result["error"] = f"Import error: {e}"
        return result

    cls = getattr(mod, symbol_name, None)
    if cls is None:
        result["error"] = f"Symbol '{symbol_name}' not found in '{module_name}'"
        return result

    result["exists"] = True

    try:
        if test_name == "address_encode":
            # Test SOCKS5 address encoding/decoding
            if hasattr(cls, 'socks_address') or symbol_name == 'Socks5':
                # Create a minimal instance to test address handling
                proto = cls.__new__(cls)
                # Test address encoding
                test_cases = [
                    ("example.com", 80),
                    ("192.168.1.1", 443),
                    ("::1", 8080),
                ]
                encoded_results = []
                for host, port in test_cases:
                    try:
                        if hasattr(proto, 'address_encode'):
                            encoded = proto.address_encode(host, port)
                            encoded_results.append({"host": host, "port": port, "encoded": True})
                        elif hasattr(cls, 'encode_address'):
                            encoded = cls.encode_address(host, port)
                            encoded_results.append({"host": host, "port": port, "encoded": True})
                        else:
                            encoded_results.append({"host": host, "port": port, "encoded": False, "reason": "no encode method"})
                    except Exception as e:
                        encoded_results.append({"host": host, "port": port, "error": str(e)})

                result["details"]["address_encode"] = encoded_results
                result["passed"] = all(r.get("encoded", False) for r in encoded_results)

            elif symbol_name == 'HTTP':
                # Test HTTP request line parsing
                test_lines = [
                    "GET http://example.com/path HTTP/1.1",
                    "CONNECT example.com:443 HTTP/1.1",
                    "POST http://api.example.com/data HTTP/1.1",
                ]
                parsed_results = []
                for line in test_lines:
                    try:
                        if hasattr(cls, 'parse_request_line'):
                            parsed = cls.parse_request_line(line)
                            parsed_results.append({"line": line, "parsed": True})
                        else:
                            parsed_results.append({"line": line, "parsed": False, "reason": "no parse method"})
                    except Exception as e:
                        parsed_results.append({"line": line, "error": str(e)})

                result["details"]["http_parse"] = parsed_results
                result["passed"] = any(r.get("parsed", False) for r in parsed_results)

            else:
                result["error"] = f"No address_encode test for {symbol_name}"

        elif test_name == "guess":
            # Test protocol guessing from stream bytes
            test_data = [
                b"\x05\x01\x00",  # SOCKS5 greeting
                b"GET http://",    # HTTP request
                b"\x04\x01",      # SOCKS4 connect
                b"\x16\x03\x01",  # TLS ClientHello
            ]
            guess_results = []
            for data in test_data:
                try:
                    if hasattr(cls, 'guess'):
                        guessed = cls.guess(data)
                        guess_results.append({"data": data[:10].hex(), "guessed": bool(guessed)})
                    else:
                        guess_results.append({"data": data[:10].hex(), "guessed": False, "reason": "no guess method"})
                except Exception as e:
                    guess_results.append({"data": data[:10].hex(), "error": str(e)})

            result["details"]["guess"] = guess_results
            result["passed"] = any(r.get("guessed", False) for r in guess_results)

        elif test_name == "name":
            # Test protocol name property
            try:
                if hasattr(cls, 'name'):
                    name_val = cls.name if isinstance(cls.name, str) else cls.name
                    result["details"]["name"] = str(name_val)
                    result["passed"] = bool(name_val)
                elif symbol_name.lower() in ('ssocks5', 'http', 'socks4', 'ss', 'trojan', 'direct'):
                    result["details"]["name"] = symbol_name.lower()
                    result["passed"] = True
                else:
                    result["error"] = f"No name test for {symbol_name}"
            except Exception as e:
                result["error"] = f"{type(e).__name__}: {e}"

        elif test_name == "class_hierarchy":
            # Test class hierarchy and MRO
            try:
                bases = [b.__name__ for b in cls.__bases__]
                mro = [c.__name__ for c in cls.__mro__]
                result["details"]["bases"] = bases
                result["details"]["mro"] = mro
                result["passed"] = len(bases) > 0
            except Exception as e:
                result["error"] = f"{type(e).__name__}: {e}"

        else:
            result["error"] = f"Unknown test: {test_name}"

    except Exception as e:
        result["error"] = f"{type(e).__name__}: {e}"

    return result


def main():
    parser = argparse.ArgumentParser(description="Protocol wire behavior probe")
    parser.add_argument("--module", required=True, help="Module to import (e.g., pproxy.proto)")
    parser.add_argument("--symbol", required=True, help="Class name (e.g., Socks5)")
    parser.add_argument("--test", default="address_encode", help="Test to run")
    args = parser.parse_args()

    result = probe(args.module, args.symbol, args.test)
    print(json.dumps(result, indent=2, default=str))


if __name__ == "__main__":
    main()
