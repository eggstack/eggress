#!/usr/bin/env bash
# Eggress standalone CLI installer (Unix).
#
# Installs prebuilt `eggress` and `pproxy` executables from a GitHub Release
# as one version-aligned unit. Release binaries use the default `eggress-cli`
# feature set; custom features (ssh, quic, legacy crypto, pproxy legacy)
# require a Cargo/source build.
#
# Usage:
#   curl -fsSL https://github.com/eggstack/eggress/releases/latest/download/install.sh | bash
#   bash install.sh --version X.Y.Z --dir /custom/bin
#
# Integrity note: the SHA-256 sidecar is downloaded from the same GitHub
# Release as the archive. It detects corruption or mismatched assets; it is
# not an independent signature or publisher-authentication guarantee.
#
# Test-only override: set EGRESS_RELEASE_BASE_URL to a base URL (for example
# file:///tmp/fixture-release) to resolve assets from local fixtures or a mock
# endpoint instead of github.com. Not for production use.

if [ -z "${BASH_VERSION:-}" ]; then
  echo "error: this installer requires bash (you may have piped it to sh); rerun with: bash install.sh" >&2
  exit 1
fi

set -euo pipefail

REPO="eggstack/eggress"
DEFAULT_BASE_URL="https://github.com/${REPO}/releases"
VERSION=""
INSTALL_DIR=""

usage() {
  cat <<'USAGE'
Usage: install.sh [--version X.Y.Z] [--dir PATH]

Installs prebuilt `eggress` and `pproxy` binaries from the Eggress GitHub Release.

Options:
  --version X.Y.Z   Install a pinned release (exact tag vX.Y.Z). Default: latest stable.
  --dir PATH        Install directory. Default: /usr/local/bin for root, $HOME/.local/bin otherwise.
  -h, --help        Show this help and exit.
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version)
      VERSION="${2:-}"
      shift 2
      ;;
    --version=*)
      VERSION="${1#--version=}"
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
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument '$1' (see --help)" >&2
      exit 1
      ;;
  esac
done

if [ -n "$VERSION" ]; then
  case "$VERSION" in
    *[!0-9.]*|*..*|.*|*.|"")
      echo "error: --version must be X.Y.Z (e.g. --version 1.2.3), got '$VERSION'" >&2
      exit 1
      ;;
  esac
  if [ "$(printf '%s' "$VERSION" | awk -F. '{print NF}')" != "3" ]; then
    echo "error: --version must be X.Y.Z (e.g. --version 1.2.3), got '$VERSION'" >&2
    exit 1
  fi
fi

OS="$(uname -s)"
ARCH="$(uname -m)"
case "${OS}:${ARCH}" in
  Linux:x86_64|Linux:amd64) TARGET="x86_64-unknown-linux-gnu" ;;
  Linux:aarch64|Linux:arm64) TARGET="aarch64-unknown-linux-gnu" ;;
  Darwin:x86_64) TARGET="x86_64-apple-darwin" ;;
  Darwin:arm64|Darwin:aarch64) TARGET="aarch64-apple-darwin" ;;
  *)
    echo "error: no prebuilt Eggress release for ${OS}/${ARCH}" >&2
    echo "Install with Cargo instead: cargo install eggress-cli --locked" >&2
    echo "See docs/INSTALLATION.md for source builds and custom features." >&2
    exit 1
    ;;
esac

ARCHIVE="eggress-${TARGET}.tar.gz"
CHECKSUM_FILE="${ARCHIVE}.sha256"

BASE_URL="${EGRESS_RELEASE_BASE_URL:-$DEFAULT_BASE_URL}"
if [ -n "$VERSION" ]; then
  TAG="v${VERSION}"
  RELEASE_URL="${BASE_URL}/download/${TAG}"
else
  TAG="latest"
  RELEASE_URL="${BASE_URL}/latest/download"
fi

if [ -z "$INSTALL_DIR" ]; then
  if [ "$(id -u)" -eq 0 ]; then
    INSTALL_DIR="/usr/local/bin"
  else
    INSTALL_DIR="${HOME}/.local/bin"
  fi
fi

# Never invoke sudo internally. If the destination is not writable, fail with
# an explicit command the user can choose to rerun with appropriate privilege.
if [ ! -d "$INSTALL_DIR" ]; then
  mkdir -p "$INSTALL_DIR"
fi
if [ ! -w "$INSTALL_DIR" ]; then
  echo "error: install directory '$INSTALL_DIR' is not writable" >&2
  echo "Rerun with a user-writable --dir, or rerun from a shell with write access, for example:" >&2
  echo "  bash install.sh --dir \"\$HOME/.local/bin\"" >&2
  echo "  mkdir -p \"$INSTALL_DIR\" && bash install.sh --dir \"$INSTALL_DIR\"" >&2
  exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "error: curl is required to download the release archive" >&2
  exit 1
