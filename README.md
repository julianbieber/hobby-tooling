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

### Overrides

| | |
|---|---|
| `HELIX_REPO` | Helix checkout holding `languages.toml` (default `~/outside_repos/helix`) |
| `HOBBY_NOTES_REMOTE` | where `clone-notes` clones from |

### What needs the work machine

Three MCP binaries (`lsp-mcp`, `julian-tree-extract-mcp`, `just_mcp`) and the
`tree-sitter-mml` grammar are built out of my work tooling repo. On a hobby-only
machine `install-mcps` skips the missing ones by name and `wire-helix` skips
entirely — `hwt`, `hcdev`, and all 18 skills work regardless.

### Helix config convention

`languages.toml` is maintained in the Helix checkout and **copied** into
`~/.config/helix/`, so `wire-helix` edits the repo copy and installs from there —
editing the installed file directly would be lost on the next refresh. Query files
land in the repo's `runtime/queries/`, which `~/.config/helix/runtime` symlinks to.

## What's Here

| | |
|---|---|
| `hwt` | Worktree + task management (nushell). `hwt new`, `go`, `tree`, `done`, `block`, `rm`, `restore` |
| `hcdev` | Zellij dev sessions with Helix + Claude side by side, on the hobby account |
| `chobby` | Bare `claude` on the hobby account |
| `hwt-overview` | Bevy dashboard over all projects/topics/tasks |
| `hobby-mml-lsp` | MML language server, routed to hobby specs by path glob |
| 18 skills | Task loop (journal, resume, spawn, subtask, triage, reflect), MML suite, PR + dev-flow, GitHub issues |

## Account Separation

`CLAUDE_CONFIG_DIR=~/.claude-hobby` isolates login, MCPs, settings, skills, and
history. `chobby` and `hcdev` set it; a bare `claude` is always the work account.

## Related

- [`hobby-mimisbrunnr`](https://github.com/julianbieber/hobby-mimisbrunnr) — the notes repo this writes into
