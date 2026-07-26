#!/usr/bin/env bash
# Report what is and isn't wired up, and what to run to fix each gap.
#
# Usage: doctor.sh

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./helix-paths.sh
source "$HERE/helix-paths.sh"

CFG="$HOME/.claude-hobby"
NU="${XDG_CONFIG_HOME:-$HOME/.config}/nushell"
HX="$HELIX_CFG"
NOTES="$HOME/fun_repos/hobby-mimisbrunnr"
HELIX_BRANCH="${HELIX_BRANCH:-custom}"

gaps=0

ok()   { printf '  \033[32m✓\033[0m %s\n' "$1"; }
bad()  { printf '  \033[31m✗\033[0m %-46s \033[2m%s\033[0m\n' "$1" "$2"; gaps=$((gaps + 1)); }
warn() { printf '  \033[33m-\033[0m %-46s \033[2m%s\033[0m\n' "$1" "$2"; }

echo "Account isolation"
[ -d "$CFG" ] && ok "config dir $CFG" || bad "config dir missing" "just bootstrap-config"
[ -e "$CFG/settings.json" ] && ok "settings.json linked" || bad "settings.json not linked" "just bootstrap-config"
[ -e "$CFG/CLAUDE.md" ] && ok "account CLAUDE.md linked" || bad "account CLAUDE.md not linked" "just bootstrap-config"
if [ -f "$CFG/.credentials.json" ]; then
    ok "logged in to the private account"
else
    bad "not logged in" "chobby   then  /login"
fi
if command -v chobby >/dev/null 2>&1; then ok "chobby on PATH"; else bad "chobby not on PATH" "just install-chobby"; fi

