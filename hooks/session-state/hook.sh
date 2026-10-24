#!/usr/bin/env -S -u BASH_ENV bash
# Claude Code hook: record whether this hobby-account session is busy (mid-turn) or idle (waiting
# on the user) in one small JSON file per session, for the hdash dashboard to read.
#
# Input: JSON on stdin from any of SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, Stop,
# Notification, SessionEnd. Effect only — never blocks, always exits 0. File format: README.md.

set -uo pipefail
trap 'exit 0' EXIT
umask 077

INPUT=$(cat)
IFS=$'\x1f' read -r EVENT SID NTYPE SOURCE CWD < <(
  printf '%s' "$INPUT" | jaq -r '[.hook_event_name, .session_id, .notification_type, .source, .cwd] | map(. // "" | tostring) | join("\u001f")' 2>/dev/null
)

[[ "$SID" =~ ^[A-Za-z0-9-]{1,64}$ ]] || exit 0

DIR="${XDG_RUNTIME_DIR:-$HOME/.cache}/hobby-session-state"
FILE="$DIR/$SID.json"
LOCK="$DIR/$SID.lock"

FRESH=0
case "$EVENT" in
  SessionEnd)
    rm -f "$FILE" "$LOCK"
    exit 0
    ;;
  SessionStart)
    if [ "$SOURCE" = "compact" ]; then
      [ -f "$FILE" ] && touch "$FILE"
      exit 0
    fi
    STATE=idle
    FRESH=1
    ;;
  Stop) STATE=idle ;;
  Notification)
    case "$NTYPE" in
      permission_prompt | idle_prompt | elicitation_dialog) STATE=idle ;;
      *) exit 0 ;;
    esac
    ;;
  UserPromptSubmit)
    STATE=busy
    FRESH=1
    ;;
  PreToolUse | PostToolUse) STATE=busy ;;
  *) exit 0 ;;
esac

if [ "$FRESH" = 0 ] && [ -f "$FILE" ] && [[ "$(<"$FILE")" == *"\"state\":\"$STATE\""* ]]; then
  touch "$FILE"
  exit 0
fi

mkdir -p -m 700 "$DIR"

claude_ancestor() {
  local pid=$1 comm
  for _ in 1 2 3 4 5 6 7 8; do
    [ "$pid" -gt 1 ] 2>/dev/null || return 1
    comm=$(<"/proc/$pid/comm") || return 1
    if [ "$comm" = "claude" ]; then
      echo "$pid"
      return 0
    fi
    pid=$(parent_of "$pid") || return 1
  done
  return 1
}

pid_start() {
  awk '{ sub(/.*\) /, ""); print $20 }' "/proc/$1/stat"
}

parent_of() {
  awk '{ sub(/.*\) /, ""); print $2 }' "/proc/$1/stat"
}

PID=$(claude_ancestor "$PPID") || PID=0
START=0
[ "$PID" -gt 0 ] && START=$(pid_start "$PID")

(
  flock -w 2 9 || exit 0
  if [ "$FRESH" = 0 ] && [ -f "$FILE" ] && [[ "$(<"$FILE")" == *"\"state\":\"$STATE\""* ]]; then
    touch "$FILE"
    exit 0
  fi
  TMP=$(mktemp "$DIR/.$SID.XXXXXX") || exit 0
  jaq -nc \
    --arg state "$STATE" \
    --argjson since "$(date +%s)" \
    --arg project_dir "${CLAUDE_PROJECT_DIR:-$CWD}" \
    --arg zellij_session "${ZELLIJ_SESSION_NAME:-}" \
    --arg zellij_pane "${ZELLIJ_PANE_ID:-}" \
    --argjson pid "$PID" \
    --argjson pid_start "${START:-0}" \
    '{version: 1, state: $state, since: $since, project_dir: $project_dir,
      zellij_session: $zellij_session, zellij_pane: $zellij_pane,
      pid: $pid, pid_start: $pid_start}' >"$TMP" && mv -f "$TMP" "$FILE"
  rm -f "$TMP"
) 9>"$LOCK"
