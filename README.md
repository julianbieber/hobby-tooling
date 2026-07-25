# hobby-tooling

Claude Code tooling for hobby projects, on a private account separate from work.

Adapted from the `julian-*` tools in my work tooling repo, restructured for
single-repo tasks: the project comes first in the path and the repo level is gone.

```
~/fun_repos/<project>                      main checkout
~/fun_worktrees/<project>/<topic>/<task>   the worktree itself
hobby-mimisbrunnr/notes/<project>/<topic>/<task>   notes for that task
```

## Install

```bash
just                 # list recipes
just install         # everything
```

Then log in to the private account once:

```bash
chobby               # then run /login
```

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
