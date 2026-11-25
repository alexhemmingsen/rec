#!/usr/bin/env bash
# End-to-end test of Rec on Linux (X11): drives the real app with xdotool and
# checks the overlay, Esc, the Stop pill, the clipboard path and the MP4.
#
#   xvfb-run -a -s "-screen 0 1920x1080x24" e2e/linux/run.sh target/release/rec
#
# Needs xdotool, xclip and ffprobe.
set -euo pipefail

APP=${1:?usage: run.sh path/to/rec}
LOG=$(mktemp)
DATA="${XDG_DATA_HOME:-$HOME/.local/share}/com.alexhem.rec"
PASSED=0

say() { echo "[e2e] $*"; }
fail() {
  echo "[e2e] FAIL: $*"
  echo "--- app log:"; cat "$LOG" || true
  echo "--- X window tree:"; xwininfo -root -tree 2>/dev/null | grep -iE 'rec|"' | head -40 || true
  ffmpeg -loglevel error -y -f x11grab -i "$DISPLAY" -frames:v 1 "${SHOT:-/tmp/rec-e2e.png}" || true
  kill "$PID" 2>/dev/null || true
  exit 1
}
ok() { say "ok: $*"; PASSED=$((PASSED + 1)); }

# Visible windows of the app as "id x y w h name" lines.
windows() {
  for id in $( (xdotool search --onlyvisible --pid "$PID"; xdotool search --onlyvisible --name '^Rec( selection)?$') 2>/dev/null | sort -u); do
    eval "$(xdotool getwindowgeometry --shell "$id" 2>/dev/null)" || continue
    echo "$id $X $Y $WIDTH $HEIGHT $(xdotool getwindowname "$id" 2>/dev/null)"
  done
}
overlay_visible() { windows | grep -q 'Rec selection'; }
pill() { windows | awk '$4 == 168 && $5 == 40 { print; exit }'; }

wait_for() { # description timeout command...
  local what=$1 timeout=$2; shift 2
  local end=$((SECONDS + timeout))
  while ((SECONDS < end)); do
    if "$@"; then ok "$what"; return 0; fi
    sleep 0.2
  done
  say "windows now:"; windows || true
  fail "timed out waiting for: $what"
}
not() { ! "$@"; }
has_pill() { [[ -n "$(pill)" ]]; }
clipboard() { xclip -o -selection clipboard 2>/dev/null || true; }
has_path() { [[ "$(clipboard)" == *.mp4 ]]; }

drag() { # x1 y1 x2 y2
  xdotool mousemove "$1" "$2"; sleep 0.3
  xdotool mousedown 1; sleep 0.1
  for i in $(seq 1 20); do
    xdotool mousemove $(( $1 + ($3 - $1) * i / 20 )) $(( $2 + ($4 - $2) * i / 20 )); sleep 0.02
  done
  xdotool mouseup 1
}

rm -rf "$DATA"
# Real desktops have a window manager; it handles focus and stacking.
if command -v openbox >/dev/null; then
  openbox >/dev/null 2>&1 &
  sleep 1
fi
"$APP" >"$LOG" 2>&1 &
PID=$!
wait_for "Rec's main window" 60 bash -c "xdotool search --onlyvisible --pid $PID >/dev/null 2>&1"
sleep 3
read -r SW SH < <(xdotool getdisplaygeometry)
say "screen ${SW}x${SH}"

# 1 + 2: the shortcut shows the overlay; Esc cancels.
xdotool key ctrl+shift+5
wait_for "the selection overlay after the shortcut" 8 overlay_visible
read -r _ OX OY OW OH _ < <(windows | grep 'Rec selection')
[[ "$OX $OY $OW $OH" == "0 0 $SW $SH" ]] && ok "the overlay covers the screen" || fail "overlay at $OX,$OY ${OW}x$OH"
sleep 0.5
xdotool key Escape
wait_for "Esc to cancel" 5 not overlay_visible

