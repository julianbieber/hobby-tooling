#!/usr/bin/env bash
# Wire the hobby MML language server into Helix.
#
# Helix's languages.toml is maintained in a local Helix checkout (branch `custom`)
# and *copied* into ~/.config/helix/. Editing the installed copy directly loses the
# change the next time it is refreshed from the repo, so all edits go to the repo
# copy and this script installs from there.
#
# `runtime/` is expected to be a symlink from ~/.config/helix/runtime into the repo,
# which is why the query files are written into the repo and need no separate copy.
#
# Idempotent: re-running detects existing entries and changes nothing.
#
# Usage: wire-helix.sh [helix-repo-dir]
#        HELIX_REPO=/path/to/helix wire-helix.sh

set -euo pipefail

REPO="${1:-${HELIX_REPO:-$HOME/outside_repos/helix}}"
CFG="${XDG_CONFIG_HOME:-$HOME/.config}/helix"
LANGS="$REPO/languages.toml"
QUERIES="$REPO/runtime/queries/mml-hobby"

if [ ! -f "$LANGS" ]; then
    echo "wire-helix: no languages.toml at $LANGS" >&2
    echo "  Pass the Helix checkout as an argument, or set HELIX_REPO." >&2
    echo "  Skipping Helix wiring — hwt/hcdev/skills all work without it." >&2
    exit 0
fi

# The forked server only makes sense if the plain `mml` language exists to borrow a
# grammar from. Without it, there is no MML support to extend.
if ! grep -q '^name = "mml"$' "$LANGS"; then
    echo "wire-helix: no \`mml\` language in $LANGS — nothing to extend, skipping." >&2
    exit 0
fi

changed=0

# --- 1. language-server entry ------------------------------------------------
if grep -q '^hobby-mml-lsp = ' "$LANGS"; then
    echo "  languages.toml: hobby-mml-lsp server already registered"
else
    # Insert directly after the work mml-lsp entry so the two sit together.
    if grep -q '^mml-lsp = ' "$LANGS"; then
        sed -i '/^mml-lsp = /a hobby-mml-lsp = { command = "hobby-mml-lsp" }' "$LANGS"
    else
        sed -i '/^\[language-server\]/a hobby-mml-lsp = { command = "hobby-mml-lsp" }' "$LANGS"
    fi
    echo "  languages.toml: registered hobby-mml-lsp"
    changed=1
fi

# --- 2. the mml-hobby language, routed by path glob --------------------------
if grep -q '^name = "mml-hobby"$' "$LANGS"; then
    echo "  languages.toml: mml-hobby language already present"
else
    # Must precede the plain `[[language]] name = "mml"` block. Verified empirically:
    # a spec under hobby-mimisbrunnr/notes starts hobby-mml-lsp, one outside starts
    # mml-lsp.
    block=$(mktemp)
    cat > "$block" <<'TOML'
# Hobby specs get the forked server, routed by path. This entry must come BEFORE
# the plain `mml` entry below: the hobby parser resolves spec->source against
# ~/fun_worktrees/<project>/<topic>/<task> rather than ~/worktrees/<topic>/<task>/<repo>.
# `grammar = "mml"` reuses the tree-sitter grammar the `mml` entry declares, so
# there is no second grammar to build.
[[language]]
name = "mml-hobby"
scope = "source.mml"
injection-regex = "mml"
file-types = [{ glob = "**/hobby-mimisbrunnr/notes/**/*.mml" }]
comment-token = "#"
indent = { tab-width = 2, unit = "  " }
grammar = "mml"
language-servers = ["hobby-mml-lsp"]

TOML
    # Find the [[language]] header that introduces `name = "mml"` and insert above it.
    line=$(grep -n '^name = "mml"$' "$LANGS" | head -1 | cut -d: -f1)
    hdr=$(awk -v n="$line" 'NR<n && /^\[\[language\]\]$/ {l=NR} END {print l}' "$LANGS")
    sed -i "$((hdr - 1))r $block" "$LANGS"
    rm -f "$block"
    echo "  languages.toml: added mml-hobby language (glob-routed)"
    changed=1
fi

# --- 3. highlight queries ----------------------------------------------------
# Helix resolves queries by *language name*, not grammar, so mml-hobby needs its own
# directory even though it reuses the mml grammar. Without this, hobby specs parse
# but render unhighlighted.
if [ -f "$QUERIES/highlights.scm" ]; then
    echo "  queries: mml-hobby/highlights.scm already present"
else
    mkdir -p "$QUERIES"
    cat > "$QUERIES/highlights.scm" <<'SCM'
; MML highlighting for hobby specs.
;
; `mml-hobby` is the same language as `mml` — it exists only so Helix can route
; hobby specs to the forked `hobby-mml-lsp`, which resolves spec->source against
; ~/fun_worktrees instead of ~/worktrees. Helix looks queries up by *language
; name*, not by grammar, so without this file hobby specs would render
; unhighlighted even though `grammar = "mml"` gives them the right parser.
;
; Inherit rather than copy, so the two stay in sync.
; inherits: mml
SCM
    echo "  queries: wrote mml-hobby/highlights.scm"
    changed=1
fi

# --- 4. install languages.toml from the repo --------------------------------
mkdir -p "$CFG"
if [ -L "$CFG/languages.toml" ]; then
    echo "  ~/.config/helix/languages.toml is a symlink — leaving it alone"
elif cmp -s "$LANGS" "$CFG/languages.toml"; then
    echo "  ~/.config/helix/languages.toml already matches the repo"
else
    cp "$LANGS" "$CFG/languages.toml"
    echo "  copied languages.toml -> $CFG/languages.toml"
    changed=1
fi

# runtime/ is normally a symlink into the repo; if it is a real directory the query
# files written above are invisible to Helix, so say so.
if [ ! -L "$CFG/runtime" ] && [ -d "$CFG/runtime" ]; then
    echo "  NOTE: $CFG/runtime is a real directory, not a symlink into $REPO."
    echo "        Copy the queries yourself:  cp -r $QUERIES $CFG/runtime/queries/"
fi

if [ "$changed" -eq 0 ]; then
    echo "  Helix already wired; nothing to do."
else
    echo "  Helix wired. Verify:  hx --health mml-hobby   and   hx --health mml"
fi
