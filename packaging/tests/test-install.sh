#!/usr/bin/env bash
# Focused installer contract tests for packaging/install.sh.
#
# Uses local file:// fixtures (stub executables + a small mock release
# layout) so no production release is consumed. Covers the Phase H contract:
# target mapping, archive naming, checksum success/mismatch, requested-version
# mismatch, candidate eggress/pproxy version mismatch, unsupported target,
# non-writable destination, custom --dir, version-pinned URL selection, and
# missing-binary refusal.
#
# Usage: packaging/tests/test-install.sh

set -u
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INSTALLER="$REPO_ROOT/packaging/install.sh"

PASS=0
FAIL=0

pass() { PASS=$((PASS + 1)); echo "ok - $1"; }
fail() { FAIL=$((FAIL + 1)); echo "FAIL - $1${2:+: $2}"; }

detect_target() {
  OS="$(uname -s)"
  ARCH="$(uname -m)"
  case "${OS}:${ARCH}" in
    Linux:x86_64|Linux:amd64) echo "x86_64-unknown-linux-gnu" ;;
    Linux:aarch64|Linux:arm64) echo "aarch64-unknown-linux-gnu" ;;
    Darwin:x86_64) echo "x86_64-apple-darwin" ;;
    Darwin:arm64|Darwin:aarch64) echo "aarch64-apple-darwin" ;;
    *) echo "unsupported" ;;
  esac
}

TARGET="$(detect_target)"
if [[ "$TARGET" == "unsupported" ]]; then
  echo "SKIP: unsupported host for fixture tests ($(uname -s)/$(uname -m))"
  exit 0
fi
ARCHIVE="eggress-${TARGET}.tar.gz"

# Build a mock release tree at $1 with stub binaries reporting $2
# (eggress_version=$3, pproxy_version=$4, include_pproxy=$5).
make_fixture() {
  local root="$1" version="$2" eggress_v="${3:-$2}" pproxy_v="${4:-$2}" include_pproxy="${5:-yes}"
  local stage="$root/stage"
  mkdir -p "$stage"
  cat > "$stage/eggress" <<EOF
#!/usr/bin/env bash
if [[ "\${1:-}" == "version" ]]; then echo "eggress $eggress_v"; exit 0; fi
echo "eggress stub help"
EOF
  chmod +x "$stage/eggress"
  if [[ "$include_pproxy" == "yes" ]]; then
    cat > "$stage/pproxy" <<EOF
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then echo "eggress-pproxy-compat $pproxy_v"; exit 0; fi
echo "pproxy stub help"
EOF
    chmod +x "$stage/pproxy"
    tar -czf "$root/$ARCHIVE" -C "$stage" eggress pproxy
  else
    tar -czf "$root/$ARCHIVE" -C "$stage" eggress
  fi
  python3 -c "import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest()+'  '+sys.argv[1].split('/')[-1])" "$root/$ARCHIVE" > "$root/$ARCHIVE.sha256"
  mkdir -p "$root/download/v$version" "$root/latest/download"
  cp "$root/$ARCHIVE" "$root/$ARCHIVE.sha256" "$root/download/v$version/"
  cp "$root/$ARCHIVE" "$root/$ARCHIVE.sha256" "$root/latest/download/"
}

# --- static contract -------------------------------------------------------

if bash -n "$INSTALLER"; then pass "bash syntax check"; else fail "bash syntax check"; fi

for t in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin; do
  if grep -q "$t" "$INSTALLER"; then pass "target mapping contains $t"; else fail "target mapping contains $t"; fi
done

if grep -q 'eggress-${TARGET}.tar.gz' "$INSTALLER"; then pass "archive naming contract"; else fail "archive naming contract"; fi
if grep -q 'cargo install eggress-cli --locked' "$INSTALLER"; then pass "unsupported target points to Cargo"; else fail "unsupported target points to Cargo"; fi
# The installer may mention sudo in comments to say it never uses it, but it
# must never invoke sudo as a command (anchored to non-comment code lines).
if grep -Ev '^\s*#' "$INSTALLER" | grep -Eq '(^|[;&|])\s*sudo(\s|$)'; then fail "installer must never invoke sudo"; else pass "no sudo invocation in installer"; fi
if grep -q 'not an independent signature' "$INSTALLER"; then pass "integrity language (no signature overclaim)"; else fail "integrity language (no signature overclaim)"; fi

# --- happy paths ------------------------------------------------------------

FIX="$(mktemp -d)"
make_fixture "$FIX" "9.9.9"
DEST="$(mktemp -d)"
if EGRESS_RELEASE_BASE_URL="file://$FIX" bash "$INSTALLER" --version 9.9.9 --dir "$DEST" >/tmp/eggress-install-test.log 2>&1; then
  if [[ -x "$DEST/eggress" && -x "$DEST/pproxy" ]] && [[ "$("$DEST/eggress" version)" == "eggress 9.9.9" ]]; then
    pass "checksum success + pinned install + custom --dir"
  else
    fail "checksum success + pinned install + custom --dir" "installed binaries missing or mis-versioned"
  fi
else
  fail "checksum success + pinned install + custom --dir" "$(cat /tmp/eggress-install-test.log)"
fi

DEST2="$(mktemp -d)"
if EGRESS_RELEASE_BASE_URL="file://$FIX" bash "$INSTALLER" --dir "$DEST2" >/tmp/eggress-install-latest.log 2>&1; then
  if [[ -x "$DEST2/eggress" && -x "$DEST2/pproxy" ]]; then pass "latest happy path (no --version)"; else fail "latest happy path" "binaries missing"; fi
