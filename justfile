# hobby-tooling — install the hobby Claude setup
#
# Fresh machine:  just bootstrap
# Existing one:   just doctor    then whatever it reports
#
# Every recipe is idempotent and independently re-runnable. `ln -sf` replaces stale
# symlinks but NOT regular files — if a target already exists as a real file, remove
# it first.

# The MAIN checkout, even when this justfile is invoked from a worktree.
#
# Every install-* recipe symlinks into ~/.claude-hobby and ~/.config. Those links
# must point at the main checkout: a worktree is deleted by `hwt rm`, which would
# leave the whole installation dangling — 18 skills, 5 hooks, chobby, the lot.
# `git rev-parse --git-common-dir` reports the main repo's .git from inside any
# linked worktree, so its parent is the main checkout. Falls back to the invocation
# directory when this isn't a git repo at all (e.g. an extracted tarball).
REPO_ROOT := ```
    if root=$(git rev-parse --git-common-dir 2>/dev/null); then
        case "$root" in /*) ;; *) root="$PWD/$root" ;; esac
        cd "$(dirname "$root")" && pwd
    else
        pwd
    fi
    ```

CONFIG_DIR := env('HOME') / '.claude-hobby'
NU_DIR     := env('HOME') / '.config/nushell'
ZJ_LAYOUTS := env('HOME') / '.config/zellij/layouts'
BIN_DIR    := env('HOME') / '.local/bin'
FUN_REPOS  := env('HOME') / 'fun_repos'
NOTES_DIR  := env('HOME') / 'fun_repos/hobby-mimisbrunnr'
HELIX_REPO := env('HELIX_REPO', env('HOME') / 'outside_repos/helix')
# Overridable so a fork, a rename, or a local clone can be used instead.
NOTES_REMOTE := env('HOBBY_NOTES_REMOTE', 'git@github.com:julianbieber/hobby-mimisbrunnr.git')
GIT_NAME   := 'julianbieber'
GIT_EMAIL  := 'julian.bieber.1992@gmail.com'

# List all recipes
default:
    @just --list

# ---------------------------------------------------------------------------
# Fresh-machine entry point
# ---------------------------------------------------------------------------

# Everything a new machine needs, in dependency order. Safe to re-run.
bootstrap: preflight clone-notes git-identity bootstrap-config install-chobby install-shell wire-nushell install-hooks install-skills install-claude-md wire-helix
    @echo ""
    @echo "════════════════════════════════════════════════════════════════"
    @echo "Core install done. One manual step remains — the login is"
    @echo "interactive, so it cannot be scripted:"
    @echo ""
    @echo "    chobby          then  /login   with your private account"
    @echo ""
    @echo "Then, optionally:"
    @echo "    just install-mcps      MCP servers (needs the work repo's binaries)"
    @echo "    just install-overview  the hwt-overview dashboard (Rust build)"
    @echo "    just install-mml       forked HMML language server (Rust build)"
    @echo ""
    @echo "Check the result any time with:  just doctor"
    @echo "════════════════════════════════════════════════════════════════"

# Bootstrap plus every optional Rust build and the MCP servers.
bootstrap-full: bootstrap install-mcps install-overview install-mml
    @echo "Full install done. Still need: chobby, then /login."

# Say so when invoked from a worktree — the install still targets the main checkout,
# and silently doing that would be worse than mentioning it.
[private]
_root-notice:
    #!/usr/bin/env bash
    if [ "$PWD" != "{{REPO_ROOT}}" ]; then
        echo "  note: invoked from $PWD"
        echo "        installing from the main checkout {{REPO_ROOT}} — symlinks must not"
        echo "        point into a worktree, since \`hwt rm\` would delete it."
    fi

# Check this machine has what the install needs
preflight:
    @"{{REPO_ROOT}}"/scripts/preflight.sh

# ---------------------------------------------------------------------------
# Repos + identity
# ---------------------------------------------------------------------------

# Clone the notes repo beside this one if it isn't there yet
clone-notes:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -d "{{NOTES_DIR}}/.git" ]; then
        echo "  notes repo already at {{NOTES_DIR}}"
    else
        mkdir -p "{{FUN_REPOS}}"
        echo "  cloning {{NOTES_REMOTE}} into {{FUN_REPOS}}"
        git clone "{{NOTES_REMOTE}}" "{{NOTES_DIR}}"
    fi
    mkdir -p "{{NOTES_DIR}}/notes" "$HOME/fun_worktrees"

