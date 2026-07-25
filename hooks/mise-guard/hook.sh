#!/bin/bash
# PreToolUse hook: blocks Bash commands that use mise-managed tools
# without proper project context (mise exec -C).
#
# When Claude runs from a parent worktree dir (~/worktrees/TOPIC/TASK/),
# project-specific .mise.toml files in subdirectories aren't activated.
# This hook catches bare tool invocations and tells Claude to fix them.
#
# BASH_ENV activates mise in non-interactive shells, so `cd` now triggers
# mise hook-env. Both `cd <dir> && <cmd>` and `mise exec -C <dir>` work.

input=$(cat)
tool_name=$(echo "$input" | jaq -r '.tool_name')

# Only check Bash commands
if [ "$tool_name" != "Bash" ]; then
  exit 0
fi

command=$(echo "$input" | jaq -r '.tool_input.command')

# Tools managed by mise that need project-specific versions
# Word-boundary match: tool must appear as a command (not substring)
MISE_TOOLS='(^|[ \t;|&]+)(sbt|java|javac|node|npm|npx|yarn|pnpm|python|python3|pip|pip3|go|cargo|rustc|terraform|gradle|mvn|maven)( |$)'

# Check if command references a mise-managed tool
if ! echo "$command" | grep -qP "$MISE_TOOLS"; then
  exit 0
fi

# Allow if already wrapped with directory context (cd or mise exec)
if echo "$command" | grep -qP '(^cd\b|&&\s*cd\b|mise\s+exec|mise\s+x\b|mise\s+run)'; then
  exit 0
fi

# Allow if cwd itself has a .mise.toml — mise exec from here will work,
# and the global PATH already has the right shims for this dir
if [ -f .mise.toml ] || [ -f .tool-versions ]; then
  exit 0
fi

# Find project dirs with .mise.toml in current tree (up to 3 levels)
mise_dirs=$(find . -mindepth 1 -maxdepth 3 -name '.mise.toml' -printf '  %h\n' 2>/dev/null | sort -u)

if [ -z "$mise_dirs" ]; then
  # No .mise.toml found nearby — nothing to guard against
  exit 0
fi

cat <<EOF
{"decision": "block", "reason": "Mise-managed tool used without project context. The current directory has no .mise.toml, so global tool versions will be used (wrong Java version, missing sbt, etc.).\n\nFix: prefix with \`cd <project-dir> &&\` or use \`mise exec -C <project-dir> --\`\nExample: \`cd ./Patagona-API && sbt compile\`\n\nDirectories with .mise.toml:\n${mise_dirs}"}
EOF
