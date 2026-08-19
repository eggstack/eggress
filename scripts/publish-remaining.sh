#!/usr/bin/env bash
# Publish the remaining 24 eggress-* crates to crates.io in dependency order.
#
# Prerequisites:
#   - eggress-uri cooldown has cleared (was 2026-08-20T08:01:26Z).
#   - crates.io credentials configured (`cargo login` or $CARGO_REGISTRY_TOKEN`).
#   - Working tree is on the published commit (clean, no uncommitted changes).
#
# Usage: scripts/publish-remaining.sh [--dry-run]
#
# Crates already published (do not include here):
#   eggress-system-proxy 1.0.2
#   eggress-testkit     1.0.2
#
# Total: 24 crates. Expected duration with index-propagation waits: 30-45 min.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

DRY_RUN=""
if [[ "${1:-}" == "--dry-run" ]]; then
    DRY_RUN="--dry-run"
fi

# Tiered publish order. Every required internal dep appears in an earlier tier
# than the crate that depends on it. The eggress-uri cooldown means tier 1
# cannot run until ~20h after the name was deleted; this script assumes the
# cooldown has cleared.
TIERS=(
    "eggress-uri"
    "eggress-core"
    "eggress-protocol-raw eggress-protocol-http eggress-protocol-socks eggress-protocol-websocket eggress-transport-tls eggress-transport-ssh eggress-transport-quic eggress-protocol-reverse eggress-routing eggress-protocol-shadowsocks"
    "eggress-protocol-trojan eggress-protocol-h3 eggress-udp"
    "eggress-config eggress-server"
    "eggress-metrics"
    "eggress-pproxy-compat eggress-runtime"
    "eggress-admin eggress-embed eggress-cli"
    "eggress-python"
)

# Wait for the crates.io index to see a freshly-published crate version.
# Polls the index endpoint with a 5s ceiling between attempts.
wait_for_index() {
    local crate="$1"
    local version="$2"
    local attempts="${3:-30}"
    for ((i=1; i<=attempts; i++)); do
        local resp
        resp=$(curl -fsS -H "User-Agent: eggress-release/1.0" \
                  "https://crates.io/api/v1/crates/${crate}/${version}" 2>/dev/null || echo "")
        if echo "$resp" | grep -q '"version"'; then
            return 0
        fi
        sleep 5
    done
    echo "WARN: index did not propagate for ${crate} ${version} after $((attempts * 5))s; proceeding anyway" >&2
    return 0
}

publish_one() {
    local crate="$1"
    echo ""
    echo "=================================================================="
    echo "Publishing $crate"
    echo "=================================================================="
    cargo publish $DRY_RUN -p "$crate" --no-verify
    if [[ -z "$DRY_RUN" ]]; then
        # Pull the version from the just-published manifest; cargo's --no-verify
        # path doesn't print it, so we infer from the workspace.
        local version
        version=$(cargo read-manifest --manifest-path "crates/${crate}/Cargo.toml" 2>/dev/null \
            | python3 -c "import sys, json; print(json.load(sys.stdin)['version'])" 2>/dev/null || echo "1.0.2")
        wait_for_index "$crate" "$version"
    fi
}

tier=0
for tier_crates in "${TIERS[@]}"; do
    tier=$((tier + 1))
    echo ""
    echo "##################################################################"
    echo "## Tier $tier"
    echo "##################################################################"
    for c in $tier_crates; do
        publish_one "$c"
    done
done

echo ""
echo "=================================================================="
echo "Publish run complete. Dry-run mode: ${DRY_RUN:-no}"
echo "=================================================================="