else
  fail "latest happy path (no --version)" "$(cat /tmp/eggress-install-latest.log)"
fi

# --- version-pinned URL selection --------------------------------------------
# Fixture with ONLY a pinned release: latest must fail, pinned must succeed.
FIX_PINNED="$(mktemp -d)"
make_fixture "$FIX_PINNED" "7.7.7"
rm -rf "$FIX_PINNED/latest"
DEST3="$(mktemp -d)"
if EGRESS_RELEASE_BASE_URL="file://$FIX_PINNED" bash "$INSTALLER" --version 7.7.7 --dir "$DEST3" >/dev/null 2>&1; then
  pass "version-pinned URL selection (pinned resolves)"
else
  fail "version-pinned URL selection (pinned resolves)"
fi
if EGRESS_RELEASE_BASE_URL="file://$FIX_PINNED" bash "$INSTALLER" --dir "$DEST3" >/dev/null 2>&1; then
  fail "version-pinned URL selection (latest must not fall back to pinned)"
else
  pass "version-pinned URL selection (latest must not fall back to pinned)"
fi
# Requesting a different version than the fixture must fail, never fall forward.
if EGRESS_RELEASE_BASE_URL="file://$FIX_PINNED" bash "$INSTALLER" --version 1.2.3 --dir "$DEST3" >/dev/null 2>&1; then
  fail "requested-version mismatch must fail (no fall-forward)"
else
  pass "requested-version mismatch must fail (no fall-forward)"
fi

# --- negative paths -----------------------------------------------------------

# Checksum mismatch.
FIX_BAD="$(mktemp -d)"
make_fixture "$FIX_BAD" "9.9.9"
echo "0000000000000000000000000000000000000000000000000000000000000000  $ARCHIVE" > "$FIX_BAD/download/v9.9.9/$ARCHIVE.sha256"
echo "0000000000000000000000000000000000000000000000000000000000000000  $ARCHIVE" > "$FIX_BAD/latest/download/$ARCHIVE.sha256"
if EGRESS_RELEASE_BASE_URL="file://$FIX_BAD" bash "$INSTALLER" --version 9.9.9 --dir "$(mktemp -d)" >/dev/null 2>&1; then
  fail "checksum mismatch must fail"
else
  pass "checksum mismatch must fail"
fi

# Candidate eggress version mismatch.
FIX_EG="$(mktemp -d)"
make_fixture "$FIX_EG" "9.9.9" "0.0.0" "9.9.9"
if EGRESS_RELEASE_BASE_URL="file://$FIX_EG" bash "$INSTALLER" --version 9.9.9 --dir "$(mktemp -d)" >/dev/null 2>&1; then
  fail "candidate eggress version mismatch must fail"
else
  pass "candidate eggress version mismatch must fail"
fi

# Candidate pproxy version mismatch (versions disagree with each other).
FIX_PP="$(mktemp -d)"
make_fixture "$FIX_PP" "9.9.9" "9.9.9" "8.8.8"
if EGRESS_RELEASE_BASE_URL="file://$FIX_PP" bash "$INSTALLER" --version 9.9.9 --dir "$(mktemp -d)" >/dev/null 2>&1; then
  fail "candidate pproxy version mismatch must fail"
else
  pass "candidate pproxy version mismatch must fail"
fi

# Archive missing pproxy.
FIX_ONE="$(mktemp -d)"
make_fixture "$FIX_ONE" "9.9.9" "9.9.9" "9.9.9" "no"
if EGRESS_RELEASE_BASE_URL="file://$FIX_ONE" bash "$INSTALLER" --version 9.9.9 --dir "$(mktemp -d)" >/dev/null 2>&1; then
  fail "archive with only one binary must fail"
else
  pass "archive with only one binary must fail"
fi

# Unsupported target (shadow uname to report an unknown platform).
SHIM="$(mktemp -d)"
cat > "$SHIM/uname" <<'EOF'
#!/usr/bin/env bash
if [[ "$1" == "-s" ]]; then echo "FreeBSD"; else echo "riscv64"; fi
EOF
chmod +x "$SHIM/uname"
if PATH="$SHIM:$PATH" bash "$INSTALLER" --dir "$(mktemp -d)" >/tmp/eggress-unsupported.log 2>&1; then
  fail "unsupported target must fail"
else
  if grep -q "cargo install eggress-cli" /tmp/eggress-unsupported.log; then
    pass "unsupported target fails with Cargo alternative"
  else
    fail "unsupported target fails with Cargo alternative" "$(cat /tmp/eggress-unsupported.log)"
  fi
fi

# Non-writable destination (/proc cannot be created; never touches real installs).
if bash "$INSTALLER" --dir "/proc/eggress-installer-test-no-write" >/tmp/eggress-nowrite.log 2>&1; then
  fail "non-writable destination must fail"
else
  pass "non-writable destination must fail"
fi

# Unknown argument.
if bash "$INSTALLER" --bogus-flag >/dev/null 2>&1; then
  fail "unknown argument must fail"
else
  pass "unknown argument must fail"
fi

rm -rf "$FIX" "$FIX_PINNED" "$FIX_BAD" "$FIX_EG" "$FIX_PP" "$FIX_ONE" "$SHIM" "$DEST" "$DEST2" "$DEST3"

echo "---"
echo "pass=$PASS fail=$FAIL"
if [[ "$FAIL" -ne 0 ]]; then exit 1; fi
