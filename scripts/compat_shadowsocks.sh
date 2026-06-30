#!/usr/bin/env bash
set -euo pipefail
# Run Shadowsocks interop tests against external ssserver/sslocal
export EGRESS_REQUIRE_SHADOWSOCKS_INTEROP=1
cargo test -p eggress-cli --test interoperability_shadowsocks -- --ignored --nocapture "$@"
