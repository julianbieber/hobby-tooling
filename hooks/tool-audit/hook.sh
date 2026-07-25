#!/usr/bin/env bash
# Formats PreToolUse hook input and appends to audit log.
# Designed to be tailed in a Zellij side pane:
#   tail -f ~/.claude-hobby/tool-audit.log
#
# Input: JSON on stdin with { tool_name, tool_input, cwd } from Claude Code PreToolUse hook.

set -euo pipefail

LOG="${CLAUDE_TOOL_AUDIT_LOG:-$HOME/.claude-hobby/tool-audit.log}"
INPUT=$(cat)

TOOL=$(echo "$INPUT" | jaq -r '.tool_name // "unknown"')
TS=$(date '+%H:%M:%S')
SEP="─────────────────────────────────────"

# Session cwd at hook-fire time, for repo attribution (e.g. which tree a
# `git push` targeted). The payload carries it; fall back to $PWD. Note this
# is the *session* cwd — a `cd X && …` inside a Bash command runs in a subshell
# and won't change it, so for those the command text itself is the source of truth.
CWD=$(echo "$INPUT" | jaq -r '.cwd // empty')
CWD="${CWD:-$PWD}"
CWD="${CWD/#$HOME/\~}"

{
  echo ""
  echo "${SEP}"
  echo "[$TS] $TOOL  (cwd: $CWD)"
  echo "${SEP}"

  case "$TOOL" in
    Bash)
      echo "$INPUT" | jaq -r '.tool_input.command // empty'
      ;;
    Read)
      echo "$INPUT" | jaq -r '.tool_input.file_path // empty'
      ;;
    Write)
      FILE=$(echo "$INPUT" | jaq -r '.tool_input.file_path // empty')
      LINES=$(echo "$INPUT" | jaq -r '.tool_input.content // empty' | wc -l)
      echo "$FILE  (${LINES} lines)"
      ;;
    Edit)
      FILE=$(echo "$INPUT" | jaq -r '.tool_input.file_path // empty')
      OLD=$(echo "$INPUT" | jaq -r '.tool_input.old_string // empty')
      NEW=$(echo "$INPUT" | jaq -r '.tool_input.new_string // empty')
      echo "$FILE"
      echo "--- old"
      echo "$OLD"
      echo "+++ new"
      echo "$NEW"
      ;;
    Grep)
      PATTERN=$(echo "$INPUT" | jaq -r '.tool_input.pattern // empty')
      PATH_ARG=$(echo "$INPUT" | jaq -r '.tool_input.path // "."')
      GLOB=$(echo "$INPUT" | jaq -r '.tool_input.glob // empty')
      echo "/$PATTERN/  in $PATH_ARG  ${GLOB:+glob=$GLOB}"
      ;;
    Glob)
      echo "$INPUT" | jaq -r '.tool_input.pattern // empty'
      ;;
    Agent)
      DESC=$(echo "$INPUT" | jaq -r '.tool_input.description // empty')
      TYPE=$(echo "$INPUT" | jaq -r '.tool_input.subagent_type // "general"')
      echo "[$TYPE] $DESC"
      ;;
    Skill)
      echo "$INPUT" | jaq -r '.tool_input.skill // empty'
      ;;
    mcp__*)
      # For MCP tools, show the full input compactly
      echo "$INPUT" | jaq -r '.tool_input' 2>/dev/null || echo "$INPUT"
      ;;
    *)
      # Fallback: show raw tool_input
      echo "$INPUT" | jaq -r '.tool_input' 2>/dev/null || echo "$INPUT"
      ;;
  esac
} >> "$LOG"
