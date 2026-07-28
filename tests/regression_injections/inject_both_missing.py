"""Regression injection: both-missing observations must fail.

The paired comparator must NOT accept mutual absence as a match.
This script verifies that compare_observations rejects both-missing.
"""
import sys
sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parents[2] / "python" / "tests" / "strict"))

from conftest import compare_observations

oracle = {"exists": False, "error": "not found"}
candidate = {"exists": False, "error": "not found"}
result = compare_observations(oracle, candidate)

if result["all_match"]:
    print("ERROR: both-missing was accepted as match", file=sys.stderr)
    sys.exit(0)  # defect NOT detected
else:
    print("OK: both-missing correctly rejected")
    sys.exit(1)  # defect detected (gate fails)
