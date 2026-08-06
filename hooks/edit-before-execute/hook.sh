#!/usr/bin/env bash
# PermissionRequest hook: opens $EDITOR to let the user tweak text before execution.
# Runs on PermissionRequest (not PreToolUse) so it only fires when the user
# would actually be prompted — auto-allowed tools skip this entirely.
#
# Triggers:
#   - Write tool calls where content exceeds a size threshold
#
# TODO(jb-comment): why the Bash triggers were removed, and what treating an
# unchanged buffer as approval did that made this unusable as a policy gate
#
# Behavior:
#   - Content unchanged → exit 0 (normal accept/reject flow)
#   - Content edited     → return updatedInput + allow (editor IS approval)
#   - Content emptied    → block the tool call (rejection)

set -euo pipefail

EDIT_MIN_CHARS="${CLAUDE_EDIT_MIN_CHARS:-500}"
INPUT=$(cat)

TOOL=$(echo "$INPUT" | jaq -r '.tool_name // "unknown"')

# --- Decide what to edit ------------------------------------------------

case "$TOOL" in
  Bash)
    # Message-bearing commands now go through hooks/human-authored-text.
    exit 0
    ;;
  Write)
    CONTENT=$(echo "$INPUT" | jaq -r '.tool_input.content // empty')
    if [ "${#CONTENT}" -lt "$EDIT_MIN_CHARS" ]; then
      exit 0
    fi
    EDITABLE="$CONTENT"
    MODE="write"
    ;;
  *)
    exit 0
    ;;
esac

# --- Open editor ---------------------------------------------------------

TMPFILE=$(mktemp --suffix=.md)
DONEFILE="${TMPFILE}.done"
# shellcheck disable=SC2064
trap "rm -f '$TMPFILE' '$DONEFILE'" EXIT

printf '%s' "$EDITABLE" > "$TMPFILE"
BEFORE=$(md5sum "$TMPFILE" | cut -d' ' -f1)

if [ -n "${ZELLIJ:-}" ]; then
  # Inside Zellij: open editor in a floating pane
  zellij run --floating --close-on-exit --name "edit-hook" -- \
    bash -c "${EDITOR:-vim} '$TMPFILE' && touch '$DONEFILE'"
else
  # Standalone: open editor in a new terminal window
  # Try common terminals in order of preference
  EDIT_CMD="${EDITOR:-vim} '$TMPFILE' && touch '$DONEFILE'"
  if command -v kitty &>/dev/null; then
    kitty --title "Claude Edit" bash -c "$EDIT_CMD" &
  elif command -v alacritty &>/dev/null; then
    alacritty --title "Claude Edit" -e bash -c "$EDIT_CMD" &
  elif command -v gnome-terminal &>/dev/null; then
    gnome-terminal --title "Claude Edit" -- bash -c "$EDIT_CMD" &
  elif command -v xterm &>/dev/null; then
    xterm -title "Claude Edit" -e bash -c "$EDIT_CMD" &
  else
    # Last resort: write instructions to stderr and wait
    echo "── Edit hook: modify $TMPFILE then run: touch $DONEFILE ──" >&2
  fi
fi

# Wait for the editor to close (sentinel appears)
while [ ! -f "$DONEFILE" ]; do
  sleep 0.2
done

AFTER=$(md5sum "$TMPFILE" | cut -d' ' -f1)
EDITED=$(cat "$TMPFILE")

# --- Produce output -----------------------------------------------------

# Emptied → block
if [ -z "$EDITED" ]; then
  jaq -n '{"decision": "block", "reason": "Blocked: content was emptied in editor."}'
  exit 0
fi

# Unchanged → fall through to normal accept/reject
if [ "$BEFORE" = "$AFTER" ]; then
  exit 0
fi

# Edited → return updatedInput and auto-allow
case "$MODE" in
  bash)
    jaq -n --arg cmd "$EDITED" '{
      "hookSpecificOutput": {
        "hookEventName": "PermissionRequest",
        "decision": {
          "behavior": "allow",
          "updatedInput": { "command": $cmd }
        }
      }
    }'
    ;;
  write)
    FILE_PATH=$(echo "$INPUT" | jaq -r '.tool_input.file_path // empty')
    jaq -n --arg path "$FILE_PATH" --arg content "$EDITED" '{
      "hookSpecificOutput": {
        "hookEventName": "PermissionRequest",
        "decision": {
          "behavior": "allow",
          "updatedInput": { "file_path": $path, "content": $content }
        }
      }
    }'
    ;;
esac
