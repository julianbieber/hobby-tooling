# Hobby Account — Claude Code Notes

You are running on the **private hobby account** (`CLAUDE_CONFIG_DIR=~/.claude-hobby`).
Hobby projects only. Nothing here relates to work.

## Overrides ~/CLAUDE.md

`~/CLAUDE.md` is the **work** configuration and it still gets auto-loaded, because
CLAUDE.md discovery walks up from the working directory to `$HOME` and cannot be
told to skip a level. Where the two disagree, **this file wins**:

| `~/CLAUDE.md` says | For hobby work, instead |
|---|---|
| Plans go in `~/projects/team-constellation-mimisbrunnr/notes/{topic}/{task}/` | `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/` |
| Task path is `~/worktrees/TOPIC/TASK/project` | `~/fun_worktrees/PROJECT/TOPIC/TASK` |
| Task ID is the second subdirectory under `~/worktrees` | Task is the **third** level: project / topic / task |
| Pre-cached schemas at `~/.claude/context/schemas/` | Not applicable — no databases here |
| Jira for issue tracking | **GitHub issues**, via `gh` |

The CLI tool list in `~/CLAUDE.md` (ripgrep, fd, bat, sd, jaq, just, mise, dust,
difftastic, bottom, …) is accurate and applies here too.

## Paths

```
~/fun_repos/<project>                              main checkout
~/fun_worktrees/<project>/<topic>/<task>           the worktree (is itself a git repo)
~/fun_repos/hobby-mimisbrunnr/notes/
    <project>/<topic>/<task>                       notes for that task
~/fun_repos/hobby-tooling                          this tooling
```

A task directory **is** the worktree — hobby tasks are single-repo. There is no
repo level below the task, and no such thing as a task spanning two repos.

## Plans

When a task needs architectural planning, write it to the task's notes directory as
a dated file. Never `.claude/plans/`.

- **Naming:** `plan-{YYYY-MM-DD}-{short-slug}.md`
- **Location:** `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/`
- **No worktree?** `~/fun_repos/hobby-mimisbrunnr/notes/{project}/misc/`
- **One file per plan** — never overwrite an earlier one; each planning session
  creates a new dated file.
- `mkdir -p` the directory if absent. Don't auto-commit; leave `git add`/`commit`
  to me.

Use zk-compatible frontmatter:

```markdown
---
title: "{task} — Plan: {short description}"
created: {YYYY-MM-DD}
tags:
  - plan
---

## Goal
## Approach
## Steps
## Key Files
## Open Questions
```

## Issue Tracking

GitHub issues via `gh`, never Jira. `gh issue list`, `gh issue view <n>`,
`gh issue comment <n>`. The `/gh-issue-create` and `/gh-issue-update` skills wrap
the common cases.

## Git Identity

Hobby repos commit as `julianbieber <julian.bieber.1992@gmail.com>`, set per-repo.
The **global** git identity is the work one, so a fresh hobby repo needs:

```bash
git config user.name julianbieber
git config user.email julian.bieber.1992@gmail.com
```

## Tooling

`hwt` manages worktrees/tasks, `hcdev` opens a zellij dev session on this account.
Both from `~/fun_repos/hobby-tooling`. Run `hwt` or `hcdev` with no arguments for
the command list.