echo ""
echo "Skills and hooks"
n_sk=$(ls "$CFG"/commands/*.md 2>/dev/null | wc -l)
[ "$n_sk" -gt 0 ] && ok "$n_sk skills installed" || bad "no skills installed" "just install-skills"
n_hk=$(ls "$CFG"/hooks/*.sh 2>/dev/null | wc -l)
[ "$n_hk" -eq 5 ] && ok "5 hooks installed" || bad "$n_hk/5 hooks installed" "just install-hooks"

echo ""
echo "Shell"
[ -e "$NU/hwt.nu" ]   && ok "hwt.nu linked"   || bad "hwt.nu not linked"   "just install-shell"
[ -e "$NU/hcdev.nu" ] && ok "hcdev.nu linked" || bad "hcdev.nu not linked" "just install-shell"
if grep -qE '^[[:space:]]*use[[:space:]]+hwt\.nu' "$NU/config.nu" 2>/dev/null; then
    ok "config.nu loads the hobby modules"
else
    bad "config.nu does not load them" "just wire-nushell"
fi
[ -e "${XDG_CONFIG_HOME:-$HOME/.config}/zellij/layouts/hobby-dev.kdl" ] \
    && ok "hobby-dev.kdl layout linked" || bad "layout not linked" "just install-shell"

echo ""
echo "Repos"
[ -d "$NOTES/.git" ] && ok "notes repo at $NOTES" || bad "notes repo missing" "just clone-notes"
# Resolve the main checkout: from inside a worktree, git-common-dir points at the
# main repo's .git, so its parent is the checkout whose identity actually matters.
if common=$(git rev-parse --git-common-dir 2>/dev/null); then
    case "$common" in /*) ;; *) common="$PWD/$common" ;; esac
    MAIN_CHECKOUT=$(cd "$(dirname "$common")" && pwd)
else
    MAIN_CHECKOUT=$PWD
fi
for r in "$MAIN_CHECKOUT" "$NOTES"; do
    [ -d "$r/.git" ] || continue
    e=$(git -C "$r" config user.email 2>/dev/null || true)
    if [ "$e" = "julian.bieber.1992@gmail.com" ]; then
        ok "$(basename "$r") commits as $e"
    else
        bad "$(basename "$r") commits as ${e:-<unset>}" "just git-identity"
    fi
done
[ -d "$HOME/fun_worktrees" ] && ok "~/fun_worktrees exists" || bad "~/fun_worktrees missing" "just clone-notes"

echo ""
echo "Optional extras"
if command -v hwt-overview >/dev/null 2>&1; then ok "hwt-overview built"; else warn "hwt-overview not built" "just install-overview"; fi
for b in hobby-mml-lsp hobby-mml-parser hobby-mml-index; do
    if command -v "$b" >/dev/null 2>&1; then ok "$b built"; else warn "$b not built" "just install-mml"; fi
done
echo ""
echo "Helix"
if resolve_repo; then
    REPO="$HELIX_REPO_RESOLVED"
    ok "checkout $REPO (via $HELIX_REPO_WHY)"

    if grep -q '^name = "hmml"$' "$REPO/languages.toml"; then
        ok "hmml language in the checkout"
    else
        warn "hmml not registered" "just wire-helix"
    fi

    # The shared file must stay machine-neutral: an absolute grammar source.path
    # committed there breaks `cargo install --path helix-term` on every machine that
    # does not have that exact directory, because build.rs is STRICT.
    if grep -A1 '^\[\[grammar\]\]$' "$REPO/languages.toml" | grep -q '^name = "mml"$'; then
        bad "machine-specific mml grammar in the shared file" "just wire-helix"
    else
        ok "shared languages.toml is machine-neutral"
    fi

    # The pre-.hmml entry claimed `.mml` by path glob; left in place it fights
    # the work `mml` language for every work spec.
    if grep -q '^name = "mml-hobby"$' "$REPO/languages.toml"; then
        bad "legacy mml-hobby language still present" "just wire-helix"
    fi

    # ~/.config/helix/languages.toml is generated: checkout + local overlay. Compare
    # against a fresh concatenation rather than against the checkout alone.
    LOCAL_OVERLAY="$HX/languages.local.toml"
    if [ -L "$HX/languages.toml" ]; then
        warn "installed languages.toml is a symlink" "rm it, then just wire-helix"
    elif [ ! -f "$HX/languages.toml" ]; then
        bad "no installed languages.toml" "just wire-helix"
    else
        want=$(mktemp)
        { cat "$REPO/languages.toml"; printf '\n'; cat "$LOCAL_OVERLAY" 2>/dev/null; } > "$want"
        if cmp -s "$want" "$HX/languages.toml"; then
            ok "installed languages.toml matches checkout + local overlay"
        else
            bad "installed languages.toml is stale" "just wire-helix"
        fi
        rm -f "$want"
    fi

    if [ -L "$HX/runtime" ]; then
        if [ "$(readlink -f "$HX/runtime")" = "$(readlink -f "$REPO/runtime")" ]; then
            ok "runtime symlink points at the checkout"
        else
            bad "runtime symlink points elsewhere" "just wire-helix"
        fi
    else
        warn "runtime is not a symlink into the checkout" "just wire-helix"
    fi

    # wire-helix writes into the checkout, so the wrong branch means the hmml entry
    # is committed somewhere it will not survive a rebuild.
    br=$(git -C "$REPO" branch --show-current 2>/dev/null || true)
    if [ -n "$br" ] && [ "$br" != "$HELIX_BRANCH" ]; then
        warn "on branch $br, not $HELIX_BRANCH" "git -C $REPO switch $HELIX_BRANCH"
    fi
else
    warn "no Helix checkout found" "just local-paths, set HELIX_REPO"
fi

if resolve_grammar_mml; then
    ok "tree-sitter-mml at $MML_GRAMMAR_RESOLVED"
else
    warn "no tree-sitter-mml — .hmml has no highlighting" "just local-paths"
fi

# Derived, so it cannot go stale the way a hand-written table does.
if [ -f "$HX/languages.toml" ]; then
    read -r _t _m mp < <("$HERE/helix-servers.sh" --count 2>/dev/null || echo "0 0 0")
    if [ "${mp:-0}" -eq 0 ]; then
        ok "every personal language server is on PATH"
    else
        warn "$mp personal language server(s) missing" "just helix-servers"
    fi
fi
echo ""
echo "MCP servers"
n_mcp=$(CLAUDE_CONFIG_DIR="$CFG" claude mcp list 2>/dev/null | grep -c 'Connected' || true)
[ "${n_mcp:-0}" -gt 0 ] && ok "$n_mcp connected" || warn "none connected" "just install-mcps"

echo ""
if [ "$gaps" -eq 0 ]; then
    printf '\033[32mNo gaps.\033[0m Optional items marked - are yours to opt into.\n'
else
    printf '\033[31m%d gap(s).\033[0m Run the commands shown, or `just bootstrap` for all of them.\n' "$gaps"
fi
