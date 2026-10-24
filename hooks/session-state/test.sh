#!/usr/bin/env bash
# Fixture test for hook.sh: feeds hook payloads on stdin and asserts the state file it leaves.
# Run: ./test.sh   (needs jaq; uses a throwaway XDG_RUNTIME_DIR)

set -euo pipefail

HOOK="$(cd "$(dirname "$0")" && pwd)/hook.sh"
export XDG_RUNTIME_DIR
XDG_RUNTIME_DIR=$(mktemp -d)
trap 'rm -rf "$XDG_RUNTIME_DIR"' EXIT
DIR="$XDG_RUNTIME_DIR/hobby-session-state"
SID=0a1b2c3d-test
FILE="$DIR/$SID.json"
FAIL=0

send() {
  jaq -nc --arg e "$1" --arg sid "${3:-$SID}" --arg n "${2:-}" \
    '{hook_event_name: $e, session_id: $sid, notification_type: $n, source: $n, cwd: "/tmp/x"}' |
    env -u CLAUDE_PROJECT_DIR ZELLIJ_SESSION_NAME=hcdev-p-t-x ZELLIJ_PANE_ID=1 "$HOOK"
}
field() { jaq -r ".$1" "$FILE"; }
check() {
  if [ "$2" = "$3" ]; then echo "ok   $1"; else
    echo "FAIL $1: expected '$3', got '$2'"
    FAIL=1
  fi
}

send SessionStart startup
check "start is idle" "$(field state)" idle
check "zellij session recorded" "$(field zellij_session)" hcdev-p-t-x
check "no main flag" "$(field main)" null
check "project dir falls back to cwd" "$(field project_dir)" /tmp/x
check "dir is private" "$(stat -c %a "$DIR")" 700

send UserPromptSubmit
check "prompt is busy" "$(field state)" busy
SINCE=$(field since)
sleep 1.1
send PreToolUse
send PostToolUse
check "repeated busy keeps since" "$(field since)" "$SINCE"

send Notification auth_success
check "unrelated notification ignored" "$(field state)" busy
send SessionStart compact
check "compact keeps state" "$(field state)" busy
send Notification permission_prompt
check "permission prompt is idle" "$(field state)" idle
send PostToolUse
send Stop
check "stop is idle" "$(field state)" idle

send UserPromptSubmit
SINCE=$(field since)
sleep 1.1
send UserPromptSubmit
check "a new prompt restarts since even when already busy" "$([ "$(field since)" -gt "$SINCE" ] && echo yes)" yes
SINCE=$(field since)
sleep 1.1
send SessionStart resume
send SessionStart resume
check "session start always rewrites" "$([ "$(field since)" -gt "$SINCE" ] && echo yes)" yes
send Stop

send Stop "" "../escape"
check "path-like session id rejected" "$(ls -A "$XDG_RUNTIME_DIR" | tr '\n' ' ')" "hobby-session-state "

printf 'not json' | "$HOOK"
check "garbage input exits 0" "$?" 0

send SessionEnd
check "end removes the file" "$([ -e "$FILE" ] && echo present || echo gone)" gone

exit $FAIL
