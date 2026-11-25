#!/usr/bin/env bash
# Builds a signed macOS release of Rec into ./release:
#   Rec_<version>_aarch64.dmg        the download
#   Rec.app.tar.gz (+ .sig)          what the in-app updater installs
#   latest.json                      what the in-app updater checks
#
# Signing (all optional; without them the app is ad-hoc signed):
#   REC_SIGN_P12, REC_SIGN_P12_PASSWORD   code signing certificate (.p12).
#       Rec uses a self-signed certificate until it moves to an Apple
#       Developer ID. Signing every build with the same certificate keeps
#       macOS's Screen Recording permission across updates.
#   RCODESIGN                             path to rcodesign (default: rcodesign)
#   TAURI_SIGNING_PRIVATE_KEY_PATH, TAURI_SIGNING_PRIVATE_KEY_PASSWORD
#       updater signing key (minisign); its public key is in tauri.conf.json.
#   REC_REPO                              owner/name for download URLs
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(node -p "require('./src-tauri/tauri.conf.json').version")
REPO=${REC_REPO:-alexhemmingsen/rec}
RCODESIGN=${RCODESIGN:-rcodesign}
OUT=release
TAG="v$VERSION"

echo "==> Building Rec $VERSION"
pnpm tauri build --bundles app

rm -rf "$OUT"
mkdir -p "$OUT/stage" "$OUT/dmg"
cp -R target/release/bundle/macos/Rec.app "$OUT/stage/"
APP="$OUT/stage/Rec.app"

if [[ -n "${REC_SIGN_P12:-}" ]]; then
  echo "==> Signing with $REC_SIGN_P12"
  "$RCODESIGN" sign --p12-file "$REC_SIGN_P12" --p12-password "${REC_SIGN_P12_PASSWORD:-}" "$APP"
else
  echo "==> No certificate given: ad-hoc signing"
  codesign --force --deep --sign - "$APP"
fi
codesign --verify --deep --strict "$APP"
codesign -dvv "$APP" 2>&1 | grep -E '^(Identifier|Authority|Signature)=' || true

echo "==> Creating the DMG"
cp -R "$APP" "$OUT/dmg/"
ln -s /Applications "$OUT/dmg/Applications"
hdiutil create -quiet -volname "Rec" -srcfolder "$OUT/dmg" -ov -format UDZO "$OUT/Rec_${VERSION}_aarch64.dmg"

echo "==> Creating the updater archive"
COPYFILE_DISABLE=1 tar -czf "$OUT/Rec.app.tar.gz" -C "$OUT/stage" Rec.app
if [[ -n "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" || -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  if [[ -n "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]]; then
    pnpm --silent tauri signer sign -f "$TAURI_SIGNING_PRIVATE_KEY_PATH" -p "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" "$OUT/Rec.app.tar.gz" >/dev/null
  else
    pnpm --silent tauri signer sign "$OUT/Rec.app.tar.gz" >/dev/null
  fi
  SIG=$(cat "$OUT/Rec.app.tar.gz.sig")
  NOTES=${REC_RELEASE_NOTES:-"See https://github.com/$REPO/releases/tag/$TAG"}
  VERSION="$VERSION" SIG="$SIG" REPO="$REPO" TAG="$TAG" NOTES="$NOTES" node -e '
    const e = process.env;
    const doc = {
      version: e.VERSION,
      notes: e.NOTES,
      pub_date: new Date().toISOString(),
      platforms: {
        "darwin-aarch64": {
          signature: e.SIG,
          url: `https://github.com/${e.REPO}/releases/download/${e.TAG}/Rec.app.tar.gz`,
        },
      },
    };
    require("fs").writeFileSync("release/latest.json", JSON.stringify(doc, null, 2) + "\n");
  '
else
  echo "==> No updater key given: skipping the signature and latest.json"
fi

rm -rf "$OUT/stage" "$OUT/dmg"
echo "==> Done:"
ls -la "$OUT"
