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
ZJ_DIR     := env('HOME') / '.config/zellij'
ZJ_LAYOUTS := ZJ_DIR / 'layouts'
BIN_DIR    := env('HOME') / '.local/bin'
FUN_REPOS  := env('HOME') / 'fun_repos'
NOTES_DIR  := env('HOME') / 'fun_repos/hobby-mimisbrunnr'
# No HELIX_REPO default: the checkout is at a different path on each machine, so
# scripts/helix-paths.sh discovers it (env, then the ~/.config/helix/runtime
# symlink, then well-known locations). Set HELIX_REPO to override.
LSP_REMOTE := env('BEVY_ASSET_LSP_REMOTE', 'ssh://git@codeberg.org/julianbieber/bevy_asset_lsp.git')
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
bootstrap: preflight clone-notes git-identity bootstrap-config install-chobby install-conversation install-shell wire-nushell install-hooks install-skills install-claude-md local-paths wire-helix
    @echo ""
    @echo "════════════════════════════════════════════════════════════════"
    @echo "Core install done. One manual step remains — the login is"
    @echo "interactive, so it cannot be scripted:"
    @echo ""
    @echo "    chobby          then  /login   with your private account"
    @echo ""
    @echo "Then, optionally:"
    @echo "    just install-mcps      the general-purpose MCP servers (Rust build)"
    @echo "    just install-overview  the hwt-overview dashboard (Rust build)"
    @echo "    just install-issues    the hgh-issues dashboard (Rust build)"
    @echo "    just install-mml       forked HMML language server (Rust build)"
    @echo "    just install-lsps      the hobby language servers Helix references"
    @echo "    just helix-build       rebuild hx itself from the discovered checkout"
    @echo ""
    @echo "Check the result any time with:  just doctor"
    @echo "════════════════════════════════════════════════════════════════"

# Bootstrap plus every optional Rust build and the MCP servers.
bootstrap-full: bootstrap install-mcps install-overview install-issues install-mml install-lsps
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

# The conversation dumper is an ordinary hooks/*/hook.sh, so `install-hooks` links it
# like the other five and settings.json calls it by absolute path — there is no
# `claude-conversation-dump` on PATH to go missing, which is what broke this before.
# Only the ctrl-g WRAPPER has to reach ~/.local/bin, because $EDITOR names it.
#
# It keeps the upstream `hx-claude` name for the reason lsps/ keeps theirs: $EDITOR
# is one variable shared by both accounts, so a second name would isolate nothing.
# Installing from here therefore replaces any link from the work checkout — fine,
# since the script is account-agnostic (it keys off the zellij session, and hcdev-*
# can never collide with the work cdev-*).

# The `hx-claude` ctrl-g wrapper — opens the conversation beside the prompt buffer
install-conversation: _root-notice
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{BIN_DIR}}"
    chmod +x "{{REPO_ROOT}}/bin/hx-claude"
    ln -sf "{{REPO_ROOT}}/bin/hx-claude" "{{BIN_DIR}}/hx-claude"
    echo "  linked hx-claude -> {{BIN_DIR}}/hx-claude"
    echo "  (the dump hook itself comes from \`just install-hooks\`)"
    if [ "${EDITOR:-}" != "hx-claude" ]; then
        echo "  NOTE: \$EDITOR is '${EDITOR:-<unset>}' — ctrl-g uses it, so set it to hx-claude:"
        echo "        nushell:  \$env.EDITOR = \"hx-claude\"   in ~/.config/nushell/env.nu"
    fi

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

# Every link goes through link-dotfile.sh rather than `ln -sf`, because zellij's
# config.kdl is a real file on any machine the old home dotfiles repo set up, and
# `ln -sf` will not replace one. The helper moves it aside and says so.

# Symlink the nushell modules + zellij config and layout
install-shell: _root-notice
    @"{{REPO_ROOT}}"/scripts/link-dotfile.sh "{{REPO_ROOT}}/dotfiles/nushell/hwt.nu"   "{{NU_DIR}}/hwt.nu"
    @"{{REPO_ROOT}}"/scripts/link-dotfile.sh "{{REPO_ROOT}}/dotfiles/nushell/hcdev.nu" "{{NU_DIR}}/hcdev.nu"
    @"{{REPO_ROOT}}"/scripts/link-dotfile.sh "{{REPO_ROOT}}/dotfiles/zellij/config.kdl" "{{ZJ_DIR}}/config.kdl"
    @"{{REPO_ROOT}}"/scripts/link-dotfile.sh "{{REPO_ROOT}}/dotfiles/zellij/layouts/hobby-dev.kdl" "{{ZJ_LAYOUTS}}/hobby-dev.kdl"

