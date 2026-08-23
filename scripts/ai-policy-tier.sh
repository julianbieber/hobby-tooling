#!/usr/bin/env bash
# Decide which AI-policy tier a repo is under. One resolver, used by
# githooks/policy-lib.sh, githooks/pre-push, the justfile and the comment skills,
# so no two of them can disagree about whether a repo is upstream-bound.
#
# Two tiers:
#   upstream   anything that can reach a repo I do not own — forks included.
#              Bevy's AI policy applies in full: no AI-written comments at all.
#   private    mine alone. AI may write doc comments on public items.
#
# Usage:
#   ai-policy-tier.sh [DIR]          prints `upstream` or `private`
#   ai-policy-tier.sh [DIR] --why    same, plus how it was decided, on stderr
#
# Sourcing it instead gives the resolver as a function, setting variables rather
# than printing, so a caller does not lose the reason to a $(...) subshell:
#
#   resolve_tier [dir]   -> $AI_POLICY_TIER, $AI_POLICY_TIER_WHY
#
# Overrides: AI_POLICY_TIER from the environment wins over everything.
# HOBBY_AI_POLICY_NO_GH=1 skips the fork probe, which is the only step that
# touches the network.

set -uo pipefail

AI_POLICY_TIER_RESOLVED=""
AI_POLICY_TIER_WHY=""

_tier_repo_root() {
    git -C "${1:-$PWD}" rev-parse --show-toplevel 2>/dev/null
}

_tier_marker() {
    local root="$1" line
    [ -f "$root/CLAUDE.md" ] || return 1
    line=$(grep -m1 -iE '^[[:space:]]*(<!--[[:space:]]*)?ai-policy:[[:space:]]*(upstream|private)' \
        "$root/CLAUDE.md" 2>/dev/null) || return 1
    case "$line" in
        *[Uu]pstream*) printf 'upstream\n' ;;
        *)             printf 'private\n' ;;
    esac
}

_tier_origin_slug() {
    local url
    url=$(git -C "${1:-$PWD}" remote get-url origin 2>/dev/null) || return 1
    case "$url" in
        *github.com[:/]*|*codeberg.org[:/]*) ;;
        *) return 1 ;;
    esac
    printf '%s\n' "$url" \
        | sed -E 's#^.*(github\.com|codeberg\.org)[:/]+##; s#\.git$##; s#/$##'
}

_tier_origin_owner() {
    local slug
    slug=$(_tier_origin_slug "${1:-$PWD}") || return 1
    printf '%s\n' "${slug%%/*}"
}

# $AI_POLICY_TIER wins, then the CLAUDE.md marker, then the origin owner, then a
# fork probe. A repo with no remote at all is private.
resolve_tier() {
    local dir="${1:-$PWD}" root owner fork
    AI_POLICY_TIER_RESOLVED=""
    AI_POLICY_TIER_WHY=""

    if [ -n "${AI_POLICY_TIER:-}" ]; then
        AI_POLICY_TIER_RESOLVED="$AI_POLICY_TIER"
        AI_POLICY_TIER_WHY="AI_POLICY_TIER=$AI_POLICY_TIER in the environment"
        return 0
    fi

    root=$(_tier_repo_root "$dir")
    if [ -n "$root" ]; then
        if AI_POLICY_TIER_RESOLVED=$(_tier_marker "$root"); then
            AI_POLICY_TIER_WHY="ai-policy: marker in $root/CLAUDE.md"
            return 0
        fi

        if owner=$(_tier_origin_owner "$root"); then
            if [ "$owner" != "julianbieber" ]; then
                AI_POLICY_TIER_RESOLVED="upstream"
                AI_POLICY_TIER_WHY="origin belongs to $owner, not me"
                return 0
            fi
            if [ -z "${HOBBY_AI_POLICY_NO_GH:-}" ] && command -v gh >/dev/null 2>&1; then
                fork=$(gh repo view "$(_tier_origin_slug "$root")" --json isFork -q .isFork 2>/dev/null)
                if [ "$fork" = "true" ]; then
                    AI_POLICY_TIER_RESOLVED="upstream"
                    AI_POLICY_TIER_WHY="origin is my fork of someone else's repo"
                    return 0
                fi
            fi
            AI_POLICY_TIER_RESOLVED="private"
            AI_POLICY_TIER_WHY="origin is mine and is not a fork"
            return 0
        fi

        AI_POLICY_TIER_RESOLVED="private"
        AI_POLICY_TIER_WHY="no github/codeberg origin"
        return 0
    fi

    AI_POLICY_TIER_RESOLVED="private"
    AI_POLICY_TIER_WHY="not a git repo"
    return 0
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    DIR="${1:-$PWD}"
    case "${1:-}" in --why) DIR="$PWD" ;; esac
    resolve_tier "$DIR"
    printf '%s\n' "$AI_POLICY_TIER_RESOLVED"
    case " $* " in *" --why "*) printf '  %s\n' "$AI_POLICY_TIER_WHY" >&2 ;; esac
fi
