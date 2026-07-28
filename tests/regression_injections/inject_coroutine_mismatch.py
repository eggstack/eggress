"""Regression injection: coroutine kind mismatch must fail.

A coroutine must not match a non-coroutine.
"""
import sys
sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parents[2] / "python" / "tests" / "strict"))

from conftest import compare_observations

oracle = {"exists": True, "type": "function", "qualname": "test.f",
          "is_coroutine": True, "is_callable": True, "signature": "(host, port)"}
candidate = {"exists": True, "type": "function", "qualname": "test.f",
             "is_coroutine": False, "is_callable": True, "signature": "(host, port)"}
result = compare_observations(oracle, candidate)

if result["all_match"]:
    print("ERROR: coroutine kind mismatch accepted", file=sys.stderr)
    sys.exit(0)  # defect NOT detected
else:
    print("OK: coroutine kind mismatch correctly rejected")
    sys.exit(1)  # defect detected