# Add `use hwt.nu *` / `use hcdev.nu *` to config.nu (idempotent)
wire-nushell:
    @"{{REPO_ROOT}}"/scripts/wire-nushell.sh

# ONE way to change Helix config, the same on every machine:
#
#   machine-neutral -> <checkout>/languages.toml     committed, shared, builds the binary
#   machine-specific -> ~/.config/helix/languages.local.toml   generated, never committed
#   then `just wire-helix` regenerates ~/.config/helix/languages.toml from both.
#
# Editing the installed file directly is the one thing that does not work — it is an
# output. Skips cleanly when no checkout is found.

# Regenerate the Helix language config from the checkout + this machine's overlay
wire-helix:
    @"{{REPO_ROOT}}"/scripts/wire-helix.sh

# Show (and create) ~/.config/hobby-tooling/paths.env, plus what resolved from it
local-paths *ARGS:
    @"{{REPO_ROOT}}"/scripts/local-paths.sh {{ARGS}}

# Which language servers Helix declares, and which are missing here (--all for upstream too)
helix-servers *ARGS:
    @"{{REPO_ROOT}}"/scripts/helix-servers.sh {{ARGS}}

# Rebuilding the hx binary needs two things that are easy to get wrong from memory, so
# they live here rather than in a note: skip the automatic grammar step (it runs
# `--grammar fetch` first, which dies on any grammar whose remote has gone away and
# takes the whole build with it), then build grammars separately where a failure is
# survivable.

# Rebuild and install hx from the discovered checkout, then build its grammars
helix-build:
    #!/usr/bin/env bash
    set -euo pipefail
    repo=$("{{REPO_ROOT}}"/scripts/helix-paths.sh repo) || {
        echo "No Helix checkout found — \`just local-paths\` to point at one." >&2
        exit 1
    }
    echo "Building hx from $repo"
    HELIX_DISABLE_AUTO_GRAMMAR_BUILD=1 cargo install --path "$repo/helix-term" --locked
    echo ""
    echo "Building grammars (individual failures are expected and non-fatal):"
    hx --grammar build || echo "  some grammars failed — hx itself is installed and usable"

# ---------------------------------------------------------------------------
# Optional extras
# ---------------------------------------------------------------------------

# `mcps/` vendors the three general-purpose servers, for the same reason `lsps/` does:
# they used to be buildable only from the work checkout, which left a hobby-only
# machine with no MCPs at all.
#
# Unlike the vendored LANGUAGE servers, these are `hobby-`-prefixed all the way down —
# package, lib and binary — so the two accounts run genuinely separate builds:
#
#     chobby (~/.claude-hobby)  ->  hobby-just-mcp, hobby-lsp-mcp, hobby-tree-extract-mcp
#     claude (~/.claude)        ->  just_mcp,       lsp-mcp,       julian-tree-extract-mcp
#
# The lsps/ argument for keeping upstream names does not apply: there is one Helix
# and one languages.toml, but each account has its OWN MCP registry, so a second name
# isolates rather than duplicates. The PACKAGE rename is the load-bearing part —
# cargo tracks installs by package name, so a hobby build sharing the work package
# name would evict the work binaries from ~/.cargo/bin on every `cargo install`.
#
# The work-only servers are deliberately not here: db and jira are work systems, and
# experts needs a repo level under the task that this layout does not have.
#
# -s user is essential: the default `local` scope binds a server to the single
# directory it was added from, so hobby projects would see no MCPs at all. The
# remove-before-add is what lets this recipe REPOINT an entry that still names the
# work binary, rather than reporting "already registered" and leaving it stale.

