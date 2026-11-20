#!/usr/bin/env bash
# Report what is and isn't wired up, and what to run to fix each gap.
#
# Usage: doctor.sh

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CFG="$HOME/.claude-hobby"
NU="${XDG_CONFIG_HOME:-$HOME/.config}/nushell"
NOTES="$HOME/fun_repos/hobby-mimisbrunnr"

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
if command -v hauto >/dev/null 2>&1; then ok "hauto on PATH"; else warn "hauto not on PATH (hcdev auto needs it)" "just install-chobby"; fi
if command -v haspect >/dev/null 2>&1; then ok "haspect on PATH"; else warn "haspect not on PATH (hcdev tab needs it)" "just install-chobby"; fi
if command -v hresearch >/dev/null 2>&1; then ok "hresearch on PATH"; else warn "hresearch not on PATH (/research needs it)" "just install-chobby"; fi

echo ""
echo "Skills and hooks"
want_sk=0
unlinked_sk=()
while IFS= read -r src; do
    want_sk=$((want_sk + 1))
    rel=${src%/skill.md}; rel=${rel##*/skills/}
    link="$CFG/commands/$rel.md"
    if [ ! -h "$link" ] || [ "$(readlink -f "$link")" != "$(readlink -f "$src")" ]; then
        unlinked_sk+=("/${rel//\//:}")
    fi
done < <(find "$HERE/../skills" -mindepth 2 -maxdepth 3 -name skill.md 2>/dev/null)
if [ "${#unlinked_sk[@]}" -eq 0 ]; then
    ok "$want_sk skills linked"
else
    bad "$((want_sk - ${#unlinked_sk[@]}))/$want_sk skills linked" "just install-skills — ${unlinked_sk[*]}"
fi
n_stale_sk=$(find "$CFG/commands" -mindepth 1 -maxdepth 2 -xtype l 2>/dev/null | wc -l)
[ "$n_stale_sk" -eq 0 ] || bad "$n_stale_sk skill links point at nothing" "just install-skills"
n_hk=$(ls "$CFG"/hooks/*.sh 2>/dev/null | wc -l)
want_hk=$(find "$HERE/../hooks" -mindepth 2 -maxdepth 2 -name hook.sh 2>/dev/null | wc -l)
[ "$n_hk" -eq "$want_hk" ] && ok "$want_hk hooks installed" || bad "$n_hk/$want_hk hooks installed" "just install-hooks"
# Named separately from the count because settings.json registers this one on
# PostToolUse with an empty matcher: unlinked, it fails after EVERY tool call, and
# a bare count would report the gap as "5/6" without saying which hook is loud.
if [ -e "$CFG/hooks/claude-conversation.sh" ]; then
    ok "conversation dump hook linked"
else
    bad "conversation dump hook missing" "just install-hooks"
fi
if [ -e "$CFG/hooks/session-state.sh" ] && grep -q 'hooks/session-state.sh' "$CFG/settings.json" 2>/dev/null; then
    ok "session-state hook linked and registered (hdash busy/idle badges)"
else
    bad "session-state hook not linked or not in settings.json" "just install-hooks bootstrap-config"
fi
if command -v hx-claude >/dev/null 2>&1; then
    # Resolving to a copy outside this checkout is how this broke in the first
    # place: ~/.local/bin pointed into a work *worktree*, one `wt rm` from gone.
    if [ "$(readlink -f "$(command -v hx-claude)")" = "$(readlink -f "$HERE/../bin/hx-claude")" ]; then
        ok "hx-claude on PATH, from this checkout"
    else
        warn "hx-claude resolves outside this checkout" "just install-conversation"
    fi
else
    bad "hx-claude not on PATH" "just install-conversation"
fi
# ctrl-g opens $EDITOR, so a dump nothing points at is still an invisible failure.
# A warn, not a gap: the shell profile is outside this repo's control.
if [ "${EDITOR:-}" = "hx-claude" ]; then
    ok "EDITOR=hx-claude (ctrl-g shows the conversation)"
else
    warn "EDITOR is ${EDITOR:-<unset>}, not hx-claude" "set it in ~/.config/nushell/env.nu"
fi

echo ""
echo "Shell"
[ -h "$NU/hwt.nu" ]   && ok "hwt.nu linked"   || bad "hwt.nu not linked"   "just install-shell"
[ -h "$NU/hcdev.nu" ] && ok "hcdev.nu linked" || bad "hcdev.nu not linked" "just install-shell"
if grep -qE '^[[:space:]]*use[[:space:]]+hwt\.nu' "$NU/config.nu" 2>/dev/null; then
    ok "config.nu loads the hobby modules"
else
    bad "config.nu does not load them" "just wire-nushell"
fi
# -h before -e, deliberately. `-e` alone is true for a regular file some *other*
# repo installed, which is how ~/.config/zellij/config.kdl went unnoticed: present,
# working, and owned by the home dotfiles repo. A real file here is a gap, not a pass.
ZJ="${XDG_CONFIG_HOME:-$HOME/.config}/zellij"
for f in "config.kdl" "layouts/hobby-dev.kdl" "layouts/hobby-claude.kdl" "layouts/hobby-auto.kdl"; do
    name=$(basename "$f")
    if [ -h "$ZJ/$f" ]; then
        ok "$name linked"
    elif [ -e "$ZJ/$f" ]; then
        bad "$name is a real file, not a link" "just install-shell"
    else
        bad "$name not linked" "just install-shell"
    fi
done
for pair in "bin/zj-notes:$HOME/.local/bin/zj-notes" \
            "dotfiles/zellij/layouts/notes-task.kdl:$ZJ/layouts/notes-task.kdl"; do
    src="$HERE/../${pair%%:*}"
    dst="${pair#*:}"
    name=$(basename "$dst")
    if [ -h "$dst" ] && [ "$(readlink -f "$dst")" = "$(readlink -f "$src")" ]; then
        ok "$name linked, from this checkout"
    elif [ -e "$dst" ] || [ -h "$dst" ]; then
        bad "$name resolves outside this checkout" "just install-shell"
    else
        bad "$name not linked" "just install-shell"
    fi
done

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
# Every repo, because the global identity is the work one and a repo that never
# overrode it commits as work — the silent case, which looks fine until the commits
# are already pushed. `git config user.email` reports the *resolved* value, so a
# repo covered only by the includeIf passes without needing local config.
wrong=0
for r in "$MAIN_CHECKOUT" "$NOTES" "$HOME/fun_repos"/*/; do
    [ -e "$r/.git" ] || continue
    e=$(git -C "$r" config user.email 2>/dev/null || true)
    [ "$e" = "julian.bieber.1992@gmail.com" ] || wrong=$((wrong + 1))
done
if [ "$wrong" -eq 0 ]; then
    ok "every repo under ~/fun_repos commits as julian.bieber.1992@gmail.com"
else
    bad "$wrong repo(s) commit as someone else" "just git-identity"
fi
[ -d "$HOME/fun_worktrees" ] && ok "~/fun_worktrees exists" || bad "~/fun_worktrees missing" "just clone-notes"

echo ""
echo "Optional extras"
if command -v hdash >/dev/null 2>&1; then ok "hdash built"; else warn "hdash not built" "just install-dash"; fi
if command -v htab >/dev/null 2>&1; then ok "htab built"; else warn "htab not built (the Claude-mode overview pane)" "just install-dash"; fi
if command -v hcoord >/dev/null 2>&1; then ok "hcoord built"; else warn "hcoord not built (hcdev coordinator needs it)" "just install-coord"; fi
if command -v hatlas >/dev/null 2>&1; then ok "hatlas built"; else warn "hatlas not built" "just install-atlas"; fi
# The two dashboards hdash replaces. Reported so a machine that installed them before the
# merge is told to reclaim the names, rather than silently keeping two stale binaries.
for b in hwt-overview hgh-issues; do
    if command -v "$b" >/dev/null 2>&1; then
        warn "$b is retired but still on PATH" "cargo uninstall hwt_app issues_app"
    fi
done
for b in hobby-mml-lsp hobby-mml-parser; do
    if command -v "$b" >/dev/null 2>&1; then ok "$b built"; else warn "$b not built" "just install-mml"; fi
done

for b in hobby-struct-lsp hobby-struct-parser; do
    if command -v "$b" >/dev/null 2>&1; then ok "$b built"; else warn "$b not built" "just install-struct"; fi
done
if command -v hobby-research-lsp >/dev/null 2>&1; then ok "hobby-research-lsp built"; else warn "hobby-research-lsp not built" "just install-research"; fi
echo ""
echo "Helix"
if command -v hx >/dev/null 2>&1; then
    ok "hx on PATH"
else
    warn "hx not on PATH" "just install, in the Helix checkout"
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
