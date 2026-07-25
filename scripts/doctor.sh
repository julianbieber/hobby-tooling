#!/usr/bin/env bash
# Report what is and isn't wired up, and what to run to fix each gap.
#
# Usage: doctor.sh

set -uo pipefail

CFG="$HOME/.claude-hobby"
NU="${XDG_CONFIG_HOME:-$HOME/.config}/nushell"
HX="${XDG_CONFIG_HOME:-$HOME/.config}/helix"
NOTES="$HOME/fun_repos/hobby-mimisbrunnr"
HELIX_REPO="${HELIX_REPO:-$HOME/outside_repos/helix}"

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
if [ -f "$HELIX_REPO/languages.toml" ]; then
    if grep -q '^name = "mml-hobby"$' "$HELIX_REPO/languages.toml"; then
        ok "Helix: mml-hobby in the repo's languages.toml"
        if cmp -s "$HELIX_REPO/languages.toml" "$HX/languages.toml"; then
            ok "Helix: installed copy matches the repo"
        else
            bad "Helix: installed copy is stale" "just wire-helix"
        fi
    else
        warn "Helix: mml-hobby not registered" "just wire-helix"
    fi
else
    warn "Helix: no checkout at $HELIX_REPO" "set HELIX_REPO, or skip"
fi
if [ -n "${CLAUDE_CONFIG_DIR:-}" ]; then
    n_mcp=$(CLAUDE_CONFIG_DIR="$CFG" claude mcp list 2>/dev/null | grep -c 'Connected' || true)
else
    n_mcp=$(CLAUDE_CONFIG_DIR="$CFG" claude mcp list 2>/dev/null | grep -c 'Connected' || true)
fi
[ "${n_mcp:-0}" -gt 0 ] && ok "$n_mcp MCP servers connected" || warn "no MCP servers" "just install-mcps"

echo ""
if [ "$gaps" -eq 0 ]; then
    printf '\033[32mNo gaps.\033[0m Optional items marked - are yours to opt into.\n'
else
    printf '\033[31m%d gap(s).\033[0m Run the commands shown, or `just bootstrap` for all of them.\n' "$gaps"
fi