# The GLOBAL git identity is the work one, so without this hobby commits land under
# the work email — exactly the leak a separate account is meant to prevent.

# Set the personal git identity on both hobby repos
git-identity:
    #!/usr/bin/env bash
    set -euo pipefail
    for r in "{{REPO_ROOT}}" "{{NOTES_DIR}}"; do
        [ -d "$r/.git" ] || continue
        git -C "$r" config user.name  "{{GIT_NAME}}"
        git -C "$r" config user.email "{{GIT_EMAIL}}"
        echo "  $(basename "$r"): $(git -C "$r" config user.email)"
    done

# ---------------------------------------------------------------------------
# Claude config dir
# ---------------------------------------------------------------------------

# Create ~/.claude-hobby and link the settings + account CLAUDE.md into it
bootstrap-config: _root-notice
    @mkdir -p "{{CONFIG_DIR}}/commands" "{{CONFIG_DIR}}/hooks" "{{CONFIG_DIR}}/agents" "{{CONFIG_DIR}}/skills"
    @ln -sf "{{REPO_ROOT}}/settings/hobby.settings.json" "{{CONFIG_DIR}}/settings.json"
    @ln -sf "{{REPO_ROOT}}/dotfiles/claude-md/hobby.CLAUDE.md" "{{CONFIG_DIR}}/CLAUDE.md"
    @echo "  config dir ready: {{CONFIG_DIR}}"

# The `chobby` wrapper — bare claude on the hobby account
install-chobby: _root-notice
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{BIN_DIR}}"
    ln -sf "{{REPO_ROOT}}/bin/chobby" "{{BIN_DIR}}/chobby"
    echo "  linked chobby -> {{BIN_DIR}}/chobby"
    case ":$PATH:" in
        *":{{BIN_DIR}}:"*) ;;
        *) echo "  WARNING: {{BIN_DIR}} is not on PATH — add it or chobby won't resolve." ;;
    esac

