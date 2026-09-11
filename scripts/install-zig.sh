#!/usr/bin/env bash
# Install a pinned Zig toolchain for portable Linux GNU builds.
#
# The CLI binary release uses `cargo-zigbuild` with an explicit glibc floor
# (currently 2.17, matching the manylinux2014 Python wheel floor) so Linux
# artifacts do not accidentally depend on the newest GitHub runner glibc.
# This script owns Zig download/version logic so the release workflow does
# not duplicate it across Linux jobs.
#
# Usage: scripts/install-zig.sh [--version 0.13.0] [--dir "$HOME/.local/zig"]
#
# In GitHub Actions the install directory is added to $GITHUB_PATH when set.

set -euo pipefail

ZIG_VERSION="0.13.0"
INSTALL_DIR="${HOME:-/tmp}/.local/zig"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version)
            ZIG_VERSION="${2:-}"
            shift 2
            ;;
        --version=*)
            ZIG_VERSION="${1#--version=}"
            shift
            ;;
        --dir)
            INSTALL_DIR="${2:-}"
            shift 2
            ;;
        --dir=*)
            INSTALL_DIR="${1#--dir=}"
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [--version 0.13.0] [--dir DIR]"
            exit 0
            ;;
        *)
            echo "ERROR: unknown argument '$1'" >&2
            exit 1
            ;;
    esac
done

ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64) ZIG_ARCH="x86_64" ;;
    aarch64|arm64) ZIG_ARCH="aarch64" ;;
    *)
        echo "ERROR: unsupported host architecture for Zig install: $ARCH" >&2
        exit 1
        ;;
esac

TARBALL="zig-linux-${ZIG_ARCH}-${ZIG_VERSION}.tar.xz"
URL="https://ziglang.org/download/${ZIG_VERSION}/${TARBALL}"

echo "installing Zig ${ZIG_VERSION} (${ZIG_ARCH}) to ${INSTALL_DIR}"
mkdir -p "${INSTALL_DIR}"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

curl -fsSL --retry 3 --retry-delay 2 -o "${TMPDIR}/${TARBALL}" "$URL"
tar -xf "${TMPDIR}/${TARBALL}" -C "$TMPDIR"
EXTRACTED="$(echo "${TMPDIR}"/zig-linux-"${ZIG_ARCH}"-"${ZIG_VERSION}")"
rm -rf "${INSTALL_DIR:?:?}/zig-linux-${ZIG_ARCH}-${ZIG_VERSION}"
mv "$EXTRACTED" "$INSTALL_DIR/zig-linux-${ZIG_ARCH}-${ZIG_VERSION}"
mkdir -p "${INSTALL_DIR}/bin"
ln -sf "${INSTALL_DIR}/zig-linux-${ZIG_ARCH}-${ZIG_VERSION}/zig" "${INSTALL_DIR}/bin/zig"

if [[ -n "${GITHUB_PATH:-}" ]]; then
    echo "${INSTALL_DIR}/bin" >> "$GITHUB_PATH"
else
    export PATH="${INSTALL_DIR}/bin:${PATH}"
fi

"${INSTALL_DIR}/bin/zig" version
