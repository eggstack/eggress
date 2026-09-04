#!/usr/bin/env bash
# Publish all 26 eggress-* crates to crates.io in dependency order.
#
# Prerequisites:
#   - crates.io credentials configured (`cargo login` or $CARGO_REGISTRY_TOKEN`).
#   - Working tree is on the published commit (clean, no uncommitted changes).
#
# Usage: scripts/publish-remaining.sh [--dry-run]
#
# Tier 1 publishes eggress-testkit and eggress-system-proxy first: several
# crates dev-depend on the testkit (resolved at publish time) and
# eggress-python depends on eggress-system-proxy non-optionally, so both must
# exist in the index before their dependents are published.
#
# Total: 26 crates. crates.io rate-limits new publishes to roughly one per 10
# minutes, so expect ~4h of wall time plus index-propagation waits.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

DRY_RUN=""
if [[ "${1:-}" == "--dry-run" ]]; then
    DRY_RUN="--dry-run"
fi

# Tiered publish order. Every required internal dep appears in an earlier tier
# than the crate that depends on it.
TIERS=(
    "eggress-testkit eggress-system-proxy"
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

# crates.io enforces a ~10-minute cooldown between new crate publishes.
# Respect it to avoid HTTP 429 rate-limit responses. Override with
# EGGRESS_PUBLISH_DELAY_SECONDS for dry-runs or local testing.
PUBLISH_DELAY_SECONDS="${EGGRESS_PUBLISH_DELAY_SECONDS:-660}"

# Wait for the crates.io index to see a freshly-published crate version.
# Polls the index endpoint; the inter-publish delay above is the dominant
# wait, so this is mostly a sanity check.
wait_for_index() {
    local crate="$1"
    local version="$2"
    local attempts="${3:-6}"
    for ((i=1; i<=attempts; i++)); do
        local resp
        resp=$(curl -fsS -H "User-Agent: eggress-release/1.0" \
                  "https://crates.io/api/v1/crates/${crate}/${version}" 2>/dev/null || echo "")
        if echo "$resp" | grep -q '"version"'; then
            return 0
        fi
        sleep 10
    done
    echo "WARN: index did not propagate for ${crate} ${version}; proceeding anyway" >&2
    return 0
}

publish_one() {
    local crate="$1"
    echo ""
    echo "=================================================================="
    echo "Publishing $crate (delay ${PUBLISH_DELAY_SECONDS}s)"
    echo "=================================================================="
    cargo publish $DRY_RUN -p "$crate" --no-verify
    if [[ -z "$DRY_RUN" ]]; then
        local version
        version=$(cargo read-manifest --manifest-path "crates/${crate}/Cargo.toml" 2>/dev/null \
            | python3 -c "import sys, json; print(json.load(sys.stdin)['version'])" 2>/dev/null || echo "1.0.3")
        wait_for_index "$crate" "$version"
        # Sleep between publishes to stay under the crates.io rate limit.
        sleep "$PUBLISH_DELAY_SECONDS"
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
