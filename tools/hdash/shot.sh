#!/usr/bin/env bash
# Launch hdash, screenshot just its window, then close it. Started without a terminal, hdash
# re-runs itself in an Alacritty window of class hdash, so its window is found by class.
#
#   ./shot.sh [OUT]
#     OUT  where to write the PNG (default /tmp/hdash-shot.png)
#
# Needs xdotool + imagemagick. Falls back to a full-root screenshot if the window never
# appears, so a failed launch still leaves evidence rather than nothing.
set -u
OUT="${1:-/tmp/hdash-shot.png}"
BIN=hdash
LOG="${XDG_CACHE_HOME:-$HOME/.cache}/hdash/hdash.log"
LOG_START=$(wc -l <"$LOG" 2>/dev/null || echo 0)
cd "$(dirname "$0")"
setsid ./target/release/"$BIN" </dev/null >/tmp/hdash-run.log 2>&1 &
APP=$!
WID=""
for _ in $(seq 1 60); do
  WID=$(xdotool search --class "$BIN" 2>/dev/null | tail -1)
  [ -n "$WID" ] && break
  sleep 0.25
done
sleep 2.0  # let the UI render a few frames
if [ -n "$WID" ]; then
  xdotool windowactivate "$WID" 2>/dev/null
  xdotool windowraise "$WID" 2>/dev/null
  sleep 0.4
  import -window "$WID" "$OUT" 2>/tmp/hdash-import.log
else
  import -window root "$OUT" 2>/tmp/hdash-import.log
fi
[ -n "$WID" ] && xdotool windowclose "$WID" 2>/dev/null
kill "$APP" 2>/dev/null; wait "$APP" 2>/dev/null
echo "WID=$WID OUT=$OUT"
echo "--- warnings/errors of this run ---"
grep -iE "warn|error|panic" /tmp/hdash-run.log | head -20
tail -n +"$((LOG_START + 1))" "$LOG" 2>/dev/null | head -20
