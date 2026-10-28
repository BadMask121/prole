#!/usr/bin/env bash
#
# Build the Prole DMG locally and upload it to the landing-page host, where
# nginx serves it at /download/Prole.dmg (see apps/prole-site/).
#
# Why local (not CI): macOS bundles need a Mac, and building locally avoids the
# com.apple.quarantine xattr that a downloaded-then-rebuilt artifact would carry.
# The app is ad-hoc signed (tauri.conf.json -> bundle.macOS.signingIdentity "-"),
# so users still do a one-time right-click -> Open; that's documented on the site.
#
# Why the app is staged before packaging: when the repo lives under an
# iCloud-managed folder (~/Desktop, ~/Documents), the File Provider tags the
# built Prole.app with com.apple.FinderInfo. Strict signature verification
# rejects that ("resource fork, Finder information, or similar detritus not
# allowed"), and clearing it in place doesn't stick. So the .app is copied
# without extended attributes to a temp dir, re-signed and verified there, and
# the DMG is made from that copy with hdiutil.
#
# Usage:
#   bash scripts/release.sh               # build, verify, upload, check the live URL
#   bash scripts/release.sh --no-upload   # build + verify only; prints the DMG path
#   bash scripts/release.sh --dry-run     # print what would happen, do nothing
#   ... --allow-dirty                     # release uncommitted changes (not recommended)
#
# Override the destination with env vars:
#   PROLE_HOST       ssh host          (default: root@95.217.185.93)
#   PROLE_REMOTE_DIR remote dir        (default: /data/prole-releases)
#   PROLE_ARCHIVE_DIR previous DMGs    (default: /data/prole-releases-archive, not served)
#   PROLE_URL        public URL        (default: https://prole.jeffrey.build/download/Prole.dmg)
#   PROLE_TARGET     rust target triple or "host"
#                                      (default: universal-apple-darwin)

set -euo pipefail

PROLE_HOST="${PROLE_HOST:-root@95.217.185.93}"
PROLE_REMOTE_DIR="${PROLE_REMOTE_DIR:-/data/prole-releases}"
PROLE_ARCHIVE_DIR="${PROLE_ARCHIVE_DIR:-/data/prole-releases-archive}"
PROLE_URL="${PROLE_URL:-https://prole.jeffrey.build/download/Prole.dmg}"
PROLE_TARGET="${PROLE_TARGET:-universal-apple-darwin}"
REMOTE_NAME="Prole.dmg"   # stable filename -> stable /download/Prole.dmg URL

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DRY_RUN=0
UPLOAD=1
ALLOW_DIRTY=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --no-upload) UPLOAD=0 ;;
    --allow-dirty) ALLOW_DIRTY=1 ;;
    *) echo "[x] Unknown option: $arg" >&2; exit 2 ;;
  esac
done

fail() { echo "[x] $*" >&2; exit 1; }

# Where Tauri drops the .app depends on whether we cross-compile to a triple.
if [ "$PROLE_TARGET" = "host" ]; then
  BUILD_ARGS=(--bundles app)
  APP_SRC="$ROOT/src-tauri/target/release/bundle/macos/Prole.app"
else
  BUILD_ARGS=(--target "$PROLE_TARGET" --bundles app)
  APP_SRC="$ROOT/src-tauri/target/$PROLE_TARGET/release/bundle/macos/Prole.app"
fi
VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
COMMIT="$(git rev-parse --short HEAD)"
OUT_DIR="$ROOT/src-tauri/target/release-dmg"
DMG_NAME="Prole_${VERSION}_${PROLE_TARGET}.dmg"

echo "Prole release"
echo "  version:   $VERSION (commit $COMMIT)"
echo "  target:    $PROLE_TARGET"
echo "  dmg:       $OUT_DIR/$DMG_NAME"
if [ "$UPLOAD" -eq 1 ]; then
  echo "  dest:      $PROLE_HOST:$PROLE_REMOTE_DIR/$REMOTE_NAME"
  echo "  archive:   $PROLE_HOST:$PROLE_ARCHIVE_DIR/"
  echo "  -> served: $PROLE_URL"
fi

if [ "$DRY_RUN" -eq 1 ]; then
  echo "(dry run - nothing built or uploaded)"
  exit 0
fi

# ── Preflight: catch problems before a multi-minute build ─────────────
if [ "$ALLOW_DIRTY" -eq 0 ] && [ -n "$(git status --porcelain)" ]; then
  fail "Uncommitted changes — a release should match a commit. Commit first, or pass --allow-dirty."
fi
if [ "$UPLOAD" -eq 1 ] && ! ssh -o BatchMode=yes -o ConnectTimeout=10 "$PROLE_HOST" true 2>/dev/null; then
  fail "Can't SSH to $PROLE_HOST without a prompt. Load your key first: ssh-add --apple-use-keychain ~/.ssh/id_rsa"
