#!/usr/bin/env bash
set -euo pipefail
# Install shadowsocks-rust for interop testing
cargo install shadowsocks-rust --features "local,server"
echo "Installed ssserver and sslocal to ~/.cargo/bin/"