# Build + register the hobby MCP servers from mcps/ (chobby only; work claude untouched)
install-mcps:
    #!/usr/bin/env bash
    set -euo pipefail
    for d in just-mcp lsp-mcp tree-extract; do
        echo "  building $d"
        cargo install --path "{{REPO_ROOT}}/mcps/$d" --locked
    done
    echo ""
    set +e
    add() {
        if command -v "$2" >/dev/null 2>&1; then
            CLAUDE_CONFIG_DIR="{{CONFIG_DIR}}" claude mcp remove -s user "$1" >/dev/null 2>&1
            CLAUDE_CONFIG_DIR="{{CONFIG_DIR}}" claude mcp add -s user "$1" -- "$2" >/dev/null 2>&1 \
                && echo "  $1 -> $2" || echo "  FAILED to register $1"
        else
            echo "  SKIP $1 — \`$2\` not on PATH (is ~/.cargo/bin on PATH?)"
        fi
    }
    add lsp-mcp      hobby-lsp-mcp
    add tree-extract hobby-tree-extract-mcp
    add just-mcp     hobby-just-mcp
    echo ""
    CLAUDE_CONFIG_DIR="{{CONFIG_DIR}}" claude mcp list

# Build + install the hwt-overview dashboard
install-overview:
    cd tools/hwt-overview && cargo install --path crates/hwt_app

# `hgh-issues` is its own workspace but shares hwt-overview's `feathers_kit` and
# `hwt_core` through path dependencies, so the two dashboards cannot drift apart
# visually and there is no third copy of the UI kit in this repo.

# Build + install the hgh-issues GitHub-issue dashboard
install-issues:
    cd tools/hgh-issues && cargo install --path crates/issues_app

# `lsps/` vendors four servers that used to be buildable only from the work checkout,
# which left a hobby-only machine unable to install them at all. They keep their
# original binary names on purpose: there is one Helix and one languages.toml across
# both accounts, so a second name would mean a second server running on every buffer.
# One binary, and `claude-skills-lsp` picks its account from CLAUDE_CONFIG_DIR at
# runtime. Building from here therefore REPLACES any build from the work checkout.
#
# `bevy_asset_lsp` is a separate hobby repo, so it is cloned rather than vendored.

# Build every language server this repo can: the vendored four, plus bevy_asset_lsp
install-lsps:
    #!/usr/bin/env bash
    set -euo pipefail
    for d in claude-lsp claude-skills-lsp treesitter-class-lsp path-lsp; do
        echo "  building $d"
        cargo install --path "{{REPO_ROOT}}/lsps/$d" --locked
    done
    src="{{FUN_REPOS}}/bevy_asset_lsp"
    if [ ! -d "$src/.git" ]; then
        mkdir -p "{{FUN_REPOS}}"
        echo "  cloning {{LSP_REMOTE}}"
        git clone "{{LSP_REMOTE}}" "$src"
    fi
    cargo install --path "$src"
    echo ""
    "{{REPO_ROOT}}"/scripts/helix-servers.sh

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

# Run every test in the repo (all language servers + MCP servers + the dashboard)
test:
    cd lsps/hobby-mml-lsp && cargo test --workspace
    cd lsps/claude-lsp && cargo test
    cd lsps/claude-skills-lsp && cargo test
    cd lsps/treesitter-class-lsp && cargo test
    cd lsps/path-lsp && cargo test
    cd mcps/just-mcp && cargo test
    cd mcps/lsp-mcp && cargo test
    cd mcps/tree-extract && cargo test
    cd tools/hwt-overview && cargo test --workspace
    cd tools/hgh-issues && cargo test --workspace

# ---------------------------------------------------------------------------
# Uninstall
# ---------------------------------------------------------------------------

# Leaves ~/.claude-hobby (and therefore the login) and both git repos untouched;
# delete those by hand if you really mean to.

# Remove the symlinks this repo installs
uninstall:
    #!/usr/bin/env bash
    set -euo pipefail
    rm -f "{{NU_DIR}}/hwt.nu" "{{NU_DIR}}/hcdev.nu" "{{ZJ_LAYOUTS}}/hobby-dev.kdl"
    rm -f "{{BIN_DIR}}/chobby" "{{BIN_DIR}}/hx-claude"
    rm -f "$HOME/fun_worktrees/CLAUDE.md"
    rm -f "{{CONFIG_DIR}}/settings.json" "{{CONFIG_DIR}}/CLAUDE.md"
    rm -f "{{CONFIG_DIR}}"/commands/*.md "{{CONFIG_DIR}}"/hooks/*.sh
    echo "Unlinked. Still present on purpose:"
    echo "  {{CONFIG_DIR}}          (holds your login — delete by hand to log out)"
    echo "  the `use` lines in config.nu   (remove by hand)"
    echo "  the hmml entry in the Helix checkout's languages.toml"
    echo "  ~/.config/helix/languages.local.toml and ~/.config/hobby-tooling/paths.env"
    echo "  (both machine-local; harmless to keep, and a reinstall reuses them)"
