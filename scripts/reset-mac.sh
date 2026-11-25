#!/usr/bin/env bash
# Reset Rec on this Mac to a first launch, so the next start shows onboarding.
#
# Quits Rec (Rec.app and `pnpm tauri dev`), then removes its settings, history,
# thumbnails, web storage (the saved onboarding step), caches, login item and
# Screen Recording and Accessibility permissions. Recordings themselves are left where they are.
set -euo pipefail

ID=com.alexhem.rec

pkill -f "Rec.app/Contents/MacOS/Rec" 2>/dev/null || true
pkill -f "target/debug/rec$" 2>/dev/null || true
sleep 1

rm -rf \
  "$HOME/Library/Application Support/$ID" \
  "$HOME/Library/WebKit/$ID" \
  "$HOME/Library/WebKit/rec" \
  "$HOME/Library/Caches/$ID" \
  "$HOME/Library/Caches/rec"

# "Open at login" is a LaunchAgent named after the app.
for plist in "$HOME/Library/LaunchAgents/Rec.plist" "$HOME/Library/LaunchAgents/rec.plist"; do
  if [[ -f "$plist" ]]; then
    launchctl unload "$plist" 2>/dev/null || true
    rm -f "$plist"
  fi
done

# Screen Recording and Accessibility permissions for Rec.app. (Under
# `pnpm tauri dev` they belong to your terminal, which this leaves alone.)
tccutil reset ScreenCapture "$ID" >/dev/null 2>&1 || true
tccutil reset Accessibility "$ID" >/dev/null 2>&1 || true

echo "==> Rec is reset. Start it with scripts/dev-mac.sh (or pnpm tauri dev) to see onboarding."
