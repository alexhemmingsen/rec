#!/usr/bin/env bash
# Build Rec, then let the end-to-end driver operate it on this Mac: it
# presses your shortcut, drags, clicks Stop and checks the video. Takes over
# the mouse and keyboard for about a minute. Needs Accessibility for your
# terminal and Screen Recording for Rec. Test recordings are deleted after.
set -euo pipefail
cd "$(dirname "$0")/.."
./scripts/dev-mac.sh
pkill -f "Rec.app/Contents/MacOS/Rec" 2>/dev/null || true
sleep 1

DRIVER=$(mktemp -d)/rec-e2e
swiftc -O -o "$DRIVER" e2e/macos/RecE2E.swift
BEFORE=$(ls -1 ~/Downloads/rec-*.mp4 2>/dev/null || true)
STATUS=0
"$DRIVER" "$PWD/target/debug/bundle/macos/Rec.app" || STATUS=$?

# Remove the test's recordings (yours stay) and their History entries.
osascript -e 'tell application id "com.alexhem.rec" to quit' 2>/dev/null || true
osascript -e 'tell application "Calculator" to quit' 2>/dev/null || true
sleep 1
NEW=$(comm -13 <(echo "$BEFORE" | sort) <(ls -1 ~/Downloads/rec-*.mp4 2>/dev/null | sort) || true)
if [[ -n "$NEW" ]]; then
  python3 - "$NEW" <<'PY'
import json, os, sys
new = [p for p in sys.argv[1].splitlines() if p]
hist = os.path.expanduser("~/Library/Application Support/com.alexhem.rec/history.json")
if os.path.exists(hist):
    data = json.load(open(hist))
    for e in data["entries"]:
        if e["path"] in new and e.get("thumbnail") and os.path.exists(e["thumbnail"]):
            os.remove(e["thumbnail"])
    data["entries"] = [e for e in data["entries"] if e["path"] not in new]
    json.dump(data, open(hist, "w"), indent=2)
for p in new:
    os.remove(p)
print(f"removed {len(new)} test recording(s)")
PY
fi
exit $STATUS
