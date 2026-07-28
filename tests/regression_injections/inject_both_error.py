"""Regression injection: both-error observations must fail.

Two errors must not be treated as compatible unless a pinned
known-upstream-defect policy applies.
"""
import sys
sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parents[2] / "python" / "tests" / "strict"))

from conftest import compare_observations

oracle = {"exists": True, "error": "TimeoutError: connection timed out"}
candidate = {"exists": True, "error": "TimeoutError: connection timed out"}
result = compare_observations(oracle, candidate)

if result["all_match"]:
    print("ERROR: both-error was accepted as match", file=sys.stderr)
    sys.exit(0)  # defect NOT detected
else:
    print("OK: both-error correctly rejected")
    sys.exit(1)  # defect detected
