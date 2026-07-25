# TODO

Split by machine, because the two have different things installed.

- **laptop** — has the work tooling checkout, so `tree-sitter-mml` and the MCP
  binaries are buildable there.
- **desktop** (`julian-System-Product-Name`) — hobby-only. Everything in this repo
  is installed and working here except what needs the work checkout.

## On the laptop

- [ ] **Restore the `mml` tree-sitter grammar.** In `~/fun_repos/helix/languages.toml`
      the `[[grammar]] name = "mml"` block is commented out — uncomment it and point
      `source.path` at the real `tree-sitter-mml` checkout. It was disabled because
      `helix-term/build.rs` fails the *whole* `hx` build on one unbuildable grammar,
      and the path it held did not exist on the desktop.
- [ ] **Rebuild after that:** `hx --grammar build`. If rebuilding the binary too, use
      `HELIX_DISABLE_AUTO_GRAMMAR_BUILD=1 cargo install --path helix-term --locked`
      — a plain build runs `--grammar fetch` first, which currently dies on `gotmpl`
      (its remote `ngalaiko/tree-sitter-go-template` looks gone and git then prompts
      for GitHub credentials).
- [ ] **Decide how the grammar path is tracked.** `source.path` is machine-specific:
      committing the laptop's path to the `custom` branch re-breaks the desktop build
      on the next pull. Either keep the block uncommitted per machine, or find a path
      that resolves on both.
- [ ] **Build the three MCP binaries** (`lsp-mcp`, `julian-tree-extract-mcp`,
      `just_mcp`), then `just install-mcps`. `install-mcps` skips by name when a
      binary is absent, so it is safe to re-run.

## Here (desktop)

- [ ] **`chobby`, then `/login`** with the private account. The only step `just
      bootstrap` cannot script. This is the last real gap in `just doctor`.
- [ ] **Install `zellij`** — without it `hcdev` cannot open dev sessions.
- [x] **Install `gh`** — done, 2.96.0, authenticated as `julianbieber`. `/pr-create`
      and the `/gh-issue-*` skills work now.
- [ ] **Review `git -C ~ diff .config/helix/languages.toml`** and commit what you
      want. Heads up: the previously installed copy (143,694 B, mtime 2025-12-04) was
      overwritten by `wire-helix` before being backed up, and it held ~26 KB of edits
      never committed on top of the 2025-03-05 commit. No stash, no dangling blob, no
      snapshot — those edits are gone. What is installed now is the newer
      `~/fun_repos/helix` version, so most of it is probably present again, but the
      diff is worth a skim for anything unique to the old file.
- [ ] **Commit the Helix change**: `~/fun_repos/helix` (branch `custom`) has the
      commented-out grammar block as an uncommitted edit.

### Missing binaries and configs (desktop)

`zellij` — `~/.config/zellij/config.kdl` is already here and tracked in the home
dotfiles repo, and this repo symlinks `layouts/hobby-dev.kdl` in. Only the
**binary** is missing, so installing zellij is the whole job; no config work.

Personal language servers referenced by `~/fun_repos/helix/languages.toml` that are
not on `PATH` here. Only `hobby-mml-lsp` is installed:

| server | command | here |
|---|---|---|
| `hobby-mml-lsp` | `hobby-mml-lsp` | installed |
| `bevy-asset-lsp` | `bevy_asset_lsp` | **missing** — matters most, hobby projects are Bevy |
| `claude-lsp` | `claude-lsp` | missing |
| `claude-skills-lsp` | `claude-skills-lsp` | missing |
| `treesitter-class-lsp` | `treesitter-class-lsp` | missing |
| `static-completions` | `static_completions_lsp` | missing |
| `mml-lsp` | `mml-lsp` | missing **on purpose** — work dialect, `.mml` only |

- [ ] Work out where each of those is built. None of their sources are in
      `~/fun_repos`, so this is probably a laptop-side build-and-copy, or a clone
      that never happened here.

Also missing: **the hobby project repos themselves.** `~/fun_repos` holds only
`helix`, `hobby-mimisbrunnr`, and `hobby-tooling` — none of the actual projects
(`mosaik_map`, `securitd`, `glsllsp`, the Bevy games) are cloned, so `hwt` has
nothing to make a worktree from yet.

- [ ] Clone whichever projects you actually want to work on here.

Not worth tracking: ~15 *upstream* Helix language servers (`gopls`,
`typescript-language-server`, `lua-language-server`, …) are also absent. Those ship
declared-but-uninstalled in stock Helix — install per language, on demand.

## Not fixable here

`.hmml` files get the language server, diagnostics, and file-type routing, but **no
syntax highlighting** until the `mml` grammar exists — `hmml` borrows it via
`grammar = "mml"`. `hx --health hmml` shows `Tree-sitter parser: None` and that is
expected until the laptop item above is done.
