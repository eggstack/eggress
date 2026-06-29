#!/usr/bin/env python3
"""Manual smoke tests for common proxy clients.

These tests exercise real client behavior through eggress and catch
client-specific tolerance issues that differential tests may miss.

Usage:
    1. Start an eggress server: cargo run --bin eggress -- -l http://127.0.0.1:8080
    2. Run this script: python3 scripts/smoke_clients.py

Requirements:
    - Python 3.8+
    - An eggress server running on 127.0.0.1:8080

Exit code 0 means all tests passed, 1 means at least one failure.
"""

import sys
import urllib.request
import urllib.error
import json


def test_urllib_http_get(proxy_url="http://127.0.0.1:8080", target="http://httpbin.org/get"):
    """Send a GET request through the HTTP proxy using urllib."""
    proxy_handler = urllib.request.ProxyHandler({"http": proxy_url})
    opener = urllib.request.build_opener(proxy_handler)
    try:
        response = opener.open(target, timeout=10)
        body = response.read()
        print(f"  [PASS] urllib GET {target}: status={response.status}, bytes={len(body)}")
        return True
    except urllib.error.URLError as e:
        print(f"  [FAIL] urllib GET {target}: {e}")
        return False
    except Exception as e:
        print(f"  [FAIL] urllib GET {target}: {type(e).__name__}: {e}")
        return False


def test_urllib_http_head(proxy_url="http://127.0.0.1:8080", target="http://httpbin.org/get"):
    """Send a HEAD request through the HTTP proxy using urllib."""
    proxy_handler = urllib.request.ProxyHandler({"http": proxy_url})
    opener = urllib.request.build_opener(proxy_handler)
    req = urllib.request.Request(target, method="HEAD")
    try:
        response = opener.open(req, timeout=10)
        body = response.read()
        print(f"  [PASS] urllib HEAD {target}: status={response.status}, bytes={len(body)}")
        return True
    except urllib.error.URLError as e:
        print(f"  [FAIL] urllib HEAD {target}: {e}")
        return False
    except Exception as e:
        print(f"  [FAIL] urllib HEAD {target}: {type(e).__name__}: {e}")
        return False


def test_urllib_http_post(proxy_url="http://127.0.0.1:8080", target="http://httpbin.org/post"):
    """Send a POST request with a JSON body through the HTTP proxy."""
    proxy_handler = urllib.request.ProxyHandler({"http": proxy_url})
    opener = urllib.request.build_opener(proxy_handler)
    data = json.dumps({"hello": "world"}).encode("utf-8")
    req = urllib.request.Request(
        target,
        data=data,
        headers={"Content-Type": "application/json"},
    )
    try:
        response = opener.open(req, timeout=10)
        body = response.read()
        print(f"  [PASS] urllib POST {target}: status={response.status}, bytes={len(body)}")
        return True
    except urllib.error.URLError as e:
        print(f"  [FAIL] urllib POST {target}: {e}")
        return False
    except Exception as e:
        print(f"  [FAIL] urllib POST {target}: {type(e).__name__}: {e}")
        return False


def main():
    proxy_url = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8080"
    print(f"Smoke tests through proxy: {proxy_url}")
    print()

    results = []
    results.append(test_urllib_http_get(proxy_url))
    results.append(test_urllib_http_head(proxy_url))
    results.append(test_urllib_http_post(proxy_url))

    print()
    passed = sum(results)
    total = len(results)
    if passed == total:
        print(f"All {total} smoke tests passed.")
        return 0
    else:
        print(f"{passed}/{total} smoke tests passed.")
        return 1


if __name__ == "__main__":
    sys.exit(main())
