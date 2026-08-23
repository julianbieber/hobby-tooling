#!/usr/bin/env bash
# Shared definitions for the git-side AI policy checks.
#
# Sourced by: githooks/commit-msg, githooks/pre-push,
#             hooks/human-authored-text/hook.sh
#
# A second copy of the regex is a message one of them accepts and another rejects.

# core.hooksPath is global, so these hooks run in every repo on the machine.
# Outside the hobby tree that would mean rejecting work commits for carrying the
# work identity.
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
# Six shapes, not one: the trailer, the session link, the footer, a claude.ai/code
# URL, the noreply address, the emoji. The harness system prompt instructs several
# of them by name, so they arrive without anyone choosing to add them.
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
# The trailing colon separates a marker from a mention: CLAUDE.md and the comment
# skills name the token in prose without one.
POLICY_PLACEHOLDER_RE='TODO\(jb-(comment|doc)\):'

# `upstream` or `private` for a directory, from scripts/ai-policy-tier.sh — the
# single definition of the tier. Prints nothing and sets nothing if the resolver
# is missing; callers treat an empty result as `upstream`, since failing closed is
# the only safe direction for a policy check.
policy_tier() {
    local here resolver
    here="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)"
    resolver="$here/../scripts/ai-policy-tier.sh"
    [ -r "$resolver" ] || return 1
    # shellcheck disable=SC1090
    . "$resolver"
    resolve_tier "${1:-$PWD}"
    printf '%s\n' "$AI_POLICY_TIER_RESOLVED"
}

# `just git-identity` sets this on hobby-tooling and the notes repo only. Every
# other hobby repo inherits the global identity, which is the work one.
POLICY_GIT_EMAIL='julian.bieber.1992@gmail.com'

policy_die() {
    printf '\n\033[31m✗ %s\033[0m\n\n' "$1" >&2
    shift
    [ "$#" -gt 0 ] && printf '%s\n' "$@" >&2
    printf '\n' >&2
    exit 1
}
