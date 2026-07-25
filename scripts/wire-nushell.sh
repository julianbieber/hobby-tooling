#!/usr/bin/env bash
# Add the hobby modules to the nushell config, idempotently.
#
# `just install-shell` symlinks hwt.nu / hcdev.nu into ~/.config/nushell/, but a
# module still has to be `use`d to become available. This appends those lines once.
#
# Nushell resolves a bare `use hwt.nu *` relative to the config directory, so the
# symlinks must already be in place — run install-shell first.
#
# Usage: wire-nushell.sh

set -euo pipefail

CFG="${XDG_CONFIG_HOME:-$HOME/.config}/nushell/config.nu"

if [ ! -f "$CFG" ]; then
    echo "  no config.nu at $CFG — creating one"
    mkdir -p "$(dirname "$CFG")"
    touch "$CFG"
fi

changed=0
for mod in hwt hcdev; do
    if grep -qE "^[[:space:]]*use[[:space:]]+${mod}\.nu" "$CFG"; then
        echo "  config.nu: ${mod}.nu already loaded"
    else
        if [ "$changed" -eq 0 ]; then
            printf '\n# Hobby tooling: hwt (worktrees) + hcdev (dev sessions), private Claude account\n' >> "$CFG"
        fi
        printf 'use %s.nu *\n' "$mod" >> "$CFG"
        echo "  config.nu: added \`use ${mod}.nu *\`"
        changed=1
    fi
done

if [ "$changed" -eq 0 ]; then
    echo "  nushell already wired; nothing to do."
else
    echo "  nushell wired. Open a new shell, then run \`hwt\` to check."
fi
