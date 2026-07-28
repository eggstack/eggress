"""Regression injection: missing observation file must fail.

A missing observation file must be reported as exists=False, not exists=True.
"""
import sys
import json
from pathlib import Path


def load_observation(obs_dir, rid, side):
    filename = f"{rid.replace('.', '_')}_{side}.json"
    filepath = obs_dir / filename
    if not filepath.exists():
        return {"exists": False, "error": f"Observation file not found: {filepath}"}
    return json.loads(filepath.read_text())


obs_dir = Path(__file__).resolve().parents[2] / "target" / "injection_nonexistent"
obs_dir.mkdir(parents=True, exist_ok=True)

result = load_observation(obs_dir, "test.missing", "oracle")

if result.get("exists", False):
    print("ERROR: missing file returned exists=True", file=sys.stderr)
    sys.exit(0)  # defect NOT detected
else:
    print("OK: missing file correctly detected")
    sys.exit(1)  # defect detected