# Hooks into the hobby config dir
install-hooks: _root-notice
    @mkdir -p "{{CONFIG_DIR}}/hooks"
    @for d in "{{REPO_ROOT}}"/hooks/*/; do \
        n=$(basename "$d"); \
        [ -f "$d/hook.sh" ] || continue; \
        chmod +x "$d/hook.sh"; \
        ln -sf "$d/hook.sh" "{{CONFIG_DIR}}/hooks/$n.sh"; \
        echo "  hook: $n"; \
    done

# Renaming a skill dir orphans its old symlink, which then lingers as a slash
# command pointing at nothing. `-xtype l` matches exactly the dangling links, so
# live commands are never touched.

# Skills as slash commands in the hobby config dir
install-skills: _root-notice
    @mkdir -p "{{CONFIG_DIR}}/commands"
    @find "{{CONFIG_DIR}}/commands" -maxdepth 1 -xtype l -print -delete \
        | sed 's|^.*/|  pruned stale skill: /|; s|\.md$||'
    @for d in "{{REPO_ROOT}}"/skills/*/; do \
        n=$(basename "$d"); \
        [ -f "$d/skill.md" ] || continue; \
        ln -sf "$d/skill.md" "{{CONFIG_DIR}}/commands/$n.md"; \
        echo "  skill: /$n"; \
    done

# CLAUDE.md for the worktree tree
install-claude-md: _root-notice
    @mkdir -p "$HOME/fun_worktrees"
    @ln -sf "{{REPO_ROOT}}/dotfiles/claude-md/fun_worktrees.CLAUDE.md" "$HOME/fun_worktrees/CLAUDE.md"
    @echo "  linked fun_worktrees.CLAUDE.md"
    @echo "  (~/CLAUDE.md stays the WORK file; the hobby account reads {{CONFIG_DIR}}/CLAUDE.md)"

# ---------------------------------------------------------------------------
# Shell + editor wiring
# ---------------------------------------------------------------------------

# Symlink the nushell modules + zellij layout
install-shell: _root-notice
    @mkdir -p "{{NU_DIR}}" "{{ZJ_LAYOUTS}}"
    @ln -sf "{{REPO_ROOT}}/dotfiles/nushell/hwt.nu"   "{{NU_DIR}}/hwt.nu"
    @ln -sf "{{REPO_ROOT}}/dotfiles/nushell/hcdev.nu" "{{NU_DIR}}/hcdev.nu"
    @ln -sf "{{REPO_ROOT}}/dotfiles/zellij/layouts/hobby-dev.kdl" "{{ZJ_LAYOUTS}}/hobby-dev.kdl"
    @echo "  linked hwt.nu, hcdev.nu, hobby-dev.kdl"

# Add `use hwt.nu *` / `use hcdev.nu *` to config.nu (idempotent)
wire-nushell:
    @"{{REPO_ROOT}}"/scripts/wire-nushell.sh

# languages.toml lives in a local Helix checkout and is COPIED into ~/.config/helix,
# so the edit goes to the repo and is installed from there — editing the installed
# copy would be lost on the next refresh. Skips cleanly when no checkout is found.

# Register hobby-mml-lsp with Helix, via the local Helix checkout
wire-helix:
    @HELIX_REPO="{{HELIX_REPO}}" "{{REPO_ROOT}}"/scripts/wire-helix.sh

# ---------------------------------------------------------------------------
# Optional extras
# ---------------------------------------------------------------------------

# The binaries are built out of the work tooling repo, so a hobby-only machine won't
# have them; each is registered only if present, and the rest are reported.
#
# -s user is essential: the default `local` scope binds a server to the single
# directory it was added from, so hobby projects would see no MCPs at all.

# Register the generic MCP servers (skips any whose binary is absent)
install-mcps:
    #!/usr/bin/env bash
    set -uo pipefail
    add() {
        if command -v "$2" >/dev/null 2>&1; then
            CLAUDE_CONFIG_DIR="{{CONFIG_DIR}}" claude mcp add -s user "$1" -- "$2" >/dev/null 2>&1 \
                && echo "  added $1" || echo "  $1 already registered"
        else
            echo "  SKIP $1 — \`$2\` not on PATH (build it from the work tooling repo)"
        fi
    }
    add lsp-mcp      lsp-mcp
    add tree-extract julian-tree-extract-mcp
    add just-mcp     just_mcp
    echo ""
    CLAUDE_CONFIG_DIR="{{CONFIG_DIR}}" claude mcp list

# Build + install the hwt-overview dashboard
install-overview:
    cd tools/hwt-overview && cargo install --path crates/hwt_app

# The parser CLI matters: the hmml-* skills shell out to `hobby-mml-parser --lint`.

# Build + install the forked HMML language server, parser CLI, and index
install-mml:
    cd lsps/hobby-mml-lsp && cargo install --path mml-lsp
    cd lsps/hobby-mml-lsp && cargo install --path mml-parser
    cd tools/hobby-mml-index && cargo install --path .

# ---------------------------------------------------------------------------
# Diagnostics
# ---------------------------------------------------------------------------

# Report what is and isn't wired up on this machine
doctor:
    @cd "{{REPO_ROOT}}" && ./scripts/doctor.sh

# Run every test in the repo (HMML parser/LSP + the dashboard)
test:
    cd lsps/hobby-mml-lsp && cargo test --workspace
    cd tools/hwt-overview && cargo test --workspace

# ---------------------------------------------------------------------------
# Uninstall
# ---------------------------------------------------------------------------

# Leaves ~/.claude-hobby (and therefore the login) and both git repos untouched;
# delete those by hand if you really mean to.

# Remove the symlinks this repo installs
uninstall:
    #!/usr/bin/env bash
    set -euo pipefail
    rm -f "{{NU_DIR}}/hwt.nu" "{{NU_DIR}}/hcdev.nu" "{{ZJ_LAYOUTS}}/hobby-dev.kdl" "{{BIN_DIR}}/chobby"
    rm -f "$HOME/fun_worktrees/CLAUDE.md"
    rm -f "{{CONFIG_DIR}}/settings.json" "{{CONFIG_DIR}}/CLAUDE.md"
    rm -f "{{CONFIG_DIR}}"/commands/*.md "{{CONFIG_DIR}}"/hooks/*.sh
    echo "Unlinked. Still present on purpose:"
    echo "  {{CONFIG_DIR}}          (holds your login — delete by hand to log out)"
    echo "  the `use` lines in config.nu   (remove by hand)"
    echo "  the hmml entry in the Helix repo's languages.toml"
