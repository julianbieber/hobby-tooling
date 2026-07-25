# hobby-tooling — install/uninstall the hobby Claude setup
#
# Every install-* recipe is idempotent and independently re-runnable.
# `ln -sf` replaces stale symlinks but NOT regular files — if a target already
# exists as a real file, remove it first.

CONFIG_DIR := env('HOME') / '.claude-hobby'
NU_DIR     := env('HOME') / '.config/nushell'
ZJ_LAYOUTS := env('HOME') / '.config/zellij/layouts'
BIN_DIR    := env('HOME') / '.local/bin'

# List all recipes
default:
    @just --list

# Everything, in dependency order
install: bootstrap-config install-chobby install-shell install-hooks install-skills install-claude-md
    @echo ""
    @echo "Installed. Two things left, both one-time:"
    @echo "  1. Log in to the private account:  chobby   then  /login"
    @echo "  2. Add the MCP servers:            just install-mcps"
    @echo ""
    @echo "Optional Rust builds: just install-overview / just install-mml"

# Create ~/.claude-hobby and link the settings + account CLAUDE.md into it
bootstrap-config:
    @mkdir -p "{{CONFIG_DIR}}/commands" "{{CONFIG_DIR}}/hooks" "{{CONFIG_DIR}}/agents" "{{CONFIG_DIR}}/skills"
    @ln -sf "$(pwd)/settings/hobby.settings.json" "{{CONFIG_DIR}}/settings.json"
    @ln -sf "$(pwd)/dotfiles/claude-md/hobby.CLAUDE.md" "{{CONFIG_DIR}}/CLAUDE.md"
    @echo "Config dir ready: {{CONFIG_DIR}}"

# The `chobby` wrapper — bare claude on the hobby account
install-chobby:
    @mkdir -p "{{BIN_DIR}}"
    @ln -sf "$(pwd)/bin/chobby" "{{BIN_DIR}}/chobby"
    @echo "Linked chobby -> {{BIN_DIR}}/chobby"
    @echo "Ensure {{BIN_DIR}} is on PATH."

# Nushell modules + zellij layout
install-shell:
    @mkdir -p "{{NU_DIR}}" "{{ZJ_LAYOUTS}}"
    @ln -sf "$(pwd)/dotfiles/nushell/hwt.nu"   "{{NU_DIR}}/hwt.nu"
    @ln -sf "$(pwd)/dotfiles/nushell/hcdev.nu" "{{NU_DIR}}/hcdev.nu"
    @ln -sf "$(pwd)/dotfiles/zellij/layouts/hobby-dev.kdl" "{{ZJ_LAYOUTS}}/hobby-dev.kdl"
    @echo "Linked hwt.nu, hcdev.nu, hobby-dev.kdl"
    @echo "Add to ~/.config/nushell/config.nu:"
    @echo "    use hwt.nu *"
    @echo "    use hcdev.nu *"

# Hooks into the hobby config dir
install-hooks:
    @mkdir -p "{{CONFIG_DIR}}/hooks"
    @for d in hooks/*/; do \
        n=$(basename "$d"); \
        [ -f "$d/hook.sh" ] || continue; \
        ln -sf "$(pwd)/$d/hook.sh" "{{CONFIG_DIR}}/hooks/$n.sh"; \
        echo "  hook: $n"; \
    done

# Skills as slash commands in the hobby config dir
install-skills:
    @mkdir -p "{{CONFIG_DIR}}/commands"
    @for d in skills/*/; do \
        n=$(basename "$d"); \
        [ -f "$d/skill.md" ] || continue; \
        ln -sf "$(pwd)/$d/skill.md" "{{CONFIG_DIR}}/commands/$n.md"; \
        echo "  skill: /$n"; \
    done

# CLAUDE.md files into ~/ and ~/fun_worktrees/
install-claude-md:
    @mkdir -p "$HOME/fun_worktrees"
    @ln -sf "$(pwd)/dotfiles/claude-md/fun_worktrees.CLAUDE.md" "$HOME/fun_worktrees/CLAUDE.md"
    @echo "Linked fun_worktrees.CLAUDE.md"
    @echo "Note: ~/CLAUDE.md is the WORK repo's file. The hobby account reads"
    @echo "{{CONFIG_DIR}}/CLAUDE.md instead — linked by bootstrap-config."

# MCP servers for the hobby account (generic ones only — no work MCPs).
# Binaries come from ~/.cargo/bin, installed out of the work tooling repo. The
# `julian-` prefix on tree-extract is just its crate name; the tool itself is
# domain-agnostic.
#
# -s user is essential: the default `local` scope binds a server to the single
# directory it was added from, so hobby projects would see no MCPs at all.
install-mcps:
    @CLAUDE_CONFIG_DIR="{{CONFIG_DIR}}" claude mcp add -s user lsp-mcp      -- lsp-mcp
    @CLAUDE_CONFIG_DIR="{{CONFIG_DIR}}" claude mcp add -s user tree-extract -- julian-tree-extract-mcp
    @CLAUDE_CONFIG_DIR="{{CONFIG_DIR}}" claude mcp add -s user just-mcp     -- just_mcp
    @CLAUDE_CONFIG_DIR="{{CONFIG_DIR}}" claude mcp list

# Build + install the hwt-overview dashboard
install-overview:
    cd tools/hwt-overview && cargo install --path crates/hwt_app

# Build + install the forked MML language server, parser CLI, and index.
# The parser CLI matters: the mml-* skills shell out to `hobby-mml-parser --lint`.
install-mml:
    cd lsps/hobby-mml-lsp && cargo install --path mml-lsp
    cd lsps/hobby-mml-lsp && cargo install --path mml-parser
    cd tools/hobby-mml-index && cargo install --path .

# Show what's currently linked
status:
    @echo "config dir: {{CONFIG_DIR}}"
    @ls -la "{{CONFIG_DIR}}" 2>/dev/null || echo "  (not created)"
    @echo ""
    @echo "commands:"
    @ls "{{CONFIG_DIR}}/commands" 2>/dev/null || echo "  (none)"
    @echo ""
    @echo "hooks:"
    @ls "{{CONFIG_DIR}}/hooks" 2>/dev/null || echo "  (none)"

# Remove symlinks from ~/.config and ~/.local/bin (leaves the config dir alone)
uninstall-shell:
    @rm -f "{{NU_DIR}}/hwt.nu" "{{NU_DIR}}/hcdev.nu" "{{ZJ_LAYOUTS}}/hobby-dev.kdl" "{{BIN_DIR}}/chobby"
    @echo "Unlinked shell integration. Remove the `use` lines from config.nu by hand."
