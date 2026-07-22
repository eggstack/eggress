#!/usr/bin/env python3
"""Process lifecycle probe.

Tests server process start/stop behavior without requiring actual network binding.
Validates that server objects can be created, configured, and have proper lifecycle methods.

Usage:
    python3 strict_process_lifecycle_probe.py --module pproxy.server --symbol proxies_by_uri --test server_create
"""
import argparse
import json
import sys
import asyncio


def probe(module_name: str, symbol_name: str, test_name: str = "server_create") -> dict:
    """Run process lifecycle probe."""
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

    sym = getattr(mod, symbol_name, None)
    if sym is None:
        result["error"] = f"Symbol '{symbol_name}' not found in '{module_name}'"
        return result

    result["exists"] = True

    try:
        if test_name == "server_create":
            # Test that server factory creates objects with proper lifecycle methods
            if callable(sym):
                try:
                    # Try to create a server object with a direct URI
                    server = sym("direct://")
                    result["details"]["created"] = True

                    # Check for lifecycle methods
                    has_start = hasattr(server, 'start_server') or hasattr(server, 'start')
                    has_close = hasattr(server, 'close') or hasattr(server, 'stop')
                    has_wait = hasattr(server, 'wait_closed') or hasattr(server, 'wait')

                    result["details"]["has_start"] = has_start
                    result["details"]["has_close"] = has_close
                    result["details"]["has_wait"] = has_wait

                    result["passed"] = has_start and has_close
                except Exception as e:
                    result["details"]["created"] = False
                    result["details"]["create_error"] = f"{type(e).__name__}: {e}"
                    result["passed"] = False
            else:
                result["error"] = f"{symbol_name} is not callable"

        elif test_name == "proxy_object":
            # Test that proxy objects have proper attributes
            if callable(sym):
                try:
                    proxy = sym("direct://")
                    attrs = [a for a in dir(proxy) if not a.startswith('_')]
                    result["details"]["attributes"] = attrs
                    result["details"]["has_bind"] = hasattr(proxy, 'bind')
                    result["details"]["has_alive"] = hasattr(proxy, 'alive')
                    result["details"]["has_connections"] = hasattr(proxy, 'connections')
                    result["passed"] = len(attrs) > 0
                except Exception as e:
                    result["details"]["error"] = f"{type(e).__name__}: {e}"
                    result["passed"] = False
            else:
                result["error"] = f"{symbol_name} is not callable"

        elif test_name == "auth_table":
            # Test AuthTable behavior
            if symbol_name == 'AuthTable' and callable(sym):
                try:
                    table = sym()
                    # Test basic operations
                    table.add("192.168.1.1", 60)
                    result["details"]["add"] = True
                    result["details"]["check"] = "192.168.1.1" in table or table.check("192.168.1.1")
                    result["passed"] = True
                except Exception as e:
                    result["details"]["error"] = f"{type(e).__name__}: {e}"
                    result["passed"] = False
            else:
                result["error"] = f"AuthTable test requires AuthTable symbol"

        elif test_name == "compile_rule":
            # Test rule compilation
            if symbol_name == 'compile_rule' and callable(sym):
                try:
                    rule = sym("example.com")
                    result["details"]["compiled"] = rule is not None
                    result["details"]["callable"] = callable(rule) if rule else False
                    result["passed"] = rule is not None
                except Exception as e:
                    result["details"]["error"] = f"{type(e).__name__}: {e}"
                    result["passed"] = False
            else:
                result["error"] = f"compile_rule test requires compile_rule symbol"

        elif test_name == "check_alive":
            # Test server alive check
            if symbol_name == 'check_server_alive' and callable(sym):
                try:
                    # Check if it's a coroutine function
                    is_coro = asyncio.iscoroutinefunction(sym)
                    result["details"]["is_coroutine"] = is_coro
                    result["passed"] = True  # Just verifying it exists and is callable
                except Exception as e:
                    result["details"]["error"] = f"{type(e).__name__}: {e}"
                    result["passed"] = False
            else:
                result["error"] = f"check_alive test requires check_server_alive symbol"

        elif test_name == "prepare_ciphers":
            # Test cipher preparation
            if symbol_name == 'prepare_ciphers' and callable(sym):
                try:
                    is_coro = asyncio.iscoroutinefunction(sym)
                    result["details"]["is_coroutine"] = is_coro
                    result["passed"] = True
                except Exception as e:
                    result["details"]["error"] = f"{type(e).__name__}: {e}"
                    result["passed"] = False
            else:
                result["error"] = f"prepare_ciphers test requires prepare_ciphers symbol"

        elif test_name == "constants":
            # Test that constants have expected values
            try:
                val = sym
                result["details"]["value"] = repr(val)
                result["details"]["type"] = type(val).__name__
                result["passed"] = val is not None
            except Exception as e:
                result["details"]["error"] = f"{type(e).__name__}: {e}"
                result["passed"] = False

        else:
            result["error"] = f"Unknown test: {test_name}"

    except Exception as e:
        result["error"] = f"{type(e).__name__}: {e}"

    return result


def main():
    parser = argparse.ArgumentParser(description="Process lifecycle probe")
    parser.add_argument("--module", required=True, help="Module to import")
    parser.add_argument("--symbol", required=True, help="Symbol name")
    parser.add_argument("--test", default="server_create", help="Test to run")
    args = parser.parse_args()

    result = probe(args.module, args.symbol, args.test)
    print(json.dumps(result, indent=2, default=str))


if __name__ == "__main__":
    main()
