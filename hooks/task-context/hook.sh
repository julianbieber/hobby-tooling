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
# A coordinator session is the one case that runs outside a task: its cwd is the
# project's main checkout, so it is told which coordinator it is instead —
# from HCOORD_NOTES_DIR when `hcdev coordinator` set it, else from the open
# coordinators under notes/<project>/coordinator/.
#
# Input: JSON on stdin from Claude Code SessionStart, with { cwd, source, ... }.
# Output (exit 0): JSON { hookSpecificOutput: { hookEventName, additionalContext } }.
# Effect-only: never blocks; prints nothing when cwd is neither a task nor a main
# checkout with a coordinator.

set -uo pipefail

WORKTREE_BASE="$HOME/fun_worktrees"
NOTES_BASE="$HOME/fun_repos/hobby-mimisbrunnr/notes"
PROJECTS_BASE="$HOME/fun_repos"

INPUT=$(cat)
CWD=$(printf '%s' "$INPUT" | jaq -r '.cwd // empty' 2>/dev/null || true)
CWD="${CWD:-$PWD}"

emit() {
  jaq -n --arg c "$1" '{hookSpecificOutput: {hookEventName: "SessionStart", additionalContext: $c}}'
  exit 0
}

coordinator_context() {
  local project="$1" dir="$2" blurb
  blurb=$(basename "$dir")
  local ctx="Coordinator session **${blurb}** for project \`${project}\` — this is the main checkout, not a task worktree."$'\n'
  ctx="${ctx}Coordinator notes: \`${dir/#$HOME/\~}\` — the task list is \`tasks.json\` there; \`/task:coordinate\` reads and writes it, the pane on the left shows it."$'\n'
  if [ -f "$dir/tasks.json" ]; then
    local counts
    counts=$(command -v hcoord >/dev/null 2>&1 && hcoord status "$dir" 2>/dev/null || true)
    [ -n "$counts" ] && ctx="${ctx}Tasks: ${counts}"$'\n'
  fi
  local last
  last=$(find "$dir" -maxdepth 1 -type f -name '????-??-??.md' -printf '%f\n' 2>/dev/null | sort -r | head -n1 || true)
  [ -n "$last" ] && ctx="${ctx}Latest journal entry: ${last%.md}"$'\n'
  ctx="${ctx}Issues for hobby projects are \`issue.md\` files in the task notes dirs — no GitHub issues, no Jira."
  emit "$ctx"
}

# A main checkout: ~/fun_repos/<project>, with no deeper path component.
case "$CWD" in
  "$PROJECTS_BASE"/*/*) ;;
  "$PROJECTS_BASE"/*)
    PROJECT="${CWD#"$PROJECTS_BASE"/}"
    if [ -n "${HCOORD_NOTES_DIR:-}" ] && [ -d "$HCOORD_NOTES_DIR" ]; then
      coordinator_context "$PROJECT" "$HCOORD_NOTES_DIR"
    fi
    open=()
    for d in "$NOTES_BASE/$PROJECT/coordinator"/*/; do
      [ -f "$d/task.json" ] || continue
      [ "$(jaq -r '.done // false' "$d/task.json" 2>/dev/null)" = "true" ] && continue
      open+=("${d%/}")
    done
    if [ "${#open[@]}" -eq 1 ]; then
      coordinator_context "$PROJECT" "${open[0]}"
    elif [ "${#open[@]}" -gt 1 ]; then
      ctx="Main checkout of \`${PROJECT}\` with ${#open[@]} open coordinators — \`/task:coordinate\` has to be told which:"$'\n'
      for d in "${open[@]}"; do ctx="${ctx}- \`${d/#$HOME/\~}\`"$'\n'; done
      emit "$ctx"
    fi
    exit 0
    ;;
esac

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
  blocked_v=$(jaq -r 'if (.blocked // null) == null then "no" else ((.blocked.reason // "yes") + (if .blocked.until then " (until " + .blocked.until + ")" else "" end)) end' "$META" 2>/dev/null || echo "no")
  CTX="${CTX}Task status: done=${done_v}, blocked=${blocked_v}"$'\n'
else
  # No task.json: this worktree is an orphan. `hwt orphans` lists them, hdash offers Adopt.
  CTX="${CTX}This worktree has no \`task.json\` in the notes repo, so it is not a recorded task (an orphan). \`hwt orphans\` lists them; hdash offers **Adopt**."$'\n'
fi

# --- notes ------------------------------------------------------------------
if [ -d "$NOTES_DIR" ]; then
  CTX="${CTX}Notes: \`${NOTES_DIR/#$HOME/\~}\`"$'\n'
  # Most recent journal entry, so a resume knows where to pick up.
  LAST=$(find "$NOTES_DIR" -maxdepth 1 -type f -name '????-??-??.md' -printf '%f\n' 2>/dev/null | sort -r | head -n1 || true)
  [ -n "$LAST" ] && CTX="${CTX}Latest journal entry: ${LAST%.md}"$'\n'
  if [ -f "$NOTES_DIR/issue.md" ]; then
    ISSUE_TITLE=$(sed -n '2,/^---$/s/^title: *"\{0,1\}\(.*[^"]\)"\{0,1\} *$/\1/p' "$NOTES_DIR/issue.md" | head -n1)
    CTX="${CTX}Issue: \`issue.md\` in the notes dir${ISSUE_TITLE:+ — ${ISSUE_TITLE}}"$'\n'
  fi
  # Flag HMML specs so the hmml-* skills don't have to hunt for them.
  MML_COUNT=$(find "$NOTES_DIR" -maxdepth 1 -type f -name '*.hmml' 2>/dev/null | wc -l)
  [ "$MML_COUNT" -gt 0 ] && CTX="${CTX}HMML specs in notes: ${MML_COUNT}"$'\n'
else
  CTX="${CTX}Notes dir does not exist yet (would be \`${NOTES_DIR/#$HOME/\~}\`)."$'\n'
fi

CTX="${CTX}Issues for hobby projects are \`issue.md\` files in the task notes dirs — no GitHub issues, no Jira."

jaq -n --arg c "$CTX" '{hookSpecificOutput: {hookEventName: "SessionStart", additionalContext: $c}}'
exit 0
