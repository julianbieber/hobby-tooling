#!/usr/bin/env bash
# Create and show the machine-local paths file.
#
# Some paths this install needs are genuinely per-machine — and one of them, the
# tree-sitter-mml checkout, sits under the work tree, which this repo is not
# allowed to record. Both problems have the same answer: keep those paths in a
# file outside the repo, and let discovery handle the rest.
#
# Everything in here is optional. A machine where discovery already works needs no
# file at all; `just doctor` says which values were resolved and how.
#
# Usage: local-paths.sh [--edit]

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./helix-paths.sh
source "$HERE/helix-paths.sh"

if [ ! -f "$HOBBY_PATHS" ]; then
    mkdir -p "$(dirname "$HOBBY_PATHS")"
    cat > "$HOBBY_PATHS" <<'ENV'
# Machine-local paths for hobby-tooling. Sourced as shell by scripts/helix-paths.sh.
# Never committed — this is the one place allowed to name paths outside ~/fun_repos.
#
# Uncomment and set only what discovery gets wrong on this machine.

# The Helix checkout holding languages.toml. Discovery already tries $HELIX_REPO,
# the ~/.config/helix/runtime symlink, and ~/{fun_repos,outside_repos,projects,
# src,repos}/helix — so this is usually unnecessary.
# HELIX_REPO="$HOME/fun_repos/helix"

# The tree-sitter-mml grammar checkout. Only machines with the work tooling tree
# have one; without it `.hmml` files keep the language server and lose only syntax
# highlighting. Set it here rather than anywhere inside the repo.
# TREE_SITTER_MML="$HOME/path/to/tree-sitter-mml"
ENV
    echo "  wrote template $HOBBY_PATHS"
else
    echo "  $HOBBY_PATHS already exists"
fi

if [ "${1:-}" = "--edit" ]; then
    exec "${EDITOR:-hx}" "$HOBBY_PATHS"
fi

echo ""
echo "  Resolved on this machine:"
if resolve_repo; then
    printf '    HELIX_REPO       %s  (via %s)\n' "$HELIX_REPO_RESOLVED" "$HELIX_REPO_WHY"
else
    printf '    HELIX_REPO       <none found>\n'
fi
if resolve_grammar_mml; then
    printf '    TREE_SITTER_MML  %s\n' "$MML_GRAMMAR_RESOLVED"
else
    printf '    TREE_SITTER_MML  <none — .hmml keeps the LSP, loses highlighting>\n'
fi
