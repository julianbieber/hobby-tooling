#!/usr/bin/env bash
# Launch a dashboard binary (default hwt-overview), screenshot just its window, then kill it.
set -u
OUT="${1:-/tmp/hwt-shot.png}"
BIN="${2:-hwt-overview}"
cd "$(dirname "$0")"
./target/release/"$BIN" >/tmp/hwt-run.log 2>&1 &
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
  import -window "$WID" "$OUT" 2>/tmp/hwt-import.log
else
  import -window root "$OUT" 2>/tmp/hwt-import.log
fi
kill "$APP" 2>/dev/null; wait "$APP" 2>/dev/null
echo "WID=$WID OUT=$OUT"
echo "--- run.log (warnings/errors) ---"
grep -iE "warn|error|panic" /tmp/hwt-run.log | head -20
