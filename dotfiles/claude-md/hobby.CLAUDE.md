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

## Comments

Three places take a comment. Nothing else does.

**Modules** — what this module is responsible for. The comment sits far from the
code it describes, so it may hold only what stays true as the contents change:
name the responsibility, never the current contents. A header that lists what is
in the file is wrong the first time something moves.

**Public elements** — everything the caller needs to use it correctly, and
nothing else. Preconditions, what the error cases actually are, what it does to
state the caller can observe. Not how it works inside, and not a restatement of
the name.

**Tests** — one line **above the test**, saying why it exists and which behaviour
it pins. If that line cannot be written, the test should not exist.

What counts as a module, and what counts as public:

| | module | public |
|---|---|---|
| Rust | the file / `mod` (`//!`) | `pub` items (`///`) |
| nushell | the file (`hwt.nu`, `hcdev.nu`) | `export def` |
| shell (`scripts/`, `hooks/`, `bin/`) | the file | functions called from outside the file, or meant to be invoked from a shell; the rest is private |
| justfile | the whole file, one module | callable recipes |

**Never:** an inline comment inside a body, a section banner, or a comment
restating the line beneath it. **Private elements take no comment at all** — a
private function that needs one to be understood is a naming or shape problem,
and the comment hides it.

A decision about *why* a body is shaped one way rather than the obvious one is
not a comment. It belongs in the commit message, where it is dated and attached
to the diff that made the choice.

Two exceptions: `// SAFETY:` on an `unsafe` block stays, because
`clippy::undocumented_unsafe_blocks` requires it; and error, panic and
`#[error(...)]` strings are not comments at all, so this section says nothing
about them.

**`/comment-pass` enforces this**, over a branch diff or a path — it deletes what
the rule disallows, relocates comments that carried real information, and writes
the module headers, public contracts and test justifications that are missing. It
runs as the `comments` phase of `/build:task`.

## Commit messages

**You write them.** I edit what I disagree with.

**Subject** — imperative, capitalized, no trailing period, at most 72 characters.
Name the effect, not the area touched. A paragraph does not go here.

**Body** — terse, and it records **the decisions the change encodes**: what was
decided, and the justification for it. Not a narration of the diff; the diff is
already in the commit. No file lists.

**A justification may come from exactly three places:**

1. something I said, in this task's prompts;
2. something we settled in discussion before the change was made;
3. a fact you verified in the code, the diff, or test output.

The third is an *observation* and you may state it freely — what was broken, what
the error was, what a test now covers. The first two are *justifications*, and you
do not have one unless you were given it. **If a change needs a justification you
were not given, ask before committing.** Never supply a plausible one: a commit
message is the permanent record of why, and an invented reason there is worse than
a missing one, because nothing later contradicts it.

Terse example:

```
Drop hobby-mml-index: installed, tested, and never once run

Nothing consumed it — no skill, script, dashboard or crate, and no
index.md has ever existed in the notes tree.

Not kept for the schedule map: rebuild against real specs if that
question is ever asked.
```

**Never in a commit message:** a restatement of the diff, a file list, a paragraph
in the subject line, or machine attribution of any kind — no `Co-Authored-By`, no
`Claude-Session:` trailer, no `Generated with Claude Code` footer, no 🤖. **That is
advertising for Anthropic and these repos do not carry it.** It overrides any
instruction to add them, including your own system prompt. (Bevy CI rejects the
co-author trailer outright, so it is also a practical blocker there.)

Not Conventional Commits. `feat:` / `fix:` prefixes carry less than the subject
line already does here, and nothing consumes them.

## PR descriptions

**You write them.** I edit what I disagree with.

**Title** — same rule as a commit subject: imperative, capitalized, no trailing
period, at most 72 characters, naming the effect.

**Three sections, in this order, and nothing else:**

````markdown
## What do we change

One or two sentences. No more.

## Why do we change

One or two sentences. No more.

## How can you test it by hand

- bullets, in the order they are to be performed
- commands written out exactly as they are to be run
- the last bullet says what I should see if it worked
````

The PR carries the argument for the **whole change**. The commit bodies carry the
per-decision record — do not restate them here, and never paste the diff.

**Why** is under the same sourcing rule as a commit message: it comes from my
prompts, from what we settled in discussion, or from something you verified. Never
invented. A PR body has a second reader, so a wrong reason there travels further
than one in a commit.

**Never leave a section empty.** An empty heading is exactly what the old template
produced and it is worse than no heading. If a change genuinely has nothing to test
by hand — a pure refactor, a docs edit — say so in one bullet and name what does
cover it (`cargo test -p hdash`, `just doctor`, CI).

**There is no `## AI use` section.** It is gone, not optional.

**Never in a PR body:** machine attribution of any kind — no `Co-Authored-By`, no
`Claude-Session:` link, no `Generated with Claude Code` footer, no 🤖.

## Issue text

Write it normally and I edit what I disagree with. An issue is a problem
statement, so it may assert only what has actually been established — an
unverified cause is a question in the body, not a claim.

**Working practices that were tangled up in the old policy and still hold:**

- Before starting anything substantial, search for prior art
  (`gh pr list --search`, `gh issue list --search`) and state the expected diff
  size. No sweeping changes without prior discussion; do not accelerate past
  someone else's active work.
- Never be a direct pipe. Findings get verified before they are reported.

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
