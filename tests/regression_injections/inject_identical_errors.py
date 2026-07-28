"""Regression injection: identical errors must fail.

The paired comparator must NOT accept identical error strings as compatible
unless a pinned known-upstream-defect policy applies.
"""
import sys
sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parents[2] / "python" / "tests" / "strict"))

from conftest import compare_observations

oracle = {"exists": True, "error": "ConnectionRefused"}
candidate = {"exists": True, "error": "ConnectionRefused"}
result = compare_observations(oracle, candidate)

if result["all_match"]:
    print("ERROR: identical errors were accepted", file=sys.stderr)
    sys.exit(0)  # defect NOT detected
else:
    print("OK: identical errors correctly rejected")
    sys.exit(1)  # defect detected
