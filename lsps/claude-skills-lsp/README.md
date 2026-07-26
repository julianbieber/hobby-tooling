# Julian Claude Skills LSP

| Field        | Value                                                                      |
| ------------ | -------------------------------------------------------------------------- |
| Type         | lsp (Rust source + binary)                                                 |
| Owner        | @julianbieber                                                              |
| Dependencies | Rust toolchain (stable edition 2024); editor with LSP client (Helix, etc.) |
| Status       | experimental                                                               |
| Upstream     | local (no remote configured at import time)                                |

## Description

LSP server that autocompletes Claude Code slash-commands / skills and jumps from
a reference to its source file. Its primary purpose is the **Ctrl+G prompt
buffer**: Claude Code opens the current prompt in `$EDITOR` as
`<tmpdir>/claude-<uid>/claude-prompt-*.md`, but that external editor has no
slash-command completion (only Claude's own TUI does). This LSP fills the gap so
typing `/mml` in the buffer offers `/julian:mml-review`, etc. It also activates
on the skill/command source files themselves — anything under a `.claude/`
directory.

It builds an index of every invocable name across three tiers, merged into one
namespace (lower tier wins a collision):

- **Project** — `<workspace-root>/.claude/{commands,skills}`
- **User** — `~/.claude/{commands,skills}`
- **Plugin** — enabled plugins listed in
  `~/.claude/plugins/installed_plugins.json`

Command names come from the path under `commands/` (`.md` stripped, `/` → `:`,
so `commands/julian/mml-extract.md` → `julian:mml-extract`). Skill names come
from the frontmatter `name:` (else the directory name). Plugin skills are
namespaced by the plugin's `plugin.json` `name`. Symlinks are resolved per file
(`canonicalize`), so goto-definition lands on the real source even when
`~/.claude/commands/*.md` symlinks into a tooling-repo clone.

Capabilities:

- `textDocument/completion` — fuzzy-ranked names; trigger characters `/` and `:`
- `textDocument/definition` — jump to the command/skill source file
- `textDocument/hover` — show the target's description and source path
- `textDocument/sync` = full (used to recover the reference under the cursor)

The server is a no-op unless the open file is the Ctrl+G buffer
(`claude-prompt-*`) or lives under a `.claude/` directory, so it is safe to
attach to `markdown` globally without disturbing ordinary notes.

## Installation

### Build

```bash
cd ~/fun_repos/hobby-tooling/lsps/claude-skills-lsp
cargo install --path .
```

Produces `~/.cargo/bin/claude-skills-lsp`.

### Wire into Helix

Add the server and append it to the existing `markdown` language-servers list in
`~/.config/helix/languages.toml`:

```toml
[language-server.claude-skills-lsp]
command = "claude-skills-lsp"

# The Ctrl+G buffer (/tmp/claude-<uid>/claude-prompt-*.md) is already markdown,
# so it attaches automatically; the server no-ops unless the file is that buffer
# or lives under a .claude/ directory. Keep your other markdown servers and just
# add "claude-skills-lsp" to the array.
[[language]]
name = "markdown"
language-servers = [ "marksman", "markdown-oxide", "rumdl", "latex_ls", "scls", "zk",
                     "claude-lsp", "static-completions", "treesitter-class-lsp",
                     "path-relative", "claude-skills-lsp" ]
```

Restart Helix. In a Claude session, press **Ctrl+G**, type `/` and start a
command name; `gd` on a reference jumps to its source; hover shows its
description.

## Usage

- **Complete:** type `/name` or `prefix:name` (or retrigger completion with
  `Ctrl-X` in Helix insert mode). The `/` you typed is preserved; only the name
  after it is replaced.
- **Goto-definition (`gd`):** place the cursor on a `/name`, `prefix:name`, or a
  backtick-wrapped `` `name` `` and jump to the source `.md`. Only known indexed
  names resolve, so a real path like `/home` is left alone.
- **Hover:** shows the name, kind (command/skill), tier, description, and source
  path.

## Known Limitations

- **Project-tier commands aren't discovered from the Ctrl+G buffer.** That
  buffer lives in `/tmp`, so the editor's workspace root is `/tmp` and
  repo-local `.claude/commands` are not seen there. User- and plugin-tier
  commands (the common case — most shared commands are symlinked into
  `~/.claude/commands`) still work. Project-tier discovery applies when editing
  actual `.claude/` files inside a repo the editor has opened as its workspace.
- Plugins are treated as enabled unless `enabledPlugins` maps their
  `<name>@<marketplace>` key explicitly to `false` (active local plugins are
  often absent from that map, so "absent" must mean enabled).
- Frontmatter parsing reads single-line `name:` / `description:` scalars only;
  folded/multiline (`>-`, `|`) values are treated as absent.
- The index is cached with a 5-second freshness window (refreshed immediately
  when you save a file under `.claude/{commands,skills}`); a brand-new skill can
  take a few seconds to appear otherwise.
- UTF-16 column math is approximated as `char` counts — fine for ASCII command
  names.

## Development

```bash
cargo build
cargo test                              # unit tests + stdio smoke test
cargo clippy --all-targets -- -D warnings
cargo fmt
```