fi

# ── Build ─────────────────────────────────────────────────────────────
# A universal build needs both arch targets; adding is idempotent and cheap.
if [ "$PROLE_TARGET" = "universal-apple-darwin" ]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
fi

echo "-> Building (this compiles release; universal builds both arches)"
pnpm tauri build "${BUILD_ARGS[@]}"
[ -d "$APP_SRC" ] || fail "No Prole.app at $APP_SRC after build."

# ── Stage, re-sign, verify ────────────────────────────────────────────
STAGE="$(mktemp -d)"
trap 'hdiutil detach "$STAGE/mnt" >/dev/null 2>&1 || true; rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/root"
ditto --noextattr --norsrc "$APP_SRC" "$STAGE/root/Prole.app"
ln -s /Applications "$STAGE/root/Applications"

echo "-> Signing the staged copy (ad-hoc)"
codesign --force --deep --options runtime --sign - "$STAGE/root/Prole.app"
codesign --verify --strict --deep "$STAGE/root/Prole.app" \
  || fail "Staged Prole.app fails strict signature verification."

# ── Package ───────────────────────────────────────────────────────────
echo "-> Creating $DMG_NAME"
mkdir -p "$OUT_DIR"
DMG="$OUT_DIR/$DMG_NAME"
hdiutil create -volname "Prole" -srcfolder "$STAGE/root" -fs HFS+ -format UDZO -ov "$DMG" >/dev/null

# ── Verify what users will actually get ───────────────────────────────
echo "-> Verifying the DMG"
hdiutil verify "$DMG" >/dev/null 2>&1 || fail "hdiutil verify failed for $DMG"
mkdir -p "$STAGE/mnt"
hdiutil attach -nobrowse -readonly -mountpoint "$STAGE/mnt" "$DMG" >/dev/null
MOUNTED_APP="$STAGE/mnt/Prole.app"
codesign --verify --strict --deep "$MOUNTED_APP" \
  || fail "Prole.app inside the DMG fails strict signature verification."
[ -L "$STAGE/mnt/Applications" ] || fail "DMG is missing the Applications drop link."
if [ "$PROLE_TARGET" = "universal-apple-darwin" ]; then
  ARCHS="$(lipo -archs "$MOUNTED_APP/Contents/MacOS/prole")"
  case "$ARCHS" in
    *x86_64*arm64* | *arm64*x86_64*) ;;
    *) fail "Expected a universal binary, got: $ARCHS" ;;
  esac
fi
hdiutil detach "$STAGE/mnt" >/dev/null
SHA="$(shasum -a 256 "$DMG" | cut -d' ' -f1)"
echo "[ok] $DMG"
echo "     sha256 $SHA"

if [ "$UPLOAD" -eq 0 ]; then
  echo "(--no-upload: not uploaded)"
  exit 0
fi

# ── Upload ────────────────────────────────────────────────────────────
echo "-> Uploading to ${PROLE_HOST}"
# Keep the outgoing DMG (outside the served dir) so a release can be rolled back.
ssh "$PROLE_HOST" "mkdir -p '$PROLE_REMOTE_DIR' '$PROLE_ARCHIVE_DIR' && \
  if [ -f '$PROLE_REMOTE_DIR/$REMOTE_NAME' ]; then \
    cp -p '$PROLE_REMOTE_DIR/$REMOTE_NAME' \"$PROLE_ARCHIVE_DIR/Prole-\$(date -u +%Y%m%dT%H%M%SZ).dmg\"; \
  fi"
# Upload to a temp name then move, so a download mid-upload never gets a partial file.
scp -q "$DMG" "$PROLE_HOST:$PROLE_REMOTE_DIR/.$REMOTE_NAME.tmp"
REMOTE_SHA="$(ssh "$PROLE_HOST" "mv '$PROLE_REMOTE_DIR/.$REMOTE_NAME.tmp' '$PROLE_REMOTE_DIR/$REMOTE_NAME' && sha256sum '$PROLE_REMOTE_DIR/$REMOTE_NAME' | cut -d' ' -f1")"
[ "$REMOTE_SHA" = "$SHA" ] || fail "Uploaded file checksum $REMOTE_SHA != local $SHA"

# ── Confirm the public URL serves it ──────────────────────────────────
echo "-> Checking $PROLE_URL"
LIVE_SHA="$(curl -fsSL "$PROLE_URL" | shasum -a 256 | cut -d' ' -f1)"
[ "$LIVE_SHA" = "$SHA" ] || fail "Live download checksum $LIVE_SHA != released $SHA (cache? wrong mount?)"

echo "[ok] Released Prole $VERSION ($COMMIT) — $PROLE_URL"
echo "     sha256 $SHA"
