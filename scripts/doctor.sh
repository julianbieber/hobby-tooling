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
if command -v hauto >/dev/null 2>&1; then ok "hauto on PATH"; else warn "hauto not on PATH (hcdev auto needs it)" "just install-chobby"; fi

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
for f in "config.kdl" "layouts/hobby-dev.kdl" "layouts/hobby-auto.kdl"; do
    name=$(basename "$f")
    if [ -h "$ZJ/$f" ]; then
        ok "$name linked"
    elif [ -e "$ZJ/$f" ]; then
        bad "$name is a real file, not a link" "just install-shell"
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
if command -v hcoord >/dev/null 2>&1; then ok "hcoord built"; else warn "hcoord not built (hcdev coordinator needs it)" "just install-coord"; fi
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
echo ""
echo "Helix"
if resolve_repo; then
    REPO="$HELIX_REPO_RESOLVED"
    ok "checkout $REPO (via $HELIX_REPO_WHY)"

    if grep -q '^name = "hstruct"$' "$REPO/languages.toml"; then
        ok "hstruct language in the checkout"
    else
        warn "hstruct not registered" "just wire-helix"
    fi

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

# hmml owns its grammar, so this one is about the WORK `.mml` dialect only.
if resolve_grammar_mml; then
    ok "tree-sitter-mml at $MML_GRAMMAR_RESOLVED"
else
    warn "no tree-sitter-mml — work .mml loses highlighting" "just local-paths"
fi
HSTRUCT_GRAMMAR_DIR="$MAIN_CHECKOUT/lsps/tree-sitter-hstruct"
if [ -d "$HSTRUCT_GRAMMAR_DIR" ]; then
    ok "lsps/tree-sitter-hstruct present"
else
    bad "lsps/tree-sitter-hstruct missing" "git pull"
fi

HMML_GRAMMAR_DIR="$MAIN_CHECKOUT/lsps/tree-sitter-hmml"
if [ -f "$HMML_GRAMMAR_DIR/src/parser.c" ]; then
    # Built by `hx --grammar build` into the checkout's runtime; without the .so
    # the language still works, it just renders unhighlighted.
    if resolve_repo && [ -f "$HELIX_REPO_RESOLVED/runtime/grammars/hmml.so" ]; then
        ok "hmml grammar built"
    else
        warn "hmml grammar not built — .hmml renders plain" "hx --grammar build"
    fi
else
    bad "lsps/tree-sitter-hmml missing" "git pull"
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
