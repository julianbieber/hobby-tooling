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
runs as the `comments` phase of `/task:build`.

It may assert a reason it verified in the code, or one it read in
`business-context.md` — the same two written sources the commit-message rule
below allows, and no third. A contract that needs a *why* neither supplies is
reported as a gap with its `file:line`, never invented.

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

The first two have a written form: `business-context.md` in the task's notes
directory, which `/task:build` fills from my own words before the change is
designed. A passage there counts as having been given the justification, and a
resumed session that has lost the conversation can still read it. Nothing else
does — not the issue, not an earlier journal entry, not your own summary of what
I probably meant.

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

**Three sections, in this order, and nothing else** — above them, when the task has
an issue, the one line that closes it:

````markdown
Closes #12

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

**A PR that has an issue closes it, and the body is the only thing that can.** The
first line is `Closes #<n>` — GitHub then closes the issue the moment the PR merges,
which is the only point at which the work is actually in the default branch. Leaving
it out means closing the issue by hand later, which is to say not closing it. The
issue number is the one in the task or branch name (`gh-12-…`), or the issue the
brief named; a task with no issue writes no line, and never invents a number. The
keyword fires only on a merge into the **default** branch, so a PR based on another
branch has to name the issue again when it is re-targeted. The line is not a section
and does not count against the three.

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

## Task shaping

Rules for carving a project into issues. They bind wherever work is split —
`/task-split` runs the pass, `/gh-issue-create` checks the result before filing,
and `/spawn` and `/subtask` are subject to them too.

**1. Every task has a verifiable impact on user-driven behaviour.** What this
actually forbids is the *layer* task — "add the table", "add the endpoint", "wire
the UI" each fail it alone — so the rule forces vertical slices, and any prefix of
the list is then a shippable system.

**The one escape is labelled.** Genuinely enabling work — a migration, an
extraction, a build change — may declare itself `enabling`, and the price of that
declaration is that it names the task whose user-visible behaviour it unlocks, and
that task exists. No orphan enablers. The two alternatives are both worse:
bundling the enabler into the first feature that needs it triples that task and
stops the review separating mechanical risk from semantic risk, and writing a
fictional user-facing claim on it destroys the one field the rule exists to make
checkable.

**2. The task states the observation that separates done from not-done, and states
it before the work starts.** One concrete thing someone can do, and what they
should see. If it cannot be written, the task is not shaped yet — that failure is
the signal, not an obstacle to route around. Without this rule 1 is unenforceable:
"impact on user behaviour" becomes a genre of sentence rather than a check.

**3. A task is independently revertible.** If backing out #3 forces #5 and #7 out
with it, they were one task split on the wrong axis. This is checkable at merge
time rather than planning time, which is what catches the splits that satisfy
rule 1 on paper and are entangled in fact.

**4. Size is bounded by review, not by effort.** Too big means the diff cannot be
reviewed in one sitting. Effort estimates produce argument; "can one person hold
this diff in their head" produces agreement, and it gets mechanical churn right —
a 40-file rename is large and reviewable, and effort-based sizing misjudges it in
both directions.

**5. No task contains an unanswered question.** A decision that must be made first
is its own item, its output is the decision plus the tasks that follow from it,
and it is explicitly exempt from rule 1. Mixing discovery into execution is what
makes a half-done task sit open for three weeks.

**6. No task leaves state that must be remembered.** No "remove the flag later",
no "the old path stays until #9". Either the cleanup is inside this task or it is
a task that exists now. Rule 1 generates these — a vertical slice often ships
behind a flag — so this is its necessary counterweight.

**No estimates, points, or size field.** Rules 3-5 already bound size by things
that can be checked; a number constrains nothing about shape and is the one field
people optimise against.

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

`hwt sync` is the whole of it, and it takes no flag but `-p <project>`. It acts —
run it on arriving at a machine and on leaving one, and it does the same thing
both times.

Three populations, three treatments:

```
~/fun_repos/<project>       fetch, fast-forward. Nothing else.
a task worktree             commit the dirty tree as `wip`, rebase onto the
                            upstream, push.
the notes repo              a chobby session running /sync-notes.
```

A main checkout is scratch — read, tried, scribbled in — so anything that is not
a fast-forward (dirty, diverged, detached, local commits) is printed for a
decision rather than attempted. A task branch is meant to travel, so it is
committed and pushed without asking; **the wip commit is a real commit on the
branch and has to be squashed or amended before the branch is reviewed.** Its
subject starts with `wip` so it is visible in `git log --oneline`.

A rebase that conflicts is aborted, and the checkout is left as it was found —
except that the working tree is now the wip commit. Resolving it is yours.

The notes repo is handed to `/sync-notes` because its commit messages and its
conflicts are prose — it commits one commit per task directory and stops on a
conflict rather than resolving it. The skill lives in that repo
(`.claude/commands/sync-notes.md`), not in `hobby-tooling`. If `hwt sync` is run
from inside a Claude session it prints the command instead of nesting a second
one.

Stashes are out of scope — they cannot be pushed, and the shared stash stack is a
hazard here anyway. Uncommitted work in a worktree is covered by the wip commit;
in a main checkout it is not, and is reported.

## Tooling

`hwt` manages worktrees/tasks, `hcdev` opens a zellij dev session on this account.
Both from `~/fun_repos/hobby-tooling`. Run `hwt` or `hcdev` with no arguments for
the command list.

`hcdev coordinator <blurb>`, run from `~/fun_repos/<project>`, opens a coordinator
session: the task list (`hcoord tui`) on the left, Claude on the right. In it,
`/task:coordinate <description>` carves the work into tasks and runs each through
`/task:auto` in its own `hcdev auto` window, one at a time. The coordinator lives
at `notes/<project>/coordinator/<blurb>/` — a task with a `task.json` and a
`tasks.json` and no worktree — and its tasks sit under the blurb as their topic.
