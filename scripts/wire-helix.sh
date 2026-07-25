#!/usr/bin/env bash
# Wire the hobby HMML language server into Helix.
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
QUERIES="$REPO/runtime/queries/hmml"
LEGACY_QUERIES="$REPO/runtime/queries/mml-hobby"

if [ ! -f "$LANGS" ]; then
    echo "wire-helix: no languages.toml at $LANGS" >&2
    echo "  Pass the Helix checkout as an argument, or set HELIX_REPO." >&2
    echo "  Skipping Helix wiring — hwt/hcdev/skills all work without it." >&2
    exit 0
fi

# Routing no longer depends on the work `mml` language — `.hmml` is its own file
# type. But `grammar = "mml"` below still borrows the tree-sitter-mml build the
# `mml` entry declares, so without that entry there is no grammar to reuse and
# hobby specs would parse as plain text.
if ! grep -q '^name = "mml"$' "$LANGS"; then
    echo "wire-helix: no \`mml\` language in $LANGS — no grammar to borrow, skipping." >&2
    exit 0
fi

changed=0

# --- 0. drop the pre-.hmml `mml-hobby` language ------------------------------
# Before the rename, both dialects claimed `.mml` and hobby specs were singled
# out by the path glob `**/hobby-mimisbrunnr/notes/**/*.mml`. That entry is dead
# now and would fight the `hmml` block below for any `.mml` file, so remove it.
# Guarded on presence: a fresh machine no-ops straight through.
if grep -q '^name = "mml-hobby"$' "$LANGS"; then
    awk '
      NR == FNR { line[FNR] = $0; n = FNR; if ($0 ~ /^name = "mml-hobby"$/) hit = FNR; next }
      FNR == 1 {
        # Walk back to the [[language]] header, then over its comment preamble.
        h = hit; while (h > 1 && line[h] !~ /^\[\[language\]\]$/) h--
        s = h;   while (s > 1 && line[s - 1] ~ /^#/) s--
        # Forward to the line before the next top-level header, plus any blanks.
        e = h + 1; while (e <= n && line[e] !~ /^\[/) e++
        e--
        while (e < n && line[e + 1] == "") e++
        for (i = 1; i <= n; i++) if (i < s || i > e) print line[i]
      }
    ' "$LANGS" "$LANGS" > "$LANGS.tmp"
    mv "$LANGS.tmp" "$LANGS"
    echo "  languages.toml: removed the legacy glob-routed mml-hobby language"
    changed=1
fi
if [ -d "$LEGACY_QUERIES" ]; then
    rm -rf "$LEGACY_QUERIES"
    echo "  queries: removed legacy mml-hobby/"
    changed=1
fi

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

# --- 2. the hmml language, routed by extension -------------------------------
# No ordering constraint: `hmml` and `mml` claim disjoint file types, so this can
# simply be appended. (Before the rename both claimed `.mml` and this block had
# to be spliced in above the `mml` entry to win.)
if grep -q '^name = "hmml"$' "$LANGS"; then
    echo "  languages.toml: hmml language already present"
else
    cat >> "$LANGS" <<'TOML'

# Hobby specs are `.hmml` and get the forked server, which resolves spec->source
# against ~/fun_worktrees/<project>/<topic>/<task> rather than the work layout
# ~/worktrees/<topic>/<task>/<repo>. `grammar = "mml"` reuses the tree-sitter
# grammar the `mml` entry declares, so there is no second grammar to build.
[[language]]
name = "hmml"
scope = "source.hmml"
injection-regex = "hmml"
file-types = ["hmml"]
comment-token = "#"
indent = { tab-width = 2, unit = "  " }
grammar = "mml"
language-servers = ["hobby-mml-lsp"]
TOML
    echo "  languages.toml: added hmml language (routed by the .hmml extension)"
    changed=1
fi

# --- 3. highlight queries ----------------------------------------------------
# Helix resolves queries by *language name*, not grammar, so hmml needs its own
# directory even though it reuses the mml grammar. Without this, hobby specs
# parse but render unhighlighted.
if [ -f "$QUERIES/highlights.scm" ]; then
    echo "  queries: hmml/highlights.scm already present"
else
    mkdir -p "$QUERIES"
    cat > "$QUERIES/highlights.scm" <<'SCM'
; HMML highlighting.
;
; `hmml` is the same grammar as `mml` — it is a separate Helix language so that
; `.hmml` files reach the forked `hobby-mml-lsp`, which resolves spec->source
; against ~/fun_worktrees instead of ~/worktrees. Helix looks queries up by
; *language name*, not by grammar, so without this file hobby specs would render
; unhighlighted even though `grammar = "mml"` gives them the right parser.
;
; Inherit rather than copy, so the two stay in sync.
; inherits: mml
SCM
    echo "  queries: wrote hmml/highlights.scm"
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
    echo "        And drop the stale one:     rm -rf $CFG/runtime/queries/mml-hobby"
fi

if [ "$changed" -eq 0 ]; then
    echo "  Helix already wired; nothing to do."
else
    echo "  Helix wired. Verify:  hx --health hmml   and   hx --health mml"
fi
