#!/usr/bin/env bash
# PostToolUse hook: when the agent opens a PR with `gh pr create`, record it in the
# task's notes `prs.json` so hwt-overview can show its live status.
#
# Ported from the work hook. The only real change is the path parse: hobby tasks
# live at ~/fun_worktrees/PROJECT/TOPIC/TASK, three levels rather than two, and
# the notes tree mirrors that.
#
# Input: JSON on stdin from Claude Code PostToolUse, with
#        { tool_name, tool_input, tool_output, cwd }.
# Effect only — never blocks, always exits 0. Does not commit the notes repo.

set -uo pipefail

INPUT=$(cat)

# Only Bash tool calls.
TOOL=$(echo "$INPUT" | jaq -r '.tool_name // empty')
[ "$TOOL" = "Bash" ] || exit 0

# Only `gh pr create` invocations.
CMD=$(echo "$INPUT" | jaq -r '.tool_input.command // empty')
echo "$CMD" | grep -Eq 'gh[[:space:]]+pr[[:space:]]+create' || exit 0

# Only on success. The exit-code field location has varied across Claude Code versions.
EXIT=$(echo "$INPUT" | jaq -r '.tool_output.exit_code // .tool_response.exit_code // 0')
[ "$EXIT" = "0" ] || exit 0

# Grab the command's stdout (field name has varied; try the known variants).
OUT=$(echo "$INPUT" | jaq -r '.tool_output.stdout // .tool_response.stdout // .tool_response // empty')

# Extract the PR URL `gh pr create` prints.
URL=$(echo "$OUT" | grep -Eo 'https://github\.com/[^[:space:]]+/pull/[0-9]+' | head -n1 || true)
[ -n "$URL" ] || exit 0

# Determine project/topic/task from the working directory:
# ~/fun_worktrees/PROJECT/TOPIC/TASK/...
WORKTREE_BASE="$HOME/fun_worktrees"
CWD=$(echo "$INPUT" | jaq -r '.cwd // empty')
case "$CWD" in
  "$WORKTREE_BASE"/*/*/*) : ;;
  *) exit 0 ;;
esac
REL=${CWD#"$WORKTREE_BASE"/}
PROJECT=${REL%%/*}
REST=${REL#*/}
TOPIC=${REST%%/*}
REST=${REST#*/}
TASK=${REST%%/*}
[ -n "$PROJECT" ] && [ -n "$TOPIC" ] && [ -n "$TASK" ] || exit 0

# owner/repo is the base repo in the URL itself (more reliable than gh fields).
REPO=$(echo "$URL" | sed -nE 's#https://github\.com/([^/]+/[^/]+)/pull/[0-9]+#\1#p')
OPENED=$(date '+%Y-%m-%d')
TITLE=$(gh pr view "$URL" --json title -q '.title' 2>/dev/null || true)

NOTES_DIR="$HOME/fun_repos/hobby-mimisbrunnr/notes/$PROJECT/$TOPIC/$TASK"
PRS="$NOTES_DIR/prs.json"
mkdir -p "$NOTES_DIR"
[ -s "$PRS" ] || echo '[]' > "$PRS"

# Skip if this URL is already recorded.
if jaq -e --arg u "$URL" 'any(.[]?; .url == $u)' "$PRS" >/dev/null 2>&1; then
  exit 0
fi

# Append the entry (omit repo/title when empty).
TMP=$(mktemp)
if jaq --arg url "$URL" --arg repo "$REPO" --arg title "$TITLE" --arg opened "$OPENED" \
  '. + [ ({url: $url}
          + (if $repo  != "" then {repo: $repo}   else {} end)
          + (if $title != "" then {title: $title} else {} end)
          + {opened: $opened}) ]' \
  "$PRS" > "$TMP" 2>/dev/null; then
  mv "$TMP" "$PRS"
else
  rm -f "$TMP"
fi

exit 0
