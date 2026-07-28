"""Regression injection: signature mismatch must fail.

An extra required parameter must not match the original signature.
"""
import sys
sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parents[2] / "python" / "tests" / "strict"))

from conftest import compare_observations

oracle = {"exists": True, "type": "function", "qualname": "test.f",
          "is_coroutine": False, "is_callable": True, "signature": "(host, port)"}
candidate = {"exists": True, "type": "function", "qualname": "test.f",
             "is_coroutine": False, "is_callable": True, "signature": "(host, port, extra=True)"}
result = compare_observations(oracle, candidate)

if result["all_match"]:
    print("ERROR: signature mismatch accepted", file=sys.stderr)
    sys.exit(0)  # defect NOT detected
else:
    print("OK: signature mismatch correctly rejected")
    sys.exit(1)  # defect detected
