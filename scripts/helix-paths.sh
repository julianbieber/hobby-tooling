#!/usr/bin/env bash
# Locate the machine-specific Helix bits. One resolver, used by wire-helix.sh,
# doctor.sh, preflight.sh and the justfile, so no two of them can disagree about
# where the checkout is.
#
# The checkout path is genuinely different per machine — ~/fun_repos/helix on one,
# ~/outside_repos/helix on another — so hardcoding it guarantees a broken install
# somewhere. Discovery beats a default.
#
# Usage:
#   helix-paths.sh repo          absolute path to the Helix checkout   (exit 1 if none)
#   helix-paths.sh repo --why    same, plus how it was found, on stderr
#   helix-paths.sh grammar-mml   absolute path to a tree-sitter-mml checkout (exit 1 if none)
#
# Sourcing it instead gives the two resolvers as functions. They set variables
# rather than printing, because a caller that captured them with $(...) would get
# the path but lose $HELIX_REPO_WHY to the subshell:
#
#   resolve_repo         -> $HELIX_REPO_RESOLVED, $HELIX_REPO_WHY
#   resolve_grammar_mml  -> $MML_GRAMMAR_RESOLVED
#
# Overrides: HELIX_REPO, TREE_SITTER_MML — from the environment, or from the
# machine-local paths file below, which is where a path that must not be committed
# (anything under the work checkout) belongs. `just local-paths` writes a template.

set -uo pipefail

HOBBY_PATHS="${HOBBY_TOOLING_PATHS:-${XDG_CONFIG_HOME:-$HOME/.config}/hobby-tooling/paths.env}"
# The environment still wins: only unset names are filled in from the file.
if [ -f "$HOBBY_PATHS" ]; then
    _env_helix_repo="${HELIX_REPO:-}"
    _env_tree_sitter_mml="${TREE_SITTER_MML:-}"
    # shellcheck disable=SC1090
    . "$HOBBY_PATHS"
    [ -n "$_env_helix_repo" ] && HELIX_REPO="$_env_helix_repo"
    [ -n "$_env_tree_sitter_mml" ] && TREE_SITTER_MML="$_env_tree_sitter_mml"
    unset _env_helix_repo _env_tree_sitter_mml
fi

HELIX_CFG="${XDG_CONFIG_HOME:-$HOME/.config}/helix"
HELIX_REPO_RESOLVED=""
HELIX_REPO_WHY=""
MML_GRAMMAR_RESOLVED=""

# A directory only counts as the checkout if it has the file we install from.
_is_helix_repo() { [ -f "$1/languages.toml" ] && [ -d "$1/helix-term" ]; }

# Resolve the checkout. Ordered most-explicit first.
#
# The runtime probe is what makes an unconfigured machine work: ~/.config/helix/runtime
# is already a symlink into the checkout (Helix needs it for queries and grammars), so
# the answer is on disk before this repo is ever installed.
resolve_repo() {
    local cand
    HELIX_REPO_RESOLVED=""
    HELIX_REPO_WHY=""

    if [ -n "${HELIX_REPO:-}" ]; then
        if _is_helix_repo "$HELIX_REPO"; then
            HELIX_REPO_RESOLVED="$HELIX_REPO"
            HELIX_REPO_WHY='$HELIX_REPO'
            return 0
        fi
        printf 'HELIX_REPO=%s is set but has no languages.toml/helix-term — ignoring it\n' \
            "$HELIX_REPO" >&2
    fi

    if [ -L "$HELIX_CFG/runtime" ]; then
        cand=$(readlink -f "$HELIX_CFG/runtime" 2>/dev/null)
        cand=$(dirname "${cand%/}")          # .../helix/runtime -> .../helix
        if _is_helix_repo "$cand"; then
            HELIX_REPO_RESOLVED="$cand"
            HELIX_REPO_WHY="the $HELIX_CFG/runtime symlink"
            return 0
        fi
    fi

    for cand in "$HOME/fun_repos/helix" "$HOME/outside_repos/helix" \
                "$HOME/projects/helix" "$HOME/src/helix" "$HOME/repos/helix"; do
        if _is_helix_repo "$cand"; then
            HELIX_REPO_RESOLVED="$cand"
            HELIX_REPO_WHY="a well-known location"
            return 0
        fi
    done

    return 1
}

# tree-sitter-mml is not hobby code — it is built out of the work tooling checkout,
# so a hobby-only machine legitimately has none, and the path on a machine that does
# have it is not something this repo may record. Hence: TREE_SITTER_MML in the
# machine-local paths file, plus a couple of hobby-side locations for a local clone.
#
# Callers must treat "not found" as a normal outcome, never an error.
resolve_grammar_mml() {
    local cand
    MML_GRAMMAR_RESOLVED=""
    if [ -n "${TREE_SITTER_MML:-}" ]; then
        if [ -d "$TREE_SITTER_MML/src" ] || [ -f "$TREE_SITTER_MML/grammar.js" ]; then
            MML_GRAMMAR_RESOLVED="$TREE_SITTER_MML"
            return 0
        fi
        printf 'TREE_SITTER_MML=%s does not look like a grammar checkout — ignoring it\n' \
            "$TREE_SITTER_MML" >&2
    fi
    for cand in "$HOME/fun_repos/tree-sitter-mml" "$HOME/outside_repos/tree-sitter-mml"; do
        if [ -d "$cand/src" ] || [ -f "$cand/grammar.js" ]; then
            MML_GRAMMAR_RESOLVED="$cand"
            return 0
        fi
    done
    return 1
}

# Only run the CLI when executed, so the functions can also be sourced.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    case "${1:-repo}" in
        repo)
            resolve_repo || exit 1
            printf '%s\n' "$HELIX_REPO_RESOLVED"
            # An `if`, not `[ ... ] && printf`: the latter is the branch's last
            # command, so a caller without --why got its false test as the exit
            # status — the path on stdout and a failure code with it.
            if [ "${2:-}" = "--why" ]; then
                printf 'via %s\n' "$HELIX_REPO_WHY" >&2
            fi
            ;;
        grammar-mml)
            resolve_grammar_mml || exit 1
            printf '%s\n' "$MML_GRAMMAR_RESOLVED"
            ;;
        *) echo "usage: helix-paths.sh [repo [--why] | grammar-mml]" >&2; exit 2 ;;
    esac
fi
