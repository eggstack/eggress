"""Shared observation loading helpers for strict differential tests."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any


def load_observation(obs_dir: Path, rid: str, side: str) -> dict[str, Any]:
    """Load an observation JSON file from the given directory.

    Args:
        obs_dir: Root observation directory
        rid: Record ID (e.g. "python.pproxy")
        side: "oracle" or "candidate"
    """
    filename = f"{rid.replace('.', '_')}_{side}.json"
    filepath = obs_dir / filename
    if not filepath.exists():
        return {"exists": False, "error": f"Observation file not found: {filepath}"}
    return json.loads(filepath.read_text())
