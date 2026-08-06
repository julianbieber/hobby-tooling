#!/usr/bin/env bash
# Shared definitions for the git-side AI policy checks.
#
# Sourced by: githooks/commit-msg, githooks/pre-push,
#             hooks/human-authored-text/hook.sh
#
# TODO(jb-doc): why these live in one file instead of one copy per hook

# TODO(jb-comment): why scope is by path, and why the git hooks must no-op
# outside the hobby tree even though they install globally
policy_in_scope() {
    local dir="${1:-$PWD}"
    dir=$(cd "$dir" 2>/dev/null && pwd -P) || return 1
    case "$dir" in
        "$HOME"/fun_repos/*|"$HOME"/fun_repos) return 0 ;;
        "$HOME"/fun_worktrees/*)               return 0 ;;
        *)                                     return 1 ;;
    esac
}

# Matched case-insensitively with grep -E.
# TODO(jb-comment): why each of these five patterns is banned, and which one
# Bevy CI actually tests for
POLICY_ATTRIBUTION_RE='(co-authored-by:[[:space:]]*.*(claude|anthropic))|(claude-session:)|(generated with .*claude code)|(claude\.ai/code)|(noreply@anthropic\.com)|(🤖)'

# Printed on rejection, in the order the regex tests.
POLICY_ATTRIBUTION_DESC='  Co-authored-by: … Claude/Anthropic
  Claude-Session: …
  Generated with … Claude Code
  claude.ai/code links
  noreply@anthropic.com
  🤖'

policy_find_attribution() {
    grep -n -i -E "$POLICY_ATTRIBUTION_RE" "$@" 2>/dev/null
}

# TODO(jb-comment): why the placeholder token is spelled in exactly one place
# TODO(jb-comment): why the trailing colon is part of the pattern
POLICY_PLACEHOLDER_RE='TODO\(jb-(comment|doc)\):'

# TODO(jb-comment): why the expected email is checked at all, given `just
# git-identity` sets it — i.e. which repos it does not cover
POLICY_GIT_EMAIL='julian.bieber.1992@gmail.com'

policy_die() {
    printf '\n\033[31m✗ %s\033[0m\n\n' "$1" >&2
    shift
    [ "$#" -gt 0 ] && printf '%s\n' "$@" >&2
    printf '\n' >&2
    exit 1
}