# 3: record a region.
printf '' | xclip -selection clipboard
xdotool key ctrl+shift+5
wait_for "the overlay again" 8 overlay_visible
sleep 0.5
drag 160 140 560 440
wait_for "the Stop pill" 10 has_pill
not overlay_visible && ok "the overlay is gone while recording" || fail "overlay still visible"
read -r PILL PX PY PW PH _ < <(pill)
say "pill at $PX,$PY ${PW}x$PH; region 160,140 400x300"
if (( PX + PW <= 160 || PX >= 560 || PY + PH <= 140 || PY >= 440 )); then ok "the pill sits outside the region"; else fail "the pill overlaps the region"; fi
sleep 2.5
xdotool mousemove $((PX + PW / 2)) $((PY + PH / 2)) click 1
wait_for "an .mp4 path on the clipboard" 15 has_path
FILE=$(clipboard)
say "clipboard: $FILE"
[[ "$FILE" == /* ]] && ok "the path is absolute" || fail "not absolute: $FILE"
[[ "$(basename "$FILE")" =~ ^rec-[0-9]{4}-[0-9]{2}-[0-9]{2}-[0-9]{2}-[0-9]{2}-[0-9]{2}(-[0-9]+)?\.mp4$ ]] && ok "the file name matches rec-YYYY-MM-DD-HH-MM-SS.mp4" || fail "bad name"
[[ -s "$FILE" ]] && ok "the file exists" || fail "missing file"
INFO=$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height:format=duration -of default=nw=1 "$FILE")
say "$(echo "$INFO" | tr '\n' ' ')"
grep -q codec_name=h264 <<<"$INFO" && ok "H.264" || fail "not H.264"
grep -q width=400 <<<"$INFO" && grep -q height=300 <<<"$INFO" && ok "the video has the region's size" || fail "wrong size"
DUR=$(grep duration= <<<"$INFO" | cut -d= -f2)
awk -v d="$DUR" 'BEGIN { exit !(d > 1.8 && d < 8) }' && ok "the length matches the recording ($DUR s)" || fail "duration $DUR"
wait_for "the pill to go away" 8 not has_pill

# 4: a click without dragging cancels.
printf '' | xclip -selection clipboard
xdotool key ctrl+shift+5
wait_for "the overlay for the click test" 8 overlay_visible
sleep 0.5
xdotool mousemove 900 500 click 1
wait_for "a click to cancel" 5 not overlay_visible
sleep 1
! has_pill && [[ -z "$(clipboard)" ]] && ok "nothing was recorded" || fail "something was recorded"

# 5: the whole screen; the pill sits inside the region but must not be in
# the video (Rec covers it with what was there when recording started).
printf '' | xclip -selection clipboard
xdotool key ctrl+shift+5
wait_for "the overlay for the full-screen test" 8 overlay_visible
sleep 0.5
drag 2 2 $((SW - 3)) $((SH - 3))
wait_for "the Stop pill (full screen)" 10 has_pill
read -r _ PX PY PW PH _ < <(pill)
say "pill at $PX,$PY ${PW}x$PH inside the full-screen region"
sleep 2.5
xdotool mousemove $((PX + PW / 2)) $((PY + PH / 2)) click 1
wait_for "a second .mp4 path" 15 bash -c '[[ "$(xclip -o -selection clipboard 2>/dev/null)" == *.mp4 && "$(xclip -o -selection clipboard)" != "'"$FILE"'" ]]'
FILE2=$(clipboard)
RAW=$(mktemp)
ffmpeg -v error -y -sseof -0.6 -i "$FILE2" -frames:v 1 -vf "crop=$PW:$PH:$((PX - 2)):$((PY - 2))" -f rawvideo -pix_fmt rgb24 "$RAW"
RED=$(od -An -v -tu1 -w3 "$RAW" | awk '$1 > 200 && $2 < 110 && $3 < 100 { n++ } END { print n + 0 }')
TOTAL=$(( $(stat -c %s "$RAW") / 3 ))
say "pill area in the video: $TOTAL px, $RED stop-button red"
(( TOTAL > 0 && RED * 200 < TOTAL )) && ok "the Stop pill is not in the video" || fail "the pill shows in the video"
grep -q "covering the Stop pill" "$LOG" && ok "Rec covered the pill's area" || fail "Rec didn't cover the pill"
wait_for "the pill to go away" 8 not has_pill

# 6: History lists both recordings.
grep -q "$FILE" "$DATA/history.json" && grep -q "$FILE2" "$DATA/history.json" && ok "History lists both recordings" || fail "not in History"

kill "$PID" 2>/dev/null || true
echo "--- app log:"; cat "$LOG"
say "PASS ($PASSED checks)"
