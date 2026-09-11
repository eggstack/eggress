#!/usr/bin/env bash
# Release preflight for canonical CLI binary artifacts.
#
# Validates the tag/version invariants shared by the Python wheel pipeline
# and the prebuilt `eggress`/`pproxy` binary pipeline. Used by
# `.github/workflows/release-binaries.yml` (tag pushes and manual dispatch
# with an existing tag input) and runnable locally by a maintainer:
#
#   scripts/release-preflight.sh --tag v1.2.3
#   scripts/release-preflight.sh --check-versions-only
#
# Hard-fails unless:
#   1. the tag has the form `vX.Y.Z` (production tags only; no pre-release);
#   2. the tag equals `v<workspace.package.version>`;
#   3. `crates/eggress-python/pyproject.toml` carries the same version;
#   4. `python-pproxy-compat/pyproject.toml` is aligned (version + eggress pin);
#   5. every internal exact `=x.y.z` pin in `[workspace.dependencies]` matches;
#   6. the checked-out commit is exactly the tagged commit;
#   7. the working tree is clean (local/manual mode; CI checkouts are clean).
#
# The binary release workflow builds from the tag commit, never from whatever
# happens to be the default-branch head. Manual dispatch must supply an
# existing tag; it never synthesizes an untagged production release.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

TAG=""
CHECK_VERSIONS_ONLY=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --tag)
            TAG="${2:-}"
            shift 2
            ;;
        --tag=*)
            TAG="${1#--tag=}"
            shift
            ;;
        --check-versions-only)
            CHECK_VERSIONS_ONLY=1
            shift
            ;;
        -h|--help)
            sed -n '2,/^$/p' "$0"
            echo "Usage: $0 [--tag vX.Y.Z] [--check-versions-only]"
            exit 0
            ;;
        *)
            echo "ERROR: unknown argument '$1' (usage: $0 [--tag vX.Y.Z] [--check-versions-only])" >&2
            exit 1
            ;;
    esac
done

if [[ "$CHECK_VERSIONS_ONLY" -eq 1 && -n "$TAG" ]]; then
    echo "ERROR: --check-versions-only cannot be combined with --tag" >&2
    exit 1
fi

python3 - "$TAG" "$CHECK_VERSIONS_ONLY" <<'PY'
import re
import sys
import tomllib
from pathlib import Path

tag = sys.argv[1]
versions_only = sys.argv[2] == "1"

def field(path: str, *keys: str) -> str:
    value = tomllib.loads(Path(path).read_text())
    for key in keys:
        if not isinstance(value, dict) or key not in value:
            raise SystemExit(f"::error::{path} has no field {'/'.join(keys)}")
        value = value[key]
    if not isinstance(value, str) or not value:
        raise SystemExit(f"::error::{path} has no string field {'/'.join(keys)}")
    return value

workspace = field("Cargo.toml", "workspace", "package", "version")
print(f"workspace version: {workspace}")

# Binding crate inherits its version from the workspace (`version.workspace`).
binding_manifest = tomllib.loads(Path("crates/eggress-python/Cargo.toml").read_text())
binding_version = binding_manifest.get("package", {}).get("version")
if isinstance(binding_version, dict):
    if binding_version.get("workspace") is not True:
        raise SystemExit("::error::crates/eggress-python/Cargo.toml package/version must be a string or workspace-inherited")
    binding = workspace
elif isinstance(binding_version, str) and binding_version:
    binding = binding_version
else:
    raise SystemExit("::error::crates/eggress-python/Cargo.toml has no package/version")

project = field("crates/eggress-python/pyproject.toml", "project", "version")
compat_version = field("python-pproxy-compat/pyproject.toml", "project", "version")
print(f"binding version: {binding}")
print(f"eggress wheel version: {project}")
print(f"pproxy-compat version: {compat_version}")

# The opt-in compat distribution must pin the exact eggress release.
compat_pyproject = tomllib.loads(Path("python-pproxy-compat/pyproject.toml").read_text())
deps = compat_pyproject.get("project", {}).get("dependencies", [])
if f"eggress=={workspace}" not in deps:
    raise SystemExit(f"::error::python-pproxy-compat/pyproject.toml must depend on eggress=={workspace}, found: {deps}")
print(f"pproxy-compat eggress pin: eggress=={workspace}")

# Every internal exact pin under [workspace.dependencies] must match.
root = tomllib.loads(Path("Cargo.toml").read_text())
ws_deps = root.get("workspace", {}).get("dependencies", {})
mismatched = []
for name, spec in sorted(ws_deps.items()):
    if isinstance(spec, dict) and isinstance(spec.get("version"), str) and spec["version"].startswith("="):
        pinned = spec["version"][1:]
        if pinned != workspace:
            mismatched.append(f"{name}={pinned}")
if mismatched:
    raise SystemExit(f"::error::internal exact version pins mismatch workspace {workspace}: {', '.join(mismatched)}")
print(f"internal exact pins: aligned at {workspace} ({len([s for s in ws_deps.values() if isinstance(s, dict) and str(s.get('version', '')).startswith('=')])} pins)")

versions = {"workspace": workspace, "binding": binding, "pyproject": project, "compat": compat_version}
if len(set(versions.values())) != 1:
    raise SystemExit(f"::error::version mismatch: {versions}")
print(f"lockstep versions aligned: {workspace}")

if versions_only:
    print("version coherence verified (no tag checks requested)")
    sys.exit(0)

if not tag:
    raise SystemExit("::error::a tag is required (pass --tag vX.Y.Z or --check-versions-only for local coherence only)")
if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag):
    raise SystemExit(f"::error::production tag must be vMAJOR.MINOR.PATCH: {tag}")
expected = tag[1:]
if expected != workspace:
    raise SystemExit(f"::error::tag version {expected} != workspace version {workspace}")
print(f"tag version matches: {tag}")

# Persist the bare version for workflow outputs.
Path("/tmp/eggress-release-version").write_text(workspace + "\n")
PY

if [[ "$CHECK_VERSIONS_ONLY" -eq 1 ]]; then
    echo "preflight: version coherence OK"
    exit 0
fi

# Git checks: the checkout must be exactly the tagged commit, with a clean tree.
if ! git rev-parse --verify "refs/tags/${TAG}^{commit}" >/dev/null 2>&1; then
    # Fall back to resolving the tag name directly (lightweight tags, or a
    # local tag namespace without refs/tags/ prefix in some CI checkouts).
    if ! git rev-parse --verify "${TAG}^{commit}" >/dev/null 2>&1; then
        echo "ERROR: tag '${TAG}' not found locally; fetch tags before releasing (git fetch --tags)" >&2
        exit 1
    fi
    TAG_COMMIT="$(git rev-parse --verify "${TAG}^{commit}")"
else
    TAG_COMMIT="$(git rev-parse --verify "refs/tags/${TAG}^{commit}")"
fi
HEAD_COMMIT="$(git rev-parse HEAD)"
if [[ "$HEAD_COMMIT" != "$TAG_COMMIT" ]]; then
    echo "ERROR: checked-out commit ${HEAD_COMMIT} is not the tagged commit ${TAG_COMMIT} for ${TAG}" >&2
    echo "The binary release workflow builds from the tag commit, never from the default-branch head." >&2
    exit 1
fi
echo "tag commit matches: ${TAG} -> ${TAG_COMMIT}"

if [[ -n "$(git status --porcelain)" ]]; then
    echo "ERROR: working tree is not clean; commit or stash changes before releasing" >&2
    git status --short >&2
    exit 1
fi
echo "working tree clean"

echo "preflight: ${TAG} OK (version ${TAG#v})"