fi
if ! command -v tar >/dev/null 2>&1; then
  echo "error: tar is required to extract the release archive" >&2
  exit 1
fi

TMPDIR="$(mktemp -d)"
cleanup() {
  rm -rf "$TMPDIR"
}
trap cleanup EXIT

echo "downloading ${ARCHIVE} (${TAG}) for ${TARGET}"
curl -fsSL --retry 3 --retry-delay 2 --connect-timeout 15 --max-time 180 \
  -o "${TMPDIR}/${ARCHIVE}" "${RELEASE_URL}/${ARCHIVE}"
curl -fsSL --retry 3 --retry-delay 2 --connect-timeout 15 --max-time 60 \
  -o "${TMPDIR}/${CHECKSUM_FILE}" "${RELEASE_URL}/${CHECKSUM_FILE}"

# Verify SHA-256 of the complete archive before extraction.
EXPECTED="$(awk '{print $1}' "${TMPDIR}/${CHECKSUM_FILE}")"
if [ -z "$EXPECTED" ]; then
  echo "error: checksum file is empty or malformed: ${CHECKSUM_FILE}" >&2
  exit 1
fi
case "$EXPECTED" in
  *[!0-9a-fA-F]*|"")
    echo "error: checksum file is malformed (expected hex SHA-256): ${CHECKSUM_FILE}" >&2
    exit 1
    ;;
esac
if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL="$(sha256sum "${TMPDIR}/${ARCHIVE}" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
  ACTUAL="$(shasum -a 256 "${TMPDIR}/${ARCHIVE}" | awk '{print $1}')"
else
  echo "error: sha256sum or shasum is required to verify the download" >&2
  exit 1
fi
if [ "$ACTUAL" != "$EXPECTED" ]; then
  echo "error: SHA-256 mismatch for ${ARCHIVE}" >&2
  echo "  expected: $EXPECTED" >&2
  echo "  actual:   $ACTUAL" >&2
  exit 1
fi
echo "checksum verified: ${ARCHIVE}"

tar -xzf "${TMPDIR}/${ARCHIVE}" -C "$TMPDIR"
chmod +x "${TMPDIR}/eggress" "${TMPDIR}/pproxy"

# Verify both staged executables before installing either one.
STAGED_EGGRESS="$("${TMPDIR}/eggress" version)"
case "$STAGED_EGGRESS" in
  "eggress "[0-9]*.[0-9]*.[0-9]*) ;;
  *)
    echo "error: staged eggress version mismatch: '$STAGED_EGGRESS'" >&2
    exit 1
    ;;
esac
STAGED_EGGRESS_VERSION="${STAGED_EGGRESS#eggress }"
STAGED_PPROXY="$("${TMPDIR}/pproxy" --version)"
STAGED_PPROXY_VERSION="$(printf '%s' "$STAGED_PPROXY" | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"
if [ -z "$STAGED_PPROXY_VERSION" ]; then
  echo "error: staged pproxy version mismatch: '$STAGED_PPROXY'" >&2
  exit 1
fi
if [ "$STAGED_EGGRESS_VERSION" != "$STAGED_PPROXY_VERSION" ]; then
  echo "error: staged binary versions disagree: eggress $STAGED_EGGRESS_VERSION vs pproxy $STAGED_PPROXY_VERSION" >&2
  exit 1
fi
if [ -n "$VERSION" ] && [ "$STAGED_EGGRESS_VERSION" != "$VERSION" ]; then
  echo "error: staged version $STAGED_EGGRESS_VERSION != requested version $VERSION" >&2
  exit 1
fi
echo "staged versions verified: eggress $STAGED_EGGRESS_VERSION / pproxy $STAGED_PPROXY_VERSION"

cp "${TMPDIR}/eggress" "${INSTALL_DIR}/eggress"
cp "${TMPDIR}/pproxy" "${INSTALL_DIR}/pproxy"
chmod +x "${INSTALL_DIR}/eggress" "${INSTALL_DIR}/pproxy"

echo "installed eggress $STAGED_EGGRESS_VERSION to ${INSTALL_DIR}/eggress"
echo "installed pproxy $STAGED_PPROXY_VERSION to ${INSTALL_DIR}/pproxy"

case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    if [ "$INSTALL_DIR" = "${HOME}/.local/bin" ]; then
      echo "note: ${INSTALL_DIR} is not in PATH; add it to use eggress without a full path:" >&2
      echo "  export PATH=\"\$HOME/.local/bin:\$PATH\"" >&2
    else
      echo "note: ${INSTALL_DIR} is not in PATH" >&2
    fi
    ;;
esac
