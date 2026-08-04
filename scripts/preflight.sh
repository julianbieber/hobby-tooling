#!/usr/bin/env bash
# Check what this machine has before installing. Fails only on genuinely required
# tools; everything else degrades to a named missing feature rather than a broken
# install.
#
# Usage: preflight.sh

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./helix-paths.sh
source "$HERE/helix-paths.sh"

miss_required=0

req() {
    if command -v "$1" >/dev/null 2>&1; then
        printf '  \033[32m✓\033[0m %-12s %s\n' "$1" "$2"
    else
        printf '  \033[31m✗\033[0m %-12s %s  \033[31mREQUIRED\033[0m\n' "$1" "$2"
        miss_required=1
    fi
}

opt() {
    if command -v "$1" >/dev/null 2>&1; then
        printf '  \033[32m✓\033[0m %-12s %s\n' "$1" "$2"
    else
        printf '  \033[33m-\033[0m %-12s %s  \033[33m(missing: %s)\033[0m\n' "$1" "$2" "$3"
    fi
}

echo "Required:"
req git    "clone + worktrees"
req nu     "hwt / hcdev are nushell modules"
req jaq    "hooks parse their JSON input with it"
req claude "the thing we are configuring"

echo ""
echo "Optional:"
opt zellij    "hcdev dev sessions"          "hcdev cannot open sessions"
opt hx        "the editor in the layout"    "layout's editor pane is empty"
opt alacritty "hdash launch actions"        "hdash buttons do nothing"
opt gh        "issue skills + hdash"        "/pr-create, /gh-issue-*, hdash issues fail"
opt cargo     "builds the Rust extras"      "no dashboard, no MML tooling"
opt zk        "note search"                 "/zk-notes fails"

echo ""
echo "Discovered paths (override in ~/.config/hobby-tooling/paths.env — \`just local-paths\`):"
if resolve_repo; then
    printf '  \033[32m✓\033[0m %-26s %s \033[2m(via %s)\033[0m\n' \
        "helix checkout" "$HELIX_REPO_RESOLVED" "$HELIX_REPO_WHY"
else
    printf '  \033[33m-\033[0m %-26s \033[33mnot found — Helix wiring skipped\033[0m\n' "helix checkout"
fi
if resolve_grammar_mml; then
    printf '  \033[32m✓\033[0m %-26s %s\n' "tree-sitter-mml" "$MML_GRAMMAR_RESOLVED"
else
    printf '  \033[33m-\033[0m %-26s \033[33mnot found — .hmml keeps the LSP, loses highlighting\033[0m\n' \
        "tree-sitter-mml"
fi

echo ""
echo "Work-repo dependencies (hobby-only machines can skip these):"
for b in lsp-mcp julian-tree-extract-mcp just_mcp; do
    if command -v "$b" >/dev/null 2>&1; then
        printf '  \033[32m✓\033[0m %-26s MCP server\n' "$b"
    else
        printf '  \033[33m-\033[0m %-26s \033[33mnot on PATH — that MCP is skipped\033[0m\n' "$b"
    fi
done

echo ""
if [ "$miss_required" -ne 0 ]; then
    echo "Missing a required tool. Install it and re-run." >&2
    exit 1
fi
echo "Preflight OK."
