#!/usr/bin/env bash
#
# Build the Prole DMG locally and publish it on the landing-page host, where
# nginx serves /data/prole-releases at /download/ (see apps/prole-site/):
#
#   /download/Prole-<version>.dmg   the release itself; never overwritten
#   /download/Prole.dmg             symlink to the newest release (old links)
#   /download/latest.json           {version, file, url, sha256, size, commit,
#                                   released}; the site reads it to point its
#                                   download buttons at the versioned URL
#
# Bump "version" in src-tauri/tauri.conf.json (and package.json, Cargo.toml)
# before releasing; publishing a version that already exists is refused.
#
# Why local (not CI): macOS bundles need a Mac, and building locally avoids the
# com.apple.quarantine xattr that a downloaded-then-rebuilt artifact would carry.
# The app is ad-hoc signed (tauri.conf.json -> bundle.macOS.signingIdentity "-"),
# so users still do a one-time right-click -> Open; that's documented on the site.
#
# Why the build happens outside the repo: when the repo lives under an
# iCloud-managed folder (~/Desktop, ~/Documents), the File Provider tags a
# freshly built Prole.app with com.apple.FinderInfo a moment after it appears.
# codesign rejects that ("resource fork, Finder information, or similar
# detritus not allowed") — when the tag lands before Tauri signs, the build
# fails; when it lands after, the shipped app fails strict verification. So
# cargo builds into PROLE_BUILD_DIR (a cache dir iCloud doesn't manage), and
# the .app is still copied without extended attributes, re-signed and
# verified before the DMG is made from that copy with hdiutil.
#
# Usage:
#   bash scripts/release.sh               # build, verify, upload, check the live URL
#   bash scripts/release.sh --no-upload   # build + verify only; prints the DMG path
#   bash scripts/release.sh --dry-run     # print what would happen, do nothing
#   ... --allow-dirty                     # release uncommitted changes (not recommended)
#   ... --replace                         # re-publish an existing version (fixing a bad upload)
#
# Override the destination with env vars:
#   PROLE_HOST       ssh host          (default: root@95.217.185.93)
#   PROLE_REMOTE_DIR remote dir        (default: /data/prole-releases)
#   PROLE_ARCHIVE_DIR replaced DMGs    (default: /data/prole-releases-archive, not served)
#   PROLE_BASE_URL   public base URL   (default: https://prole.jeffrey.build/download)
#   PROLE_TARGET     rust target triple or "host"
#                                      (default: universal-apple-darwin)
#   PROLE_BUILD_DIR  cargo target dir  (default: ~/Library/Caches/prole-release/target)

set -euo pipefail

PROLE_HOST="${PROLE_HOST:-root@95.217.185.93}"
PROLE_REMOTE_DIR="${PROLE_REMOTE_DIR:-/data/prole-releases}"
PROLE_ARCHIVE_DIR="${PROLE_ARCHIVE_DIR:-/data/prole-releases-archive}"
PROLE_BASE_URL="${PROLE_BASE_URL:-https://prole.jeffrey.build/download}"
PROLE_TARGET="${PROLE_TARGET:-universal-apple-darwin}"
PROLE_BUILD_DIR="${PROLE_BUILD_DIR:-$HOME/Library/Caches/prole-release/target}"
LATEST_NAME="Prole.dmg"   # stable alias: always the newest release

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DRY_RUN=0
UPLOAD=1
ALLOW_DIRTY=0
REPLACE=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --no-upload) UPLOAD=0 ;;
    --allow-dirty) ALLOW_DIRTY=1 ;;
    --replace) REPLACE=1 ;;
    *) echo "[x] Unknown option: $arg" >&2; exit 2 ;;
  esac
done

fail() { echo "[x] $*" >&2; exit 1; }

# Where Tauri drops the .app depends on whether we cross-compile to a triple.
if [ "$PROLE_TARGET" = "host" ]; then
  BUILD_ARGS=(--bundles app)
  APP_SRC="$PROLE_BUILD_DIR/release/bundle/macos/Prole.app"
else
  BUILD_ARGS=(--target "$PROLE_TARGET" --bundles app)
  APP_SRC="$PROLE_BUILD_DIR/$PROLE_TARGET/release/bundle/macos/Prole.app"
fi
VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
COMMIT="$(git rev-parse --short HEAD)"
OUT_DIR="$PROLE_BUILD_DIR/release-dmg"
DMG_NAME="Prole_${VERSION}_${PROLE_TARGET}.dmg"
REMOTE_NAME="Prole-${VERSION}.dmg"
RELEASE_URL="$PROLE_BASE_URL/$REMOTE_NAME"

