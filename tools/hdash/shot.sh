#!/usr/bin/env bash
# Launch the dashboard, screenshot just its window, then kill it.
#
#   ./shot.sh [OUT]
#     OUT  where to write the PNG (default /tmp/hdash-shot.png)
#
# Needs xdotool + imagemagick. Falls back to a full-root screenshot if the window
# never appears, so a failed launch still leaves evidence rather than nothing.
set -u
OUT="${1:-/tmp/hdash-shot.png}"
BIN="hdash"
HERE="$(cd "$(dirname "$0")" && pwd)"
EXE="$HERE/target/release/$BIN"
"$EXE" >/tmp/hdash-run.log 2>&1 &
APP=$!
WID=""
for _ in $(seq 1 60); do
  WID=$(xdotool search --name "$BIN" 2>/dev/null | head -1)
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
kill "$APP" 2>/dev/null; wait "$APP" 2>/dev/null
echo "WID=$WID OUT=$OUT"
echo "--- run.log (warnings/errors) ---"
grep -iE "warn|error|panic" /tmp/hdash-run.log | head -20
