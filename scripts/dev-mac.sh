#!/usr/bin/env bash
# Build Rec.app (debug), sign it with Rec's certificate and relaunch it.
#
# Signing keeps macOS's Screen Recording permission across rebuilds: macOS
# ties the permission to the signature, and an unsigned build gets a new
# identity every time. Uses the Developer ID in ~/.tauri/rec-developer-id,
# the one releases are signed with, so dev builds and the installed app share
# the permission; falls back to the old self-signed ~/.tauri/rec-codesign (see
# AGENTS.md). rcodesign is downloaded once to ~/.tauri/bin.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

RC="$HOME/.tauri/bin/rcodesign"
if [[ ! -x "$RC" ]]; then
  echo "==> Installing rcodesign"
  VERSION=0.29.0 SHA=d1a532150adaf90048260d76359261aa716abafc45c53c5dc18845029184334a
  NAME="apple-codesign-$VERSION-aarch64-apple-darwin"
  TMP=$(mktemp -d)
  curl -fsSL -o "$TMP/$NAME.tar.gz" "https://github.com/indygreg/apple-platform-rs/releases/download/apple-codesign%2F$VERSION/$NAME.tar.gz"
  echo "$SHA  $TMP/$NAME.tar.gz" | shasum -a 256 -c - >/dev/null
  tar -xzf "$TMP/$NAME.tar.gz" -C "$TMP"
  mkdir -p "$(dirname "$RC")" && mv "$TMP/$NAME/rcodesign" "$RC" && rm -rf "$TMP"
fi

echo "==> Building"
pnpm tauri build --debug --bundles app 2>&1 | grep -E 'error|warning: unused|Finished 1 bundle' || true
APP="$PWD/target/debug/bundle/macos/Rec.app"
[[ -d "$APP" ]] || { echo "build failed"; exit 1; }

P12=""
for KEY in "$HOME/.tauri/rec-developer-id/rec-developer-id.p12" "$HOME/.tauri/rec-codesign/rec-codesign.p12"; do
  [[ -f "$KEY" ]] && { P12="$KEY"; break; }
done
if [[ -n "$P12" ]]; then
  "$RC" sign --p12-file "$P12" --p12-password "$(cat "$(dirname "$P12")/p12-password.txt")" "$APP" >/dev/null 2>&1
  echo "==> Signed with $(basename "$P12" .p12)"
else
  echo "==> No certificate in ~/.tauri: ad-hoc signed (macOS will ask for Screen Recording again)"
fi

pkill -f "Rec.app/Contents/MacOS/Rec" 2>/dev/null || true
sleep 1
open -n "$APP" ${REC_DEBUG:+--env REC_DEBUG=1}
echo "==> Rec is running. Logs: ~/Library/Logs/com.alexhem.rec/Rec.log"