[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "Version “$VERSION” in tauri.conf.json isn't x.y.z."
for f in package.json src-tauri/Cargo.toml; do
  grep -qE "\"version\": \"$VERSION\"|^version = \"$VERSION\"" "$f" \
    || fail "$f doesn't say version $VERSION — keep it in step with tauri.conf.json."
done

echo "Prole release"
echo "  version:   $VERSION (commit $COMMIT)"
echo "  target:    $PROLE_TARGET"
echo "  dmg:       $OUT_DIR/$DMG_NAME"
if [ "$UPLOAD" -eq 1 ]; then
  echo "  dest:      $PROLE_HOST:$PROLE_REMOTE_DIR/$REMOTE_NAME"
  echo "  -> url:    $RELEASE_URL"
  echo "  latest:    $PROLE_BASE_URL/$LATEST_NAME, $PROLE_BASE_URL/latest.json"
fi

if [ "$DRY_RUN" -eq 1 ]; then
  echo "(dry run - nothing built or uploaded)"
  exit 0
fi

# ── Preflight: catch problems before a multi-minute build ─────────────
if [ "$ALLOW_DIRTY" -eq 0 ] && [ -n "$(git status --porcelain)" ]; then
  fail "Uncommitted changes — a release should match a commit. Commit first, or pass --allow-dirty."
fi
if [ "$UPLOAD" -eq 1 ]; then
  ssh -o BatchMode=yes -o ConnectTimeout=10 "$PROLE_HOST" true 2>/dev/null \
    || fail "Can't SSH to $PROLE_HOST without a prompt. Load your key first: ssh-add --apple-use-keychain ~/.ssh/id_rsa"
  # A published version is immutable: people may already have that exact file.
  if [ "$REPLACE" -eq 0 ] && ssh "$PROLE_HOST" "test -e '$PROLE_REMOTE_DIR/$REMOTE_NAME'"; then
    fail "$REMOTE_NAME is already published. Bump the version, or pass --replace to overwrite it."
  fi
fi

# ── Build ─────────────────────────────────────────────────────────────
# A universal build needs both arch targets; adding is idempotent and cheap.
if [ "$PROLE_TARGET" = "universal-apple-darwin" ]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
fi

echo "-> Building in $PROLE_BUILD_DIR (universal builds both arches; first run is a full compile)"
mkdir -p "$PROLE_BUILD_DIR"
rm -rf "$APP_SRC" # never package a stale bundle from an earlier build
CARGO_TARGET_DIR="$PROLE_BUILD_DIR" pnpm tauri build "${BUILD_ARGS[@]}"
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
hdiutil create -volname "Prole" -srcfolder "$STAGE/root" -fs HFS+ -format UDZO \
  -imagekey zlib-level=9 -ov "$DMG" >/dev/null

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
R="$PROLE_REMOTE_DIR"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
echo "-> Uploading $REMOTE_NAME to ${PROLE_HOST}"
# With --replace, keep the copy being replaced (outside the served dir). A
# Prole.dmg that is still a plain file predates versioned releases; keep it too
# before it becomes a symlink.
ssh "$PROLE_HOST" "mkdir -p '$R' '$PROLE_ARCHIVE_DIR' && cd '$R' && \
  if [ -f '$REMOTE_NAME' ]; then cp -p '$REMOTE_NAME' '$PROLE_ARCHIVE_DIR/Prole-$VERSION-replaced-$STAMP.dmg'; fi && \
  if [ -f '$LATEST_NAME' ] && [ ! -L '$LATEST_NAME' ]; then cp -p '$LATEST_NAME' '$PROLE_ARCHIVE_DIR/Prole-unversioned-$STAMP.dmg'; fi"
# Upload to a temp name then move, so a download mid-upload never gets a partial file.
scp -q "$DMG" "$PROLE_HOST:$R/.$REMOTE_NAME.tmp"
REMOTE_SHA="$(ssh "$PROLE_HOST" "cd '$R' && mv -f '.$REMOTE_NAME.tmp' '$REMOTE_NAME' && sha256sum '$REMOTE_NAME' | cut -d' ' -f1")"
[ "$REMOTE_SHA" = "$SHA" ] || fail "Uploaded file checksum $REMOTE_SHA != local $SHA"

# Manifest the site reads to link the versioned file.
SIZE="$(stat -f %z "$DMG")"
cat > "$STAGE/latest.json" <<JSON
{
  "version": "$VERSION",
  "file": "$REMOTE_NAME",
  "url": "$RELEASE_URL",
  "sha256": "$SHA",
  "size": $SIZE,
  "commit": "$COMMIT",
  "released": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
JSON
node -e "JSON.parse(require('fs').readFileSync('$STAGE/latest.json','utf8'))" || fail "latest.json isn't valid JSON"
scp -q "$STAGE/latest.json" "$PROLE_HOST:$R/.latest.json.tmp"

# Point the alias and the manifest at the new release. Each is a rename, so a
# visitor always gets a complete file; the alias is a relative symlink, which
# nginx follows inside the mounted dir.
echo "-> Making $REMOTE_NAME the latest"
ssh "$PROLE_HOST" "cd '$R' && ln -sfn '$REMOTE_NAME' '.$LATEST_NAME.lnk' && mv -fT '.$LATEST_NAME.lnk' '$LATEST_NAME' \
  && mv -f .latest.json.tmp latest.json"

# ── Confirm the public URLs serve it ──────────────────────────────────
check() { # url expected-sha
  local got
  got="$(curl -fsSL -H 'Cache-Control: no-cache' "$1" | shasum -a 256 | cut -d' ' -f1)"
  [ "$got" = "$2" ] || fail "$1 serves $got, expected $2 (cache? wrong mount?)"
  echo "   ok  $1"
}
echo "-> Checking the public URLs"
check "$RELEASE_URL" "$SHA"
check "$PROLE_BASE_URL/$LATEST_NAME" "$SHA"
LIVE_VERSION="$(curl -fsSL -H 'Cache-Control: no-cache' "$PROLE_BASE_URL/latest.json" | node -pe "JSON.parse(require('fs').readFileSync(0,'utf8')).version")"
[ "$LIVE_VERSION" = "$VERSION" ] || fail "latest.json says $LIVE_VERSION, expected $VERSION"
echo "   ok  $PROLE_BASE_URL/latest.json ($LIVE_VERSION)"

echo "[ok] Released Prole $VERSION ($COMMIT) — $RELEASE_URL"
echo "     sha256 $SHA"
