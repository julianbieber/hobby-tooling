#!/usr/bin/env bash
# SessionStart hook: tell Claude which hobby task this session is in, where its
# notes live, and whether it is done or blocked.
#
# This is the slimmed-down counterpart of the work `task-projects` hook. That one
# existed mainly because a work task directory is a *container* of repo worktrees
# and therefore has no branch or git status of its own, so it had to enumerate the
# repos inside. A hobby task directory IS the worktree, so Claude already gets
# branch and gitStatus for free — all that's left worth injecting is the task
# identity, the notes path, and the task.json state.
#
# Input: JSON on stdin from Claude Code SessionStart, with { cwd, source, ... }.
# Output (exit 0): JSON { hookSpecificOutput: { hookEventName, additionalContext } }.
# Effect-only: never blocks; prints nothing when cwd is not a task.

set -uo pipefail

WORKTREE_BASE="$HOME/fun_worktrees"
NOTES_BASE="$HOME/fun_repos/hobby-mimisbrunnr/notes"
PROJECTS_BASE="$HOME/fun_repos"

INPUT=$(cat)
CWD=$(printf '%s' "$INPUT" | jaq -r '.cwd // empty' 2>/dev/null || true)
CWD="${CWD:-$PWD}"

# Only act inside a task path: ~/fun_worktrees/<project>/<topic>/<task>[/...].
case "$CWD" in
  "$WORKTREE_BASE"/*/*/*) ;;
  *) exit 0 ;;
esac

REL="${CWD#"$WORKTREE_BASE"/}"
PROJECT="${REL%%/*}"
REST="${REL#*/}"
TOPIC="${REST%%/*}"
REST="${REST#*/}"
TASK="${REST%%/*}"
[ -n "$PROJECT" ] && [ -n "$TOPIC" ] && [ -n "$TASK" ] || exit 0

TASK_DIR="$WORKTREE_BASE/$PROJECT/$TOPIC/$TASK"
[ -d "$TASK_DIR" ] || exit 0

NOTES_DIR="$NOTES_BASE/$PROJECT/$TOPIC/$TASK"
MAIN_REPO="$PROJECTS_BASE/$PROJECT"

CTX="Hobby task **${TASK}** — project \`${PROJECT}\`, topic \`${TOPIC}\`."$'\n'
CTX="${CTX}This directory is the git worktree itself (single-repo task); the main checkout is \`${MAIN_REPO/#$HOME/\~}\`."$'\n'

# --- task.json state (notes repo) --------------------------------------------
META="$NOTES_DIR/task.json"
if [ -f "$META" ]; then
  done_v=$(jaq -r '.done // false' "$META" 2>/dev/null || echo false)
  blocked_v=$(jaq -r 'if (.blocked // null) == null then "no" else ((.blocked.reason // "yes") + (if .blocked.until then " (until " + .blocked.until + ")" else "" end) + (if .blocked.pr then " (PR " + .blocked.pr + " " + (.blocked.pr_state // "state unread") + ")" else "" end)) end' "$META" 2>/dev/null || echo "no")
  CTX="${CTX}Task status: done=${done_v}, blocked=${blocked_v}"$'\n'
  if [ -d "$NOTES_DIR/aspects" ]; then
    CTX="${CTX}This task has aspects. Their state is \`aspect.json\` under \`aspects/\` in the notes dir: answer a status question by running \`hwt aspect list\`, never from what this session saw earlier."$'\n'
  fi
else
  CTX="${CTX}This worktree has no \`task.json\` in the notes repo, so it is not a recorded task (an orphan). \`hwt orphans\` lists them; hdash offers **Adopt**."$'\n'
fi

# --- notes ------------------------------------------------------------------
if [ -d "$NOTES_DIR" ]; then
  CTX="${CTX}Notes: \`${NOTES_DIR/#$HOME/\~}\`"$'\n'
  LAST=$(find "$NOTES_DIR" -maxdepth 1 -type f -name '????-??-??.md' -printf '%f\n' 2>/dev/null | sort -r | head -n1 || true)
  [ -n "$LAST" ] && CTX="${CTX}Latest journal entry: ${LAST%.md}"$'\n'
  if [ -f "$NOTES_DIR/issue.md" ]; then
    ISSUE_TITLE=$(sed -n '2,/^---$/s/^title: *"\{0,1\}\(.*[^"]\)"\{0,1\} *$/\1/p' "$NOTES_DIR/issue.md" | head -n1)
    CTX="${CTX}Issue: \`issue.md\` in the notes dir${ISSUE_TITLE:+ — ${ISSUE_TITLE}}"$'\n'
  fi
  MML_COUNT=$(find "$NOTES_DIR" -maxdepth 1 -type f -name 'delta-*.hmml' 2>/dev/null | wc -l)
  [ "$MML_COUNT" -gt 0 ] && CTX="${CTX}HMML review deltas in notes: ${MML_COUNT}"$'\n'
else
  CTX="${CTX}Notes dir does not exist yet (would be \`${NOTES_DIR/#$HOME/\~}\`)."$'\n'
fi

CTX="${CTX}Issues for hobby projects are \`issue.md\` files in the task notes dirs — no GitHub issues, no Jira."

jaq -n --arg c "$CTX" '{hookSpecificOutput: {hookEventName: "SessionStart", additionalContext: $c}}'
exit 0
