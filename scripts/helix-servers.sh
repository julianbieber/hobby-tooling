#!/usr/bin/env bash
# Which language servers does Helix declare, and which of them exist here?
#
# This replaces a hand-written table of "missing on the desktop" that went stale the
# moment either machine changed. The declarations are read out of the effective
# languages.toml, so the answer is derived, never maintained.
#
# Stock Helix declares ~100 servers and most of any machine's misses are upstream
# ones you install per language, on demand — so the default output is the personal
# ones only. `--all` includes upstream.
#
# Usage: helix-servers.sh [--all] [--count]

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./helix-paths.sh
source "$HERE/helix-paths.sh"

show_all=0
count_only=0
for a in "$@"; do
    case "$a" in
        --all) show_all=1 ;;
        --count) count_only=1 ;;
        *) echo "usage: helix-servers.sh [--all] [--count]" >&2; exit 2 ;;
    esac
done

LANGS="$HELIX_CFG/languages.toml"
[ -f "$LANGS" ] || { echo "no $LANGS — run \`just wire-helix\`" >&2; exit 1; }

# A server is "personal" when its command is not a name stock Helix ships. Rather
# than keep a list of upstream servers, use the inverse test that actually matters:
# personal tools are built by the owner of this machine, so they live in ~/.cargo/bin
# or are declared by a checkout-local edit. The cheap, stable discriminator is the
# `julian`/`hobby`/`bevy`/`claude` naming these tools already use.
is_personal() {
    case "$1" in
        julian*|hobby*|bevy*|claude*|*mml*|static_completions*|treesitter-class*|path_lsp*) return 0 ;;
        *) return 1 ;;
    esac
}

# Both spellings occur: `name = { command = "x" }` inline, and a
# [language-server.name] section with `command = "x"` on its own line.
commands=$(awk '
    /^\[language-server\.[^]]+\]$/ { insec = 1; next }
    /^\[/ && !/^\[language-server\.[^]]+\]$/ { insec = 0 }
    insec && /^command = "/ { match($0, /"[^"]+"/); print substr($0, RSTART + 1, RLENGTH - 2); next }
    /^[A-Za-z0-9_.-]+ = \{[^}]*command = "/ {
        match($0, /command = "[^"]+"/)
        s = substr($0, RSTART + 11, RLENGTH - 12)
        print s
    }
' "$LANGS" | sort -u)

missing=0
total=0
missing_personal=0
out=""
while IFS= read -r cmd; do
    [ -n "$cmd" ] || continue
    total=$((total + 1))
    personal=no
    is_personal "$cmd" && personal=yes
    if command -v "$cmd" >/dev/null 2>&1; then
        state=ok
    else
        state=missing
        missing=$((missing + 1))
        [ "$personal" = yes ] && missing_personal=$((missing_personal + 1))
    fi
    if [ "$show_all" = 1 ] || [ "$personal" = yes ]; then
        out="$out$state $personal $cmd"$'\n'
    fi
done <<< "$commands"

if [ "$count_only" = 1 ]; then
    printf '%s %s %s\n' "$total" "$missing" "$missing_personal"
    exit 0
fi

printf '%d language servers declared, %d not on PATH (%d of them personal).\n\n' \
    "$total" "$missing" "$missing_personal"
printf '%s' "$out" | while read -r state personal cmd; do
    if [ "$state" = ok ]; then
        printf '  \033[32m✓\033[0m %-28s %s\n' "$cmd" "$(command -v "$cmd")"
    else
        printf '  \033[31m✗\033[0m %-28s \033[2mnot on PATH\033[0m\n' "$cmd"
    fi
done
if [ "$show_all" = 0 ]; then
    printf '\n  Personal servers only. \033[2m--all\033[0m adds the ~%d upstream ones,\n' \
        "$((total - $(printf '%s' "$out" | grep -c . )))"
    printf '  which stock Helix declares uninstalled and you add per language.\n'
fi
