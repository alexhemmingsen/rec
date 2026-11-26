#!/usr/bin/env bash
# Builds a signed macOS release of Rec into ./release:
#   Rec_<version>_aarch64.dmg        the download
#   Rec.app.tar.gz (+ .sig)          what the in-app updater installs
# scripts/latest-json.mjs turns the signed artifacts of all platforms into
# the updater's latest.json.
#
# Signing (all optional; without them the app is ad-hoc signed):
#   REC_SIGN_P12, REC_SIGN_P12_PASSWORD   code signing certificate (.p12).
#       Rec uses a self-signed certificate until it moves to an Apple
#       Developer ID. Signing every build with the same certificate keeps
#       macOS's Screen Recording permission across updates.
#   RCODESIGN                             path to rcodesign (default: rcodesign)
#   REC_NOTARY_API_KEY                    App Store Connect API key JSON (from
#       `rcodesign encode-app-store-connect-api-key`). When set, REC_SIGN_P12
#       must be the Apple Developer ID certificate: the app is signed with the
#       hardened runtime, notarized and stapled, and Gatekeeper opens it
#       without the "could not verify" warning.
#   TAURI_SIGNING_PRIVATE_KEY_PATH, TAURI_SIGNING_PRIVATE_KEY_PASSWORD
#       updater signing key (minisign); its public key is in tauri.conf.json.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(node -p "require('./src-tauri/tauri.conf.json').version")
RCODESIGN=${RCODESIGN:-rcodesign}
OUT=release

echo "==> Building Rec $VERSION"
pnpm tauri build --bundles app

rm -rf "$OUT"
mkdir -p "$OUT/stage" "$OUT/dmg"
cp -R target/release/bundle/macos/Rec.app "$OUT/stage/"
APP="$OUT/stage/Rec.app"

NOTARIZE=${REC_NOTARY_API_KEY:+yes}
if [[ -n "${REC_SIGN_P12:-}" ]]; then
  echo "==> Signing with $REC_SIGN_P12${NOTARIZE:+ (hardened runtime, for notarization)}"
  "$RCODESIGN" sign --p12-file "$REC_SIGN_P12" --p12-password "${REC_SIGN_P12_PASSWORD:-}" \
    ${NOTARIZE:+--code-signature-flags runtime} "$APP"
  if [[ -n "$NOTARIZE" ]]; then
    echo "==> Notarizing the app"
    "$RCODESIGN" notary-submit --api-key-file "$REC_NOTARY_API_KEY" --staple "$APP"
  fi
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
if [[ -n "$NOTARIZE" ]]; then
  # Stapling the ticket to a DMG needs the DMG itself signed. Only with the
  # Developer ID: the self-signed certificate isn't trusted, so signing the
  # DMG with it gains nothing.
  echo "==> Signing and notarizing the DMG"
  "$RCODESIGN" sign --p12-file "$REC_SIGN_P12" --p12-password "${REC_SIGN_P12_PASSWORD:-}" "$OUT/Rec_${VERSION}_aarch64.dmg"
  "$RCODESIGN" notary-submit --api-key-file "$REC_NOTARY_API_KEY" --staple "$OUT/Rec_${VERSION}_aarch64.dmg"
fi

echo "==> Creating the updater archive"
COPYFILE_DISABLE=1 tar -czf "$OUT/Rec.app.tar.gz" -C "$OUT/stage" Rec.app
if [[ -n "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" || -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  if [[ -n "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]]; then
    pnpm --silent tauri signer sign -f "$TAURI_SIGNING_PRIVATE_KEY_PATH" -p "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" "$OUT/Rec.app.tar.gz" >/dev/null
  else
    pnpm --silent tauri signer sign "$OUT/Rec.app.tar.gz" >/dev/null
  fi
else
  echo "==> No updater key given: the updater archive is unsigned"
fi

rm -rf "$OUT/stage" "$OUT/dmg"
echo "==> Done:"
ls -la "$OUT"
