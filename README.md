# hobby-tooling

Claude Code tooling for hobby projects, on a private account separate from work.

Adapted from the `julian-*` tools in my work tooling repo, restructured for
single-repo tasks: the project comes first in the path and the repo level is gone.

```
~/fun_repos/<project>                      main checkout
~/fun_worktrees/<project>/<topic>/<task>   the worktree itself
hobby-mimisbrunnr/notes/<project>/<topic>/<task>   notes for that task
```

## Install on a new machine

```bash
git clone git@github.com:julianbieber/hobby-tooling.git ~/fun_repos/hobby-tooling
cd ~/fun_repos/hobby-tooling
just bootstrap       # or: just bootstrap-full  (adds the Rust builds + MCPs)
chobby               # then /login with the private account
```

`bootstrap` is idempotent — re-run it any time. It checks dependencies, clones the
notes repo, sets the per-repo git identity, links everything into `~/.claude-hobby`
and `~/.config`, adds the `use` lines to `config.nu`, and wires the Helix language
entry. Optional pieces are skipped with a named reason rather than failing.

The `/login` is the one step that can't be scripted.

```bash
just doctor          # what's wired, what isn't, and the command to fix each gap
just --list          # every recipe
just uninstall       # remove the symlinks (keeps your login and repos)
```

### Machine-specific paths

Nothing is hardcoded to one machine. `just local-paths` shows what was discovered
and writes `~/.config/hobby-tooling/paths.env`, the one place — outside the repo,
never committed — for a path that discovery gets wrong.

| | |
|---|---|
| `HELIX_REPO` | Helix checkout holding `languages.toml`. Discovered from the `~/.config/helix/runtime` symlink, then `~/{fun_repos,outside_repos,projects,src,repos}/helix` |
| `TREE_SITTER_MML` | grammar checkout for `.hmml` highlighting. No default — it lives outside the hobby tree |
| `HOBBY_NOTES_REMOTE` | where `clone-notes` clones from |
| `BEVY_ASSET_LSP_REMOTE` | where `install-lsps` clones `bevy_asset_lsp` from |

### What needs the work machine

Nothing. A machine with only this repo can build the whole setup.

**HMML** doesn't. `.hmml` gets its language server, diagnostics **and** syntax
highlighting anywhere, because the grammar (`lsps/tree-sitter-hmml`) ships here with
its C parser committed — a C compiler is the only requirement. `TREE_SITTER_MML` is
now purely about the work `.mml` dialect.

**The language servers** don't. `claude-lsp`, `claude-skills-lsp`,
`treesitter-class-lsp` and `path-lsp` are vendored into `lsps/`, so
`just install-lsps` builds them anywhere. `claude-skills-lsp` resolves
`$CLAUDE_CONFIG_DIR` before `~/.claude`, so the one binary indexes the hobby skills
in a hobby session and the work ones otherwise.

**The MCP servers** don't. `just-mcp`, `lsp-mcp` and `tree-extract` are vendored into
`mcps/`, so `just install-mcps` builds and registers them rather than skipping the
missing binaries by name. The work-only servers (db, jira, experts) are deliberately
absent — see `CLAUDE.md` for why each one doesn't apply here.

The two vendored sets differ in one respect, on purpose:

- **Language servers keep their upstream names.** One Helix and one `languages.toml`
  serve both accounts, so a second name would run a second server on every buffer
  rather than isolate anything. Building from here therefore replaces any build from
  the work checkout, and the two copies drift.
- **MCP servers are `hobby-`-prefixed** — package, lib and binary. Each account has
  its own MCP registry, so there is nothing to collide over and a prefix buys real
  separation: `chobby` runs `hobby-just-mcp` / `hobby-lsp-mcp` /
  `hobby-tree-extract-mcp` while a bare `claude` keeps `just_mcp` / `lsp-mcp` /
  `julian-tree-extract-mcp`. Both live in `~/.cargo/bin` and neither overwrites the
  other. The registered *server* names stay unprefixed, so `mcp__tree-extract__*`
  resolves on either account.

### Reading the conversation with ctrl-g

**ctrl-g** in Claude Code opens `$EDITOR` on the prompt buffer. With
`EDITOR=hx-claude` it opens the conversation so far beside it, so you can read
what just happened while writing the next prompt.

The `claude-conversation` hook rewrites
`/tmp/claude-conversation-$ZELLIJ_SESSION_NAME.md` after every tool call and at
the end of every turn; `hx-claude` vsplits it next to the prompt buffer, at its
last line. Both halves ship here (`hooks/claude-conversation/`, `bin/hx-claude`),
so this works on a machine that has only this repo. `just doctor` names either
half if it's missing, and warns if `$EDITOR` doesn't point at the wrapper — the
one part that lives in your shell profile rather than in this repo.

### Helix config convention

Helix merges `~/.config/helix/languages.toml` over the `languages.toml` compiled
into the binary, so there are two inputs and one generated output:

| file | who owns it |
|---|---|
| `<checkout>/languages.toml` | machine-**neutral**, committed, shared by every machine, and what the binary is built from |
| `~/.config/helix/languages.local.toml` | machine-**specific**, generated by `wire-helix`, never committed |
| `~/.config/helix/languages.toml` | the two concatenated — an **output**, so editing it is the one thing that doesn't work |

The split exists for one line. `[[grammar]] name = "mml"` needs an absolute
`source.path`, which differs per machine and is absent on hobby-only ones —
and `helix-term/build.rs` builds grammars with `STRICT = true`, so one bad path
fails the entire `hx` build everywhere else. Keeping it out of git and generating
it per machine makes every machine correct with no per-machine edits to undo.

`just helix-build` rebuilds `hx` itself with the flags that make that work, and
`just helix-servers` lists which language servers `languages.toml` declares and
which are actually installed here. Query files land in the checkout's
`runtime/queries/`, which `~/.config/helix/runtime` symlinks to.

## What's Here

| | |
|---|---|
| `hwt` | Worktree + task management (nushell). `hwt new`, `go`, `tree`, `done`, `block`, `rm`, `restore` |
| `hcdev` | Zellij dev sessions with Helix + Claude side by side, on the hobby account |
| `chobby` | Bare `claude` on the hobby account |
| `hx-claude` | `$EDITOR` for ctrl-g — Helix on the prompt buffer, conversation in a vsplit |
| `hdash` | Bevy dashboard over every work item — GitHub issues, the worktrees they become, PRs, done. One card per piece of work |
| `hobby-mml-lsp` | HMML language server, routed to `.hmml` specs by file type |
| 4 vendored LSPs | `claude-lsp`, `claude-skills-lsp`, `treesitter-class-lsp`, `path-lsp` — buildable without the work checkout |
| 18 skills | Task loop (journal, resume, spawn, subtask, triage, reflect), HMML suite, PR + dev-flow, GitHub issues |

## Account Separation

`CLAUDE_CONFIG_DIR=~/.claude-hobby` isolates login, MCPs, settings, skills, and
history. `chobby` and `hcdev` set it; a bare `claude` is always the work account.

## Related

- [`hobby-mimisbrunnr`](https://github.com/julianbieber/hobby-mimisbrunnr) — the notes repo this writes into
