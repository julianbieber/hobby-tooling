# TODO

Split by machine, because the two have different things installed.

- **laptop** — has the work tooling checkout, so `tree-sitter-mml` and the MCP
  binaries are buildable there.
- **desktop** (`julian-System-Product-Name`) — hobby-only. Everything in this repo
  is installed and working there except what needs the work checkout.

Both now keep Helix at `~/fun_repos/helix`, but no path is written down in the
tooling regardless — `just local-paths` reports what this machine resolved, and
`just doctor` names the recipe for each gap.

## Resolved by tooling (kept for the reasoning)

- [x] **The `mml` tree-sitter grammar no longer breaks the other machine.** Its
      `[[grammar]]` block was the only machine-specific line in a file shared by
      both, and `helix-term/build.rs` builds grammars with `STRICT = true`, so the
      one absolute path failed the entire `hx` build wherever it did not resolve.
      `wire-helix` now evicts it from the checkout's `languages.toml` and generates
      it into `~/.config/helix/languages.local.toml` from `TREE_SITTER_MML`. The
      eviction is committed and pushed on `custom`, so both machines now pull a file
      that builds — every grammar left in it is git-sourced, with no local paths.
- [x] **The grammar path is tracked in `~/.config/hobby-tooling/paths.env`**, which
      is outside the repo and uncommitted — the only correct home for a path under
      the work tree.
- [x] **The Helix checkout is discovered, not hardcoded.** `~/fun_repos/helix` was
      wrong on the laptop from the day it was written. `scripts/helix-paths.sh`
      tries `$HELIX_REPO`, then the `~/.config/helix/runtime` symlink (which already
      points at the checkout before anything is installed), then the well-known
      locations.
- [x] **`wire-helix` no longer bails out on hobby-only machines.** It used to skip
      everything when the `mml` language was absent, which left those machines with
      no HMML server at all. `.hmml` now always gets the language server.
- [x] **`.hmml` no longer loses highlighting anywhere.** HMML borrowed the work
      grammar via `grammar = "mml"`, and its queries were a bare `; inherits: mml` —
      two dependencies on a checkout hobby-only machines do not have. It now has its
      own fork, `lsps/tree-sitter-hmml`, whose path `wire-helix` derives from the
      repo root, so it is declared unconditionally. `src/parser.c` is committed, so
      building needs a C compiler and nothing else. `TREE_SITTER_MML` is now only
      about the work `.mml` dialect.
      Still v1 in shape, so v2 sections render plainly — but that is now ours to fix.
- [x] **Every personal language server but one is now buildable anywhere.**
      `claude-lsp`, `claude-skills-lsp`, `treesitter-class-lsp` and `path-lsp` were
      copied into `lsps/`; `just install-lsps` builds those four plus a cloned
      `bevy_asset_lsp`. `just helix-servers` derives declared-vs-installed from
      `languages.toml`, so there is no table to keep current.
- [x] **Helix moved to `~/fun_repos/helix`.** Discovery needed no update — the
      `~/.config/helix/runtime` symlink found it at the new path immediately.

| server | source | reachable from a hobby machine? |
|---|---|---|
| `hobby-mml-lsp`, `hobby-mml-parser`, `hobby-mml-index` | this repo | yes — `just install-mml` |
| `claude-lsp`, `claude-skills-lsp`, `treesitter-class-lsp`, `path_lsp` | this repo, `lsps/` | yes — `just install-lsps` |
| `bevy_asset_lsp` | `codeberg.org/julianbieber/bevy_asset_lsp` | yes — `just install-lsps` |
| `static_completions_lsp` | work tooling checkout | no — and fine: declared for markdown, silently inert where the binary is absent |
| `mml-lsp` | work tooling checkout | no, and not wanted — work dialect, `.mml` only |

- [x] **`static_completions_lsp` stays declared and is not vendored.** Helix simply
      ignores a language server whose command is not on `PATH`, so the single
      registration works on the laptop and is inert on the desktop. Nothing to build,
      nothing to remove — `just helix-servers` will list it as missing there, which
      is a note, not a gap.
- [ ] **The vendored four now exist twice.** Same binary names, so whichever repo
      you `cargo install` from last wins in `~/.cargo/bin`, and the copies drift.
      Only this copy has the `CLAUDE_CONFIG_DIR` fix in `claude-skills-lsp`; port it
      to the work copy, or accept that rebuilding there regresses hobby sessions.

## On the laptop

Done, other than the two optional items at the end.

- [x] **Committed the languages.toml change** on `custom` in `~/fun_repos/helix`
      (`78265c4d5`) and pushed it — the `[[grammar]] name = "mml"` block is out of
      the shared file. That repo's identity was also switched to the hobby one; the
      12 commits before it are under the work email.
