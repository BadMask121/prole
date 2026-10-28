#!/usr/bin/env bash
#
# Build Prole, sign it with a stable identity, install it to /Applications and
# relaunch it.
#
# Why a stable identity: macOS keys the Screen Recording grant to the app's
# designated requirement. An ad-hoc build's requirement is its cdhash, which
# changes on every build — so after each update System Settings still shows
# Prole switched on, but the new binary is refused. Signing with an Apple
# Development certificate makes the requirement "this bundle id + this
# certificate", which survives rebuilds: grant once, keep it.
#
# The identity is picked from your keychain (first "Apple Development" one) or
# PROLE_SIGNING_IDENTITY. Nothing personal goes into tauri.conf.json, which
# stays ad-hoc for the public DMG (scripts/release.sh).
#
# Usage:
#   bash scripts/install-local.sh
#   bash scripts/install-local.sh --reset-permission   # also clear the old grant

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_SRC="$ROOT/src-tauri/target/release/bundle/macos/Prole.app"
APP_DST="/Applications/Prole.app"
BUNDLE_ID="com.prole.app"

IDENTITY="${PROLE_SIGNING_IDENTITY:-$(security find-identity -v -p codesigning \
  | awk -F'"' '/Apple Development/ { print $2; exit }')}"

echo "-> Building"
(cd "$ROOT" && pnpm tauri build --bundles app)

# Sign a copy staged outside the source tree: under ~/Desktop the bundle keeps
# picking up com.apple.FinderInfo, which fails strict verification.
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
ditto --noextattr --norsrc "$APP_SRC" "$STAGE/Prole.app"
APP_SRC="$STAGE/Prole.app"

if [ -n "$IDENTITY" ]; then
  echo "-> Signing with: $IDENTITY"
  codesign --force --deep --options runtime --sign "$IDENTITY" "$APP_SRC"
else
  echo "[!] No Apple Development identity found; keeping the ad-hoc signature."
  echo "    Screen Recording will need re-granting after every install."
fi
codesign --verify --strict "$APP_SRC"
codesign -dr - "$APP_SRC" 2>&1 | grep designated

if pgrep -f "$APP_DST/Contents/MacOS/prole" >/dev/null; then
  echo "-> Quitting the running Prole"
  pkill -TERM -f "$APP_DST/Contents/MacOS/prole" || true
  for _ in $(seq 1 20); do
    pgrep -f "$APP_DST/Contents/MacOS/prole" >/dev/null || break
    sleep 0.25
  done
fi

echo "-> Installing to $APP_DST"
rm -rf "$APP_DST"
ditto "$APP_SRC" "$APP_DST"

if [ "${1:-}" = "--reset-permission" ]; then
  echo "-> Clearing the old Screen Recording entry"
  tccutil reset ScreenCapture "$BUNDLE_ID"
fi

open -a "$APP_DST"
echo "[ok] Prole installed and launched."
