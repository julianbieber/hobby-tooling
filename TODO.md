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
- [x] **`helix-paths.sh repo` no longer exits 1 on success.** Its `repo` branch ended
      with `[ "${2:-}" = "--why" ] && printf ...`, so without `--why` that false test
      *was* the script's exit status: the correct path on stdout, and a failure code
      behind it. Every other caller sources the file and calls `resolve_repo`
      directly, so only `just helix-build` ever saw it — and it saw it as
      "No Helix checkout found", on a machine where the checkout was resolving fine.
      Now an `if`, so the branch always ends 0.
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
- [x] **The grammar speaks v2, and stops erroring through whole files.** The fork
      still modelled v1, which was worse than plain rendering: `## components` and
      friends were not in `section_name`, so an unmatched header errored out to the
      end of the file — 13 of the 19 real `.hmml` specs, the three `/hmml-new`
      templates among them. The cause behind the cause was `\n` sitting in `extras`,
      which left the parser no line boundary to resynchronise at. Newlines are now
      significant, so a bad line costs one line; on top of that the five Bevy
      sections, `marker Name`, the access block (a closed key set, highlighted as
      metadata) and wrapped bullet continuations all parse. Sweep is 0 of 19.
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

Done — `just doctor` reports **no gaps** as of 2026-07-26.

- [x] **Dropped the local edit, pulled both repos.** `hobby-tooling` on `main` at
      `3e9507c`, `helix` on `custom` at `582a2dfa`, both clean.
- [x] **`just bootstrap`** — config dir, skills, hooks, shell modules, `paths.env`
      and the generated Helix config are all in place. Discovery found the checkout
      through the `~/.config/helix/runtime` symlink; no `HELIX_REPO` was needed.
- [x] **`just install-lsps`** — all five built and on `PATH`: `claude-lsp`,
      `claude-skills-lsp`, `treesitter-class-lsp`, `path_lsp`, `bevy_asset_lsp`.
      The step that was impossible before the vendoring.
- [x] **Grammars built.** 114 built on the first pass, `hmml` among them. The six
      failures (`gotmpl`, `php`, `ocaml`, `ocaml-interface`, `svelte`, `fsharp`)
      are the expected missing-source ones and are harmless.
- [x] **Rebuilt `hx` — and it turned out to be mandatory here, not optional.**
      The installed binary was built at `20c1dbde`, whose compiled-in
      `languages.toml` still had `grammar = "mml"` on the `hmml` language. Helix
      *merges* the user file over that one, and a merge can add or override a key
      but **cannot delete one** — so `hmml` kept resolving to the work `mml`
      grammar, which does not exist here, and `hx --health hmml` reported
      `Tree-sitter parser: None` even with `hmml.so` built and in place. Only
      rebuilding from `582a2dfa` (where the line is gone) fixed it. Generalise:
      when a change to the checkout's `languages.toml` **removes** a key, every
      machine needs `just helix-build`; the merged user config only carries
      additions.
- [x] **`chobby` + `/login`** — logged in to the private account.
- [x] **Install `zellij`** — 0.44.3, already present via cargo, and the
      `hobby-dev.kdl` layout is linked. `hcdev` can open sessions.
- [x] **Install `gh`** — done, 2.96.0, authenticated as `julianbieber`. `/pr-create`
      and the `/gh-issue-*` skills work now.
- [x] **Reviewed `git -C ~ diff .config/helix/languages.toml`** — nothing usable was
      lost. The diff is almost entirely upstream churn plus this repo's generated
      blocks. The only entries unique to the old committed copy were `helix-gpt`
      attached to rust/python/zig/scala, and `metals` pointing at `run_metals.sh`
      with two extra inlay-hint options; the checkout's copy renamed that to
      `run-metals.sh` and dropped the `gpt` attachments deliberately. **None of
      those three binaries is installed on this machine**, so every one of the
      differences is inert here. Nothing to restore. Remember the file is now
      **generated** — anything worth keeping belongs in the checkout, or in
      `~/.config/helix/languages.local.toml` if it is machine-specific.
- [ ] **Clone the hobby projects you actually want to work on.** `~/fun_repos` holds
      only `helix`, `hobby-mimisbrunnr`, `hobby-tooling` and the freshly cloned
      `bevy_asset_lsp`, so `hwt` still has nothing to make a worktree from.
- [x] **MCP servers no longer need the work checkout.** `just-mcp`, `lsp-mcp` and
      `tree-extract` are vendored into `mcps/` under `hobby-`-prefixed package and
      binary names, so `chobby` runs `hobby-*-mcp` while a bare `claude` keeps the
      work builds — both in `~/.cargo/bin`, neither overwriting the other.
      `just install-mcps` builds all three and registers them at `-s user` scope.
      Still has to be run once per machine — doctor reports it as an optional `-`
      until then.

Not worth tracking: the ~226 *upstream* Helix language servers (`gopls`,
`typescript-language-server`, `lua-language-server`, …) that stock Helix declares
uninstalled. Install per language, on demand. `just helix-servers --all` lists them
if you ever want the full picture.
