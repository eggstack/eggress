#!/usr/bin/env bash
# Compatibility wrapper for the graph-derived crates.io publisher.
#
# The hand-maintained 28-crate tier list and the fixed 660-second per-crate
# delay were removed: `cargo publish --workspace` remains nightly-only on the
# pinned stable toolchain (Cargo 1.89), so ordering now comes from
# `cargo metadata` via `scripts/publish-crates.py`, with resume support and
# reactive registry-throttle backoff instead of unconditional sleeps.
#
# Usage:
#   scripts/publish-remaining.sh [--dry-run]
#   scripts/publish-crates.py --list
#   scripts/publish-crates.py --dry-run
#   scripts/publish-crates.py --execute
#
# Prerequisites:
#   - crates.io credentials configured (`cargo login` or $CARGO_REGISTRY_TOKEN`).
#   - Working tree is on the published commit (clean, no uncommitted changes).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ "${1:-}" == "--dry-run" ]]; then
    exec python3 "$SCRIPT_DIR/publish-crates.py" --dry-run
fi

if [[ $# -gt 0 ]]; then
    echo "Usage: $(basename "$0") [--dry-run]" >&2
    echo "For listing or real publication use:" >&2
    echo "  python3 scripts/publish-crates.py --list" >&2
    echo "  python3 scripts/publish-crates.py --dry-run" >&2
    echo "  python3 scripts/publish-crates.py --execute" >&2
    exit 1
fi

exec python3 "$SCRIPT_DIR/publish-crates.py" --execute