- [x] **Moved Helix to `~/fun_repos/helix`** and rebuilt nothing to do it: the
      `~/.config/helix/runtime` symlink was repointed and discovery found the rest.
- [x] **Vendored the four language servers** into `lsps/` and installed them, which
      replaced the work-checkout builds in `~/.cargo/bin`.
- [ ] *Optional:* **rebuild `hx`** with `just helix-build`. Not required — the
      binary is from 2026-07-02 and everything since arrives through the merged
      user config, with `hx --health hmml` reporting a parser and a server. The
      recipe wraps the `HELIX_DISABLE_AUTO_GRAMMAR_BUILD=1` dance, because a plain
      build runs `--grammar fetch` first and that dies on `gotmpl` (its remote
      `ngalaiko/tree-sitter-go-template` looks gone, and git then prompts for
      GitHub credentials).
- [ ] *Optional:* **port the `CLAUDE_CONFIG_DIR` fix to the work copy** of
      `claude-skills-lsp` — see the drift item above. Until then, a rebuild from
      the work checkout silently regresses hobby sessions to the work skills.

## On the desktop

In order — the first item has to happen *before* the pull, or the pull fails.

- [ ] **Drop the local edit to `~/fun_repos/helix/languages.toml` before pulling.**
      That machine has the `[[grammar]] name = "mml"` block commented out as an
      uncommitted change; the incoming commit deletes those same lines, so the pull
      will conflict. The edit is obsolete — the block is generated per machine now —
      so discard it: `git -C ~/fun_repos/helix checkout -- languages.toml`.
- [ ] **Pull both repos.** `hobby-tooling` on `main`, `helix` on `custom`.
- [ ] **`just bootstrap`.** Idempotent, and it now links the new scripts, writes
      `~/.config/hobby-tooling/paths.env`, and regenerates the Helix config. No
      `HELIX_REPO` needs setting: the checkout is already at a well-known location
      there, and discovery finds it.
- [ ] **`just install-lsps`** — the four vendored servers plus a cloned
      `bevy_asset_lsp`, which matters most there since hobby projects are Bevy.
      This is the step that was impossible before: none of them needed the work
      checkout any more.
- [ ] **`hx --grammar build`** (or `just helix-build`) once, to compile the
      vendored `hmml` grammar. Needs only a C compiler. Individual failures for
      other grammars are expected and harmless — `gotmpl`'s remote is gone.
- [ ] **`just doctor`** should come back with only the login gap. Expected
      non-gaps: `tree-sitter-mml` reports missing (correct — no work checkout, so
      only the *work* `.mml` dialect loses highlighting; `.hmml` has its own
      grammar), and `helix-servers` lists `static_completions_lsp` and `mml-lsp` as
      missing (also correct — Helix ignores a server whose binary is absent).
- [ ] **`chobby`, then `/login`** with the private account. Still the one step
      that cannot be scripted, and the last real gap in `just doctor`.

Independent of the above:

- [ ] **Install `zellij`** — without it `hcdev` cannot open dev sessions. The
      config (`~/.config/zellij/config.kdl`) is already there and this repo
      symlinks `layouts/hobby-dev.kdl` in, so the binary is the whole job.
- [x] **Install `gh`** — done, 2.96.0, authenticated as `julianbieber`. `/pr-create`
      and the `/gh-issue-*` skills work now.
- [ ] **Review `git -C ~ diff .config/helix/languages.toml`** and commit what you
      want. Heads up: the previously installed copy (143,694 B, mtime 2025-12-04) was
      overwritten by `wire-helix` before being backed up, and it held ~26 KB of edits
      never committed on top of the 2025-03-05 commit. No stash, no dangling blob, no
      snapshot — those edits are gone. What is installed now is the newer
      checkout version, so most of it is probably present again, but the diff is
      worth a skim for anything unique to the old file. Note that this file is now
      **generated** — anything you want to keep belongs in the checkout, or in
      `~/.config/helix/languages.local.toml` if it is specific to that machine.
- [ ] **Clone the hobby projects you actually want to work on.** `~/fun_repos`
      there holds only `helix`, `hobby-mimisbrunnr` and `hobby-tooling`, so `hwt`
      has nothing to make a worktree from yet.

Not worth tracking: the ~226 *upstream* Helix language servers (`gopls`,
`typescript-language-server`, `lua-language-server`, …) that stock Helix declares
uninstalled. Install per language, on demand. `just helix-servers --all` lists them
if you ever want the full picture.
