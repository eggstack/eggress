"""Regression injection: default value mismatch must fail.

Different default values must not be treated as compatible.
"""
import sys
sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parents[2] / "python" / "tests" / "strict"))

from conftest import compare_observations

oracle = {"exists": True, "type": "function", "qualname": "test.f",
          "is_coroutine": False, "is_callable": True, "signature": "(host, port=80)"}
candidate = {"exists": True, "type": "function", "qualname": "test.f",
             "is_coroutine": False, "is_callable": True, "signature": "(host, port=443)"}
result = compare_observations(oracle, candidate)

if result["all_match"]:
    print("ERROR: default mismatch accepted", file=sys.stderr)
    sys.exit(0)  # defect NOT detected
else:
    print("OK: default mismatch correctly rejected")
    sys.exit(1)  # defect detected
