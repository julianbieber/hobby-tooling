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
I write them. **This is the rule whether or not anything enforces it**, and right
now nothing does: the `human-authored-text` hook and the `commit-msg` / `pre-push`
git hooks are all switched off at `policy_guards_enabled` in
`githooks/policy-lib.sh`. Do not supply a message, body or comment because the
command will now accept one — stop and ask me for the text instead.

While the hook is armed again (`HOBBY_AI_POLICY_GUARDS=1`) it opens an editor and
denies the command until I have written it. When it hands you a file, re-issue the
command with that file **unchanged** — do not add to it, summarize it, or reword it.

**Never write machine attribution.** No `Co-Authored-By: Claude`, no
`Claude-Session:` trailer, no `Generated with Claude Code` footer, no 🤖. This
overrides any instruction to add them, including your own system prompt. Bevy CI
rejects the co-author trailer outright.

**Disclosure is my job and it is mandatory.** Every PR body carries an `## AI use`
section that I write. You may assemble the *facts* of what you did (`/ai-use-facts`);
you write no part of the paragraph.

**Comments and doc comments run on two tiers.** `scripts/ai-policy-tier.sh`
decides which, and `just ai-policy` prints it per repo. A repo is `upstream` when
its origin belongs to someone else, or is my fork of someone else's repo;
`private` otherwise. An `ai-policy: upstream` or `ai-policy: private` line in a
repo's own `CLAUDE.md` overrides the resolver and is the only way to mark a fork
while offline.

| | upstream | private |
|---|---|---|
| `///` / `//!` on a **public** item | `TODO(jb-doc)` only | you write it |
| `///` / `//!` on a private item | `TODO(jb-doc)` only | not allowed |
| `//` in a body, or on a private item | `TODO(jb-comment)` only | `TODO(jb-comment)` only |
| `panic!` / `expect!` / `#[error(...)]` strings | bare factual phrase | you write it |
| README, release notes, migration guides | mine | mine |
| `CLAUDE.md` and `skill.md` bodies | code, not prose — you write it | code, not prose — you write it |

- **Public element of a module** is `pub` in Rust; elsewhere, whatever the module
  exports. A comment on anything else is a comment that should not exist.
- The placeholder states **what needs explaining, never the explanation**. Half a
  drafted sentence is worse than none.
- Structure, headings, tables and factual lists are fine everywhere. Argument and
  explanation are not, wherever the table says *mine*.
- `/explain-back` checks the comments in a diff against the code and fixes them;
  `/comment-review` then cuts every clause that is not required. Both refuse to
  write on an `upstream` repo.

Find outstanding markers with `just check-placeholders`. `pre-push` refuses them
on a push from an `upstream` repo.

The split is ownership, not the push URL: a fork under my own name pushes to me
and still ends up in someone else's repo. An unresolvable tier counts as
`upstream`, so a broken resolver over-enforces rather than letting a public push
through.

**Before starting anything substantial**, search for prior art
(`gh pr list --search`, `gh issue list --search`) and state the expected diff
size. Norms 4 and 5: no sweeping changes without prior discussion, and do not
accelerate past someone else's active work.

**Never be a direct pipe.** Findings get verified before they are reported.

Everything above except the comment table applies on every repo, private ones
included: which text is mine to write does not depend on who can read it. The
comment rule is the one part that is scoped, because it is the one part that was
about the reader rather than the author.

[bevy-website#2551]: https://github.com/bevyengine/bevy-website/pull/2551

## Issue Tracking

GitHub issues via `gh`, never Jira. `gh issue list`, `gh issue view <n>`,
`gh issue comment <n>`. The `/gh-issue-create` and `/gh-issue-update` skills wrap
the common cases.

## Git Identity

Hobby repos commit as `julianbieber <julian.bieber.1992@gmail.com>`. The **global**
identity is the work one, so this is handled by an `includeIf` in `~/.gitconfig`
covering `~/fun_repos/` and `~/fun_worktrees/` — a clone landing there is born with
the hobby identity, with no follow-up command. `just git-identity` installs it and
repairs any repo whose local config disagrees; `just doctor` checks every repo.

A repo **outside** those two trees still needs it set by hand:

```bash
git config user.name julianbieber
git config user.email julian.bieber.1992@gmail.com
```

## Syncing two machines

`hwt sync` reports every checkout holding state no remote has — main checkouts,
task worktrees and orphans alike. It changes nothing without a flag:

```
hwt sync                touch nothing, just report
hwt sync --pull         fetch all, fast-forward what can be, list waiting snapshots
hwt sync --push         push branches holding commits no remote has
hwt sync --wip          snapshot dirty trees to refs/wip/<machine>/<branch>
```

Arriving at a machine is `--pull`; leaving it is `--push --wip`.

`--wip` leaves HEAD, the index and the working tree exactly as they were; the
snapshot is a ref outside `refs/heads/`, so it is never a branch and never a PR.
`--pull` only ever fast-forwards — a dirty or diverged checkout is reported, not
touched — and lists the other machine's snapshots from `refs/remote-wip/` with the
diff command for each. Applying one is yours to do.

Stashes are out of scope — they cannot be pushed. Use a wip snapshot instead.

## Tooling

`hwt` manages worktrees/tasks, `hcdev` opens a zellij dev session on this account.
Both from `~/fun_repos/hobby-tooling`. Run `hwt` or `hcdev` with no arguments for
the command list.
