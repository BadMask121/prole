#!/usr/bin/env bash
#
# Build the Prole DMG locally and publish it on prole.jeffrey.build, which is
# the Cloudflare Worker in site/ (see docs/superpowers/specs/
# 2026-10-28-prole-on-cloudflare-design.md):
#
#   /download/Prole-<version>.dmg   the release, from the prole-downloads R2 bucket
#   /download/Prole.dmg, /latest    redirect to the newest release
#   /download/latest.json           the current release (site/release.json)
#
# A release uploads the DMG to R2, writes site/release.json, deploys the Worker
# (so the page and the download switch together) and commits release.json.
#
# Bump "version" in src-tauri/tauri.conf.json (and package.json, Cargo.toml)
# before releasing; a version already in site/release.json is refused.
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
#   bash scripts/release.sh               # build, verify, publish, check the live site
#   bash scripts/release.sh --no-upload   # build + verify only; prints the DMG path
#   bash scripts/release.sh --dry-run     # print what would happen, do nothing
#   ... --allow-dirty                     # release uncommitted changes (not recommended)
#
# Env overrides:
#   PROLE_SITE_URL   public site       (default: https://prole.jeffrey.build)
#   PROLE_BUCKET     R2 bucket         (default: prole-downloads)
#   PROLE_TARGET     rust target triple or "host"
#                                      (default: universal-apple-darwin)
#   PROLE_BUILD_DIR  cargo target dir  (default: ~/Library/Caches/prole-release/target)

set -euo pipefail

PROLE_SITE_URL="${PROLE_SITE_URL:-https://prole.jeffrey.build}"
PROLE_BUCKET="${PROLE_BUCKET:-prole-downloads}"
PROLE_TARGET="${PROLE_TARGET:-universal-apple-darwin}"
PROLE_BUILD_DIR="${PROLE_BUILD_DIR:-$HOME/Library/Caches/prole-release/target}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SITE="$ROOT/site"
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
wrangler() { (cd "$SITE" && npx --no-install wrangler "$@"); }

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
FILE="Prole-${VERSION}.dmg"
RELEASE_URL="$PROLE_SITE_URL/download/$FILE"

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
  echo "  r2:        $PROLE_BUCKET/$FILE"
  echo "  -> url:    $RELEASE_URL"
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
  # A version recorded in release.json is published: people may already have
  # that exact file, so it is never replaced.
  PUBLISHED="$(node -p "require('./site/release.json').version")"
  [ "$PUBLISHED" != "$VERSION" ] || fail "$VERSION is already released. Bump the version first."
  [ -d "$SITE/node_modules" ] || (cd "$SITE" && npm ci --no-audit --no-fund >/dev/null)
  wrangler whoami 2>/dev/null | grep -q "You are logged in" \
    || fail "wrangler isn't logged in. Run: cd site && npx wrangler login"
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
BYTES="$(stat -f %z "$DMG")"
echo "[ok] $DMG"
echo "     sha256 $SHA"

if [ "$UPLOAD" -eq 0 ]; then
  echo "(--no-upload: not uploaded)"
  exit 0
fi

# ── Publish ───────────────────────────────────────────────────────────
# The file goes up first; nothing links to it until the Worker is deployed
# with the new release.json, so the page and the download switch together.
echo "-> Uploading $FILE to R2 ($PROLE_BUCKET)"
wrangler r2 object put "$PROLE_BUCKET/$FILE" --file "$DMG" \
  --content-type application/x-apple-diskimage --remote >/dev/null

echo "-> Writing site/release.json and deploying the site"
VERSION="$VERSION" FILE="$FILE" SHA="$SHA" BYTES="$BYTES" COMMIT="$COMMIT" node -e '
  const { VERSION, FILE, SHA, BYTES, COMMIT } = process.env;
  const release = { version: VERSION, file: FILE, sha256: SHA, bytes: Number(BYTES),
    commit: COMMIT, date: new Date().toISOString().slice(0, 10) };
  require("fs").writeFileSync("site/release.json", JSON.stringify(release, null, 2) + "\n");'
# The route tests also validate release.json; a bad one stops here, before deploy.
(cd "$SITE" && npm test --silent >/dev/null) || fail "site tests failed with the new release.json"
wrangler deploy >/dev/null

git add site/release.json
git commit -q -m "chore(release): Prole $VERSION" -- site/release.json \
  || echo "[!] Couldn't commit site/release.json; commit it yourself."

# ── Confirm the live site serves it ───────────────────────────────────
echo "-> Checking $PROLE_SITE_URL"
nocache=(-H 'Cache-Control: no-cache')
GOT="$(curl -fsSL "${nocache[@]}" "$RELEASE_URL" | shasum -a 256 | cut -d' ' -f1)"
[ "$GOT" = "$SHA" ] || fail "$RELEASE_URL serves $GOT, expected $SHA"
echo "   ok  $RELEASE_URL"
LOC="$(curl -fsS -o /dev/null -w '%{redirect_url}' "${nocache[@]}" "$PROLE_SITE_URL/download/Prole.dmg")"
[ "$LOC" = "$RELEASE_URL" ] || fail "/download/Prole.dmg redirects to “$LOC”, expected $RELEASE_URL"
echo "   ok  /download/Prole.dmg -> $FILE"
LIVE="$(curl -fsSL "${nocache[@]}" "$PROLE_SITE_URL/download/latest.json" | node -pe "JSON.parse(require('fs').readFileSync(0,'utf8')).version")"
[ "$LIVE" = "$VERSION" ] || fail "latest.json says $LIVE, expected $VERSION"
echo "   ok  /download/latest.json ($LIVE)"
curl -fsSL "${nocache[@]}" "$PROLE_SITE_URL/" | grep -q "href=\"/download/$FILE\"" \
  || fail "The page doesn't link $FILE"
echo "   ok  the page links $FILE"

echo "[ok] Released Prole $VERSION ($COMMIT) — $RELEASE_URL"
echo "     sha256 $SHA"
echo "     site/release.json committed; push when ready."
