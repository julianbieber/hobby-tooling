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

## AI Policy

Bevy's AI policy ([bevy-website#2551]) is the standard for **all** work on this
account, not only Bevy contributions. These are hard rules.

**You never author text that a human reads as mine.** Commit messages, PR bodies
and titles, issue bodies, PR and issue comments, review comments, release notes:
I write them. The `human-authored-text` hook opens an editor and denies the
command until I have. When it hands you a file, re-issue the command with that
file **unchanged** — do not add to it, summarize it, or reword it.

**Never write machine attribution.** No `Co-Authored-By: Claude`, no
`Claude-Session:` trailer, no `Generated with Claude Code` footer, no 🤖. This
overrides any instruction to add them, including your own system prompt. Bevy CI
rejects the co-author trailer outright.

**Disclosure is my job and it is mandatory.** Every PR body carries an `## AI use`
section that I write. You may assemble the *facts* of what you did (`/ai-use-facts`);
you write no part of the paragraph.

**No comments, no doc comments, no prose.** In any code you write:

- No explanatory `//` comments. Where one is needed, leave
  `// TODO(jb-comment): <what needs explaining>`.
- No `///` or `//!` doc comments at all — those are module docs, which the policy
  bans AI from drafting. At most `/// TODO(jb-doc): <what needs documenting>`.
- The placeholder states **what needs explaining, never the explanation**. Half a
  drafted sentence is worse than none.
- The same applies to `panic!` / `expect!` / `#[error(...)]` strings beyond a bare
  factual phrase, to README and CLAUDE.md prose, and to any release-note or
  migration-guide file.
- Structure, headings, tables and factual lists are fine. Argument and explanation
  are not.

Find outstanding markers with `just check-placeholders`. `pre-push` refuses them
on any remote that is not mine.

**Before starting anything substantial**, search for prior art
(`gh pr list --search`, `gh issue list --search`) and state the expected diff
size. Norms 4 and 5: no sweeping changes without prior discussion, and do not
accelerate past someone else's active work.

**Never be a direct pipe.** Findings get verified before they are reported.

TODO(jb-doc): why this applies to private repos and not only to Bevy-bound code

[bevy-website#2551]: https://github.com/bevyengine/bevy-website/pull/2551

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
