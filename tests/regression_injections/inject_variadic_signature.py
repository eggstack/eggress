"""Regression injection: variadic signature wrapper must fail.

A variadic (*args, **kwargs) signature must not automatically match
an explicit (host, port) signature.
"""
import sys
sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parents[2] / "python" / "tests" / "strict"))

from conftest import compare_observations

oracle = {"exists": True, "type": "function", "qualname": "test.f",
          "is_coroutine": False, "is_callable": True, "signature": "(host, port)"}
candidate = {"exists": True, "type": "function", "qualname": "test.f",
             "is_coroutine": False, "is_callable": True, "signature": "(*args, **kwargs)"}
result = compare_observations(oracle, candidate)

if result["all_match"]:
    print("ERROR: variadic wrapper accepted", file=sys.stderr)
    sys.exit(0)  # defect NOT detected
else:
    print("OK: variadic wrapper correctly rejected")
    sys.exit(1)  # defect detected
