# Hobby Tooling

## What This Repo Is

Personal Claude Code tooling for hobby projects, running under a **separate Claude
account** from work. The private-account counterpart to
`team-constellation-tooling`. Adapted from that repo's `julian-*` tools, but
restructured for single-repo tasks.

## The Structural Difference That Explains Everything

At work, `~/worktrees/TOPIC/TASK/` is a **container** holding several repo
worktrees, because a work task often spans Patagona-API + request-worker + infra.

Here, `~/fun_worktrees/PROJECT/TOPIC/TASK/` **is** the worktree. Hobby tasks are
single-repo, so the project moves to the front of the path and the repo level
disappears.

Consequences, all of which make this tooling smaller than its work counterpart:

- No `wt add` — a task can't gain a second repo
- `task.json` is just `{ done, blocked }`, plus `backlog` once hdash has set one; no `repos[]`, since the project is in the path
- `hwt go <task>` takes no project argument
- A task directory is itself a git repo, so Claude gets branch + git status for free
- No experts (one repo means one expert, and a third path level strains the
  108-byte Unix socket limit)

## Layout

```
dotfiles/
  nushell/{hwt.nu, hcdev.nu}          # symlinked into ~/.config/nushell/
  zellij/{config.kdl, layouts/hobby-dev.kdl}   # symlinked into ~/.config/zellij/
  claude-md/                          # symlinked into ~/.claude-hobby/ and ~/fun_worktrees/
bin/{chobby, hauto, haspect, happ, hresearch, hx-claude, zj-notes}   # symlinked into ~/.local/bin/
hooks/<name>/hook.sh                  # symlinked into ~/.claude-hobby/hooks/
skills/<name>/{skill.md, README.md}   # symlinked into ~/.claude-hobby/commands/
settings/hobby.settings.json          # symlinked to ~/.claude-hobby/settings.json
scripts/                              # install logic too involved for a justfile recipe
lsps/hobby-mml-lsp/                   # forked HMML language server + grammar.md
lsps/{claude,claude-skills,treesitter-class,path}-lsp/   # vendored from the work checkout
mcps/{just-mcp, lsp-mcp, tree-extract}/   # vendored from the work checkout
tools/hdash/                          # the work dashboard and htab, the Claude-mode overview pane (ratatui)
tools/hcoord/                         # the coordinator's task-list pane + `hcoord wait` (ratatui)
tools/hatlas/                         # a Bevy app's linked HTML atlas, from rustdoc JSON
```

## An issue is a notes file, because these repos have one author

An issue is `issue.md` beside `task.json` in `notes/<project>/<topic>/<task>/` — zk
frontmatter (`title`, `created`, tag `issue`) and the problem statement, with dated
entries appended under `## Progress`. `/issue-create` writes or refreshes one,
`/issue-update` appends progress, and `hdash` shows the `title:` on the task's card.
There is no GitHub issue behind it and nothing reads one.

**It used to be a GitHub issue, and that was double bookkeeping.** Every piece of
work with an issue was recorded twice — the issue said what the work was, `task.json`
said it existed — and most of the machinery around the two existed only to make them
agree: a `topic:<name>` label to supply the one path component an issue could not
imply, a match from worktree name to issue number that had to accept three
historical spellings, untagged-issue cards that could not be launched, and a
`backlog` label mirrored against `task.json` with a rule for which side wins. These
repos are worked on alone, so the second record coordinated nothing. It was
dropped, not reconciled.

The open issues were migrated rather than abandoned: each got `issue.md` in its
task's notes dir (seven of them a new notes dir, named `gh-<n>-<slug>` like the rest
of their generation), carrying the body and comments verbatim and a `github:` line
naming the original, which was then closed with a comment pointing at the notes.

**New tasks are plain slugs.** `gh-<n>` had a number to point at; a notes entry does
not, and a per-project counter would be a second thing to keep in step for no reader.
Existing `gh-<n>-…` names stay — a name is a path and a branch, and renaming either
buys nothing.

## The notes repo is what makes a task exist

A task exists because `notes/<project>/<topic>/<task>/task.json` exists. Not because a
directory under `~/fun_worktrees` does — that tree answers one question only, *is it
checked out*, and is never enumerated to find out what work there is.

| | source | answers |
|---|---|---|
| a task exists | `notes/P/T/TASK/task.json` | which tasks there are |
| a task is checked out | `~/fun_worktrees/P/T/TASK` | Active vs Removed |
| done / blocked / backlog | `task.json` | state |
| what the work *is* | `notes/P/T/TASK/issue.md`, when there is one | title, problem, progress |

`hdash` and `hwt` apply the same rule from the same file — `scan_tasks` reads the notes
tree, `known-tasks` reads the notes tree — so `hwt list | length` and the dashboard's task
count agree. They did not before: `hwt` treated the *directory* as the marker and `hdash`
the `task.json`, which is why `hwt forget` (whose whole job is removing that file) dropped
a task from one tool and not the other.

A worktree with no `task.json` behind it is an **orphan**: on disk, recorded nowhere.
`hwt orphans` lists them; `hdash` gives each one a card at the top of the list with an
**Adopt** button that writes the file. Nothing adopts silently — a scan never writes.

**Issue-less tasks stay first-class.** `hwt new` writes no `issue.md`, and a notes dir
with only `task.json` is a complete task.

The notes repo decides because it is the only one of the two that survives everything.
A worktree is deleted the moment the work pauses — `hwt rm` is routine — so a tree that
recorded existence would lose a task every time one was checked out somewhere else, or
on a second machine. The notes dir is committed and shared; the worktree is scratch.
Enumerating the worktree tree would also answer a different question than the one asked:
it lists what is on this disk, not what work there is.

### `backlog` parks a task by choice

A backlogged task is parked by choice — unlike `blocked`, nothing external is awaited —
and the mark exists to keep `hdash` readable: the normal list hides backlogged open work,
and **Show backlog** switches to a list of exactly that, done work excluded. It lives in
`task.json` (`"backlog": true|false`, after `blocked`) and nowhere else; the card's
**Backlog** button writes it. The `hwt` verbs never set `backlog` but carry it through
every rewrite, because a dropped key would lose the mark.

## Two machines: what is actually at risk, and `hwt sync`

Work moves between two machines, so anything a remote has never seen exists on one
disk. `hwt status` did not answer that question — it reads `hwt list`, which is
task-driven, so it sees only the worktrees.

**The worktrees were never the leak.** 34 of 37 are clean, with an upstream, zero
commits ahead: the PR flow pushes them as a side effect of how a task is worked.
Every unsynced byte was in the **main checkouts** under `~/fun_repos`, which no
command enumerated. `hwt sync` scans all three populations — main checkouts, task
worktrees, and orphans — because the last two are cheap to include once the walk
exists, and an orphan is by definition recorded nowhere else.

Four states, and they now say what a checkout needs rather than what flag reaches
it:

| state | means | what happens |
|---|---|---|
| `unsynced` | commits no remote has, or a dirty tree | a worktree is committed and pushed; a main checkout is reported |
| `no-remote` | commits, but nowhere to push them | reported, with the `gh repo create` |
| `unborn` | zero commits; every file untracked | reported; a first commit is a decision |
| `skipped` | `hobby.sync=false` in that repo | — |

`unborn` is its own state because it is not a degree of `unsynced`: five repos here
were `git init`'d and never committed, so `.gitignore` and `Cargo.toml` are
untracked alongside `src/`. Reporting that as "12 dirty files" would have buried it.

The opt-out is `git config hobby.sync false`, in the repo's own `.git/config`
rather than a marker file — a marker file in the working tree would itself show up
as untracked, i.e. as the thing it is meant to silence. It does not travel between
machines, which is correct: whether a scratch checkout exists at all is a fact
about one disk.

### Three populations, three treatments, and no flags

The first version had `--pull`, `--push` and `--wip`, and a bare `hwt sync` that
only reported. That put the decision in the wrong place. **Which of those a
checkout needs is a fact about the checkout, not a thing to be chosen at the
prompt** — a main checkout wants exactly a fast-forward, a task worktree wants
exactly commit-rebase-push, every time, in both directions of a trip. Choosing
per run meant choosing wrong: `--push --wip` on leaving would push a main
checkout's scratch commits, and `--pull` on arriving never rebased a worktree at
all.

So the treatment is keyed on the population and the only flag left is
`-p <project>`:

| | treatment | because |
|---|---|---|
| `~/fun_repos/<project>` | fetch, fast-forward, nothing else | a clone here is read, tried and scribbled in; its local commits and dirty files are scratch, not work in flight |
| a task worktree or orphan | wip-commit the tree, rebase onto the upstream, push | a task branch exists to become a PR, so it is meant to travel |
| the notes repo | a `chobby` session running `/sync-notes` | its commit messages and its conflicts are prose |

Anything a main checkout cannot fast-forward — dirty, diverged, detached, holding
local commits — is named in the summary with the command that would fix it. That
is all that survives of the old report-only mode, and it is the part that was
load-bearing: it is where every unsynced byte turned out to be.

### The wip commit is on the branch, and the refs are gone

`--wip` used to write the working tree to `refs/wip/<machine>/<branch>`, outside
`refs/heads/` so a snapshot could never become a branch or a PR by accident, and
`--pull` fetched the other machine's into `refs/remote-wip/` and listed them with
a diff command.

**That is deleted, and a dirty worktree is now committed onto its own branch**,
with `wip` as the first word of the subject. The reason is that the ref could not
participate in the rest of the treatment: a rebase refuses to run over a dirty
tree and a push carries branches and nothing else, so a snapshot beside the branch
left the branch exactly as unsynced as before and required a person to notice a
list, read a diff, and apply it by hand. Every step of that was optional, and the
optional steps are what does not happen on the day you switch machines.

The price is real and is stated where it will be read: a `wip` commit is in the
branch's history and has to be squashed or amended before review. It is
`--no-verify`, because a repo's commit hooks lint what a person wrote and this is
a machine picking up a tree mid-edit; a failing hook must not block the sync.

Nothing was lost in the deletion — a sweep of every checkout found zero refs under
`refs/wip` or `refs/remote-wip`. The mechanism was built, documented, and never
once used, which is itself the argument: the path that needs a person to run a
second command after reading a report does not get run.

A rebase that conflicts is **aborted**, not left in progress. What touches a
worktree next is an editor or a Claude session, and neither expects to arrive
mid-rebase.

**Stashes remain out of scope.** They cannot be pushed at all, so no amount of
tooling reaches them, and `CLAUDE.md` already treats the shared stash stack as a
hazard rather than a place to keep things. Uncommitted work in a worktree is now
covered by the wip commit; in a main checkout it is reported and left alone.

### The wip message is a pointer, not a summary

A wip commit's subject is a file count and the machine; its body says the commit
is a snapshot and must be squashed, and — for a task worktree — names that task's
notes directory. It never describes the change. The notes are synced too and are
where the prose lives, so the message points at them instead of paraphrasing them.

`<machine>` is `$HOBBY_MACHINE`, falling back to the short hostname. In the ref
scheme it was a collision key; here it is the only thing in the commit that says
which disk the uncommitted work was sitting on when the sync picked it up.

### The notes repo is a session, not a git command

Every other checkout is synced by rules that need no judgement — fast-forward or
don't, rebase or abort. The notes repo breaks that in both directions, so
`hwt sync` hands it to `chobby` running `/sync-notes` and touches it no further.

**Its commit messages read the files.** The convention is
`<project>/<topic>/<task>: <what these notes are>`, one commit per task
directory, which makes `git log --format=%s` the list of what has been worked on.
A tree of `wip @ laptop: 3 modified` would destroy exactly that, and a file count
is all a shell function can honestly write.

**Its conflicts are prose, and the skill refuses them.** Two machines writing a
journal entry for the same day produce a both-modified file that no merge driver
can resolve, so `/sync-notes` stops and reports rather than auto-resolving. That
is the right call for the same reason the wip commit is not: a wrong resolution
here loses work with no test failing and no compiler complaining.

**The skill is not in this repo.** It is `.claude/commands/sync-notes.md` in
`hobby-mimisbrunnr`, project-scoped to the repo it operates on, which is the one
place it is ever invoked from — `hwt sync` starts the session in that directory.
Nothing here installs it, and `install-skills` does not see it.

Run from inside a Claude session, `hwt sync` prints the command rather than
nesting a second agent in the first.

### The identity leak this surfaced

Scanning the same tree turned up ~600 commits across 29 repos authored as the work
email, 11 of those repos public. The cause was structural: the **global** git
identity is the work one, and `just git-identity` set the hobby identity on exactly
two repos — this one and the notes repo. Every other clone inherited work identity
and nothing ever said so.

The fix is an `includeIf` on `gitdir:~/fun_repos/` and `gitdir:~/fun_worktrees/`
pointing at `~/.gitconfig-hobby`, so a repo is born with the right identity because
of **where it lands** rather than because a recipe was re-run after cloning.
Per-repo local config is still written on top, so a checkout moved out from under
those paths keeps the identity. `just doctor` now checks every repo under
`~/fun_repos` instead of two.

History was not rewritten: that means a force-push across public repos, and it is
a separate decision from stopping the bleeding.

## One dashboard, because there was only ever one list

`tools/hdash` replaces `hwt-overview` (the worktrees that exist) and `hgh-issues`
(the issues that have none yet). They were the same app over two halves of one
list: `hgh-issues` walked `~/fun_worktrees` to decide whether an issue had a
worktree, `hwt-overview` drew that worktree with no idea an issue existed, and
~370 lines of chrome, sidebar, channel plumbing and card recipe were duplicated
per app.

It began as a join of GitHub issues to the tasks they became, matched by issue number;
with issues now in the notes (above) the join is gone and every card is a task. One card
carries a piece of work from its notes entry through the worktree and its PRs to the day it
is marked done, with the primary button always the next step: **Start task** (no worktree
and no branch yet — a fresh `/issue-create`) → **Open hcdev** → **Restore** if the worktree
was removed. Start and Restore run the same `hwt restore` + `hcdev wt`; what separates the
two stages is whether a branch named after the task exists in `~/fun_repos/<project>`. The
card's title is `issue.md`'s `title:` when there is one, else the task name.

Three consequences worth knowing:

- **An orphan outranks everything but a session waiting on you.** A worktree the notes
  repo has never heard of sorts above even a lifted block — only an open task whose Claude
  session is idle sorts higher — and no toggle can hide it; a filter chip that is on can.
- **Topic is not a grouping level.** The list is flat and globally sorted by how much
  attention an item wants; project and topic are printed on the card.
- **`show_done` off is a load-bearing default** — there are ~25× more done tasks than
  open ones.

The sidebar lists the projects that have a notes directory. It used to list every clone in
`~/fun_repos`, because a row was the only way to reach a project's issues; `~/fun_repos`
holds upstream clones too (`bevy`, `helix`), and with no issues to reach a row for them
would be empty.

With one workspace there is no cross-workspace path dependency, so `cargo fmt`/`clippy`
do not reach sideways and editing `hwt_core` needs one workspace's tests, not two.

### It runs in the terminal, and it watches the sessions

`hdash` is a ratatui app ported from the work `wt-overview`, which made the same move
first. Started without a terminal it re-runs itself in a fullscreen Alacritty titled and
classed `hdash`; warnings go to `~/.cache/hdash/hdash.log`, never to the screen. With the
port came what the work version had grown and this one had not: the list rescans by
itself when the notes or worktree trees move, a fuzzy filter narrows the project sidebar,
filter chips narrow the list (including by the `/task:build` / `/task:auto` phase in
`build-task.json`), and the primary button reads **Focus** when a window already shows the
task's session — it activates that window, so leftwm switches to its tag — or **Attach**
when the session runs without one.

A card also carries the session's **live plan**: `todo` and `waiting_on` from the same
`build-task.json`, which `/task:build` and `/task:auto` keep current — a `▸ Todo (3/7)`
toggle with the active step beside it, `waiting: <line>` in amber while a gate holds, and
the whole list when expanded. It is the hobby half of work's `flow` record: the todos and
the waiting line came over, the aspects did not as they were, because a second PR here is
a code aspect (decided at GATE S, or added with `/aspect`) and the card already lists those. One session writes the file, so it
needs none of work's `wt flow patch` locking.

Cards carry an `idle 12m` / `busy 3m` badge from `hooks/session-state`, which writes one
file per hobby-account Claude session under `$XDG_RUNTIME_DIR/hobby-session-state`. **Every
Claude process counts** — a plain hcdev pane, both `/task:auto` stages, a coordinator's
pane — and a record belongs to the task whose zellij session it ran in, falling back to the
worktree its directory lies in; the newest live record per task decides the badge. The
directory is not the work hook's `claude-session-state`, so neither dashboard reads or
deletes the other's records. An idle session on an open task sorts first.

Task names never reach a nu command line as text: a launch passes them as
`HDASH_PROJECT`/`HDASH_TOPIC`/`HDASH_TASK` to a fixed script, and the zellij session names
come from one batched `hcdev-session-names` call into `hcdev.nu` — still the only place a
session name is spelled.

**Installing never needs tests** — the committed code is already tested, so
`install-dash` builds and replaces the binary and nothing more.

## A coordinator is a task with no worktree, and `tasks.json` is its whole state

`/task:auto` and `/task:coordinate` exist for work where I care about what the
change does and not how it is built. The first runs a task in two processes —
a headless **plan** stage that reads the code once and writes `plan.md`, then a
fresh **build** stage that implements from the plan alone — with two gates:
acceptance criteria at the start, only if the brief does not already say what
done looks like, and the running application at the end, before the PR opens.
The second works a list of such tasks one at a time from a **coordinator
session**: `hcdev coordinator <blurb>` run from `~/fun_repos/<project>`, a
zellij session with `hcoord tui` on the left and `chobby` on the right.

**Two processes per task, because context was the cost.** The first version ran
`/task:build`'s whole pipeline in one session — HMML baseline, annotated delta,
the multi-lens spec review, then the implementation on top of all of it — and
one task used up a session. The spec review loop and the planning conversation
carried into the implementation were the two largest costs, and neither buys
anything when the UX is what matters. So `/task:auto` has no HMML, and `hauto`
(`bin/hauto`, the Claude pane of the `hobby-auto` layout) starts the plan stage
as `chobby -p` and the build stage as a fresh `chobby` once the phase is
`planned`. A session cannot clear another session's context — a cross-session
message is delivered as text and a slash command in it is never executed — so
a new process is the only clear there is, and it costs nothing because the
window had to be opened anyway. The build stage runs on `model` from the state
file, `sonnet` when a coordinator seeded it, since a plan that names the files
and the steps leaves mostly mechanical work.

**The coordinator knows nothing about any task.** It never reads a task's code,
plan, diff or journal; planning happens in the task's own window. What reaches
it is a phase name from `build-task.json`, the acceptance result beside it, and
a PR URL from `prs.json`, which is what lets one coordinator session work a
whole list.

**The coordinator is a task under the reserved topic `coordinator`.** Its notes
dir `notes/<project>/coordinator/<blurb>/` has a `task.json`, so `hwt`, `hdash`
and the journal skills all see it without a new kind of thing existing — and no
worktree, because it works other tasks' worktrees rather than one of its own.
The three places that would otherwise treat it as a removed task each know the
topic by name: `hcdev wt` and `hwt restore` refuse it and name
`hcdev coordinator`, and `hdash` gives it a `Coordinator` stage whose primary
button is **Open coordinator** and whose card shows the counts from
`tasks.json`. The topic is a constant in each of the three, not a config.

**`tasks.json` is the record and the display, and there is no channel between
them.** `/task:coordinate` writes the file at every change; `hcoord tui`
re-reads it whenever its mtime moves; `hdash` counts its `status` fields. A
resumed coordinator picks up from it after an interrupt, and editing it by hand
between tasks is expected. The tasks it creates sit under the blurb as their
topic (`~/fun_worktrees/<project>/<blurb>/<blurb>_<slug>`), so a coordinator's
work groups itself.

**The coordinator waits on an idle notice, and falls back to a shell call.** A
Claude session cannot sleep except through a tool. The task's window is a peer
session on this machine, so the coordinator subscribes with `SendMessage`
(`notify_when_idle`, no message) and gets one notice when that session ends a
turn — a gate or `done` — for free. It never sends the session text. Before the
peer is listed, and after a resume, `hcoord wait <task notes dir>` blocks on
the task's `build-task.json` and returns when the phase is one a person answers
(`await-*`) or terminal. That phase naming is the contract that keeps the two
sessions independent: `/task:auto` names its gates `await-acceptance`,
`await-play` and `await-fix`, and nothing else in either skill has to know the
other exists — the third was added without touching `hcoord` at all, because
`await-` is the whole of the *declared* contract for an `auto: true` state file.
Only a state file without `auto` — a `/task:build` session — also stops on that
pipeline's two waiting states, `context` and `assess-split`; an unattended run
never writes them, so `hcoord` counts them only there. A `play` phase left untouched
for three minutes is a backstop under it, for the one case the naming cannot
cover: the skill's write to `await-play` and its printed report are two halves
of one step (#45), and a session that drops the write leaves nothing named
`await-*` for `hcoord wait` to see.
`--after <phase>` is the second call on the same gate, so a reported
`await-play` is not reported again at once. The timeout is 570 seconds because
a Claude Code shell call gets ten minutes.

**The file carries `written` and `phase_since`, because mtime cannot tell the
two rewrites apart.** A phase that has not moved for an hour is either a
healthy session doing something slow or a dead one, and the only distinguishing
signal available to a script was sampling `/proc/<pid>/stat` for CPU ticks and
reading a pane it cannot actually read (#46) — both unavailable to `hcoord`, and
neither reliable. The file's own mtime looked like a substitute, but a write
that leaves the phase unchanged moves the mtime exactly as much as one that
advances it, so mtime alone cannot distinguish them. `written` records every
write; `phase_since` records only the ones that changed the phase; `hcoord wait`
and `hcoord status` both read the gap between them.

`hcoord merged <pr>` is the same shape one layer further on: it polls `gh pr
view` and returns `merged`, `blocked` with a reason, or `timeout` — plus
`mergeable`, only with `--ready`, which is how a task session learns it may run
`gh pr merge` on its own PR. The coordinator calls it without the flag, and only
for a PR whose session finished without merging.

**The coordinator layout is rendered per session, not installed.** The TUI pane
needs the notes dir as an argument and KDL cannot expand anything, so a
committed layout could only ever show one coordinator. `hcdev coordinator`
writes `hobby-coordinator-<session>.kdl` into the runtime dir and hands zellij
the path. `hobby-auto.kdl` *is* installed: it is `hobby-dev` with the Claude
pane running `hauto`, which is how a window a coordinator spawns starts working
the moment it exists.

**A coordinator's windows go to one desktop.** `hcdev auto --window` gives its
alacritty the WM_CLASS `hcdev-auto`, and the window manager does the rest: in
leftwm that is a `window_rules` entry on that class with a `spawn_on_tag`. The
rule lives in `~/.config/leftwm/config.ron`, which this repo does not manage,
so it is a per-machine line, not an install step.

**One task session at a time is the design.** Two `/task:auto` sessions on one
repo would race on the default branch and on my attention, since a gate assumes
one window is being looked at.

**A task is not done until its PR is merged.** `done` used to mean *the session
ended*, and a task whose dependency was still open was based on that
dependency's branch — a stack that had to be merged in order and rebased
whenever a review changed anything early in it. Now the list holds in
`awaiting-merge` between the session ending and the PR landing, and the stack
is gone with it: every task branches off the default branch, because everything
before it is already there.

**An unattended task merges its own PR, and there is no other option.** The
task's **own session** merges once its checks are green and its acceptance
passed: every seed writes `merge: coordinator` into its `build-task.json`, it
watches its own checks with `hcoord merged --ready`, merges, confirms the merge
landed, runs `hwt done` on its own task — so hdash's card closes with the PR —
and reports `done` with the merge already made. `tasks.json` no longer carries a
`merge` choice, so a list `/task:goal` wrote needs no GATE L before it is
worked. Both decisions are mine, recorded in
`notes/hobby-tooling/skills/coord-contract/business-context.md`
(`## one-merge-option`, `## task-done-owner`). `hcoord merged` decides green
from `gh`'s `statusCheckRollup`, and a repo with no checks at all counts as
green: most hobby repos have no CI, and the alternative is a list that never
moves.

**A decision the session cannot make holds the list on that task.** A conflict,
a `gh` refusal, a red check `fix-ci` (below) may not repair, or an acceptance
that would not pass is `await-fix`, with the reason in `ci_result`,
`merge_result` or `acceptance_result`. `hcoord wait` names the one still
standing as `decision` on its line, beside the PR, and the coordinator marks
the card `blocked` with `requires decision: <decision>` and waits on with
`--after await-fix`. I answer in the task's window, the session carries on to a
merged PR, and the list moves (`## leave-await-fix`). A resumed coordinator
re-checks every `blocked` entry, so a PR merged by hand releases its
dependents.

**A red check on a task's PR is that task's own diff, and its own session is
the one holding the branch, the plan and the log** — handing it to a
coordinator that deliberately knows nothing about the task would be the wrong
place to fix it. `hcoord ci` separates the failure that is the diff's own from
the three that are not: the base branch is already red there, a pinned
revision now needs re-pinning (a project decision, not a repair), or the
runner died (a bounded rerun, not a code change). The bound is three attempts
and lives in the binary, not the prompt, so a session that loses count still
stops. The mechanism is inert in a repo with no CI, which is every repo here
— recorded in `business-context.md` `## ci-scope`, adding CI to
`hobby-tooling` would change the merge gate for every task behind it, so it
is deliberately not part of this change.

**`/task:auto` commits once, after its last gate.** A PR cannot open without
commits and nobody is at the keyboard between the last gate and the PR, so the
pipeline commits once after it, to the same message rule. `/task:build` commits
too, after GATE 2, where the proposed commit and PR messages are shown with the
local changes. Neither commits the notes repo.

**An unattended run has no play gate; it runs its own acceptance.** A gate is
worth having because a person reads what it prints, and in a coordinator's task
window nobody does: the window is 800x600 and off-screen and the Claude pane is
about seventeen columns wide, so a report there is not merely unread but
unreadable, and `continue` gets answered having read none of it. A task shipped
that way with four of its six scenarios failing. So with `auto: true` the plan
stage writes the acceptance observation as **commands**, a `verify` phase runs
them, and a failure is the session's own to fix for at most three rounds. The
run either opens a PR with its checks passing or reaches `await-fix` with none.

The report is replaced rather than improved because the property that matters is
not who looks: a failing command stops the pipeline by itself and a printed
report does not. The rounds are capped because a repair loop nobody can
interrupt is the same failure wearing different clothes, and the count is
recorded so a run that keeps spending all three is visible.

What the checks did is written to `build-task.json` as `acceptance_result` —
command, exit status, per-check verdict — and `hcoord wait` puts it on the line
it already returns. **Evidence has to arrive as data or it does not arrive**,
which is the same argument `tasks.json` makes one level up. The acceptance gate
survives unattended: a done-when that cannot be written as commands stops at
`await-acceptance` and asks, because a check that passes while the observation
fails is worse than no check. `auto` is set only by a coordinator, so a
hand-typed `hcdev auto` keeps the play gate and the running application.

`hcoord` is its own workspace rather than a member of `tools/hdash`'s, and `hdash`
reads the one field it needs from `tasks.json` with `serde_json::Value` rather than
depending on the crate.

## An aspect is a part of a task, worked in a tab; a task is work for later

Work split off a task has two homes, and the axis between them is **when**,
not what: a **task** is work that will happen at some point and is not run
immediately; an **aspect** is work run immediately or very soon as part of a
task (`notes/hobby-tooling/skills/aspect-vs-task/business-context.md`
`## when-task-when-aspect`). There is no third. Subtasks — a task of its own
owned by another through `subtasks.json`, worked now as a tab or postponed on
its card — were that third home, and are gone (`## no-subtasks`): run-now
code work had two homes, a research aspect if its answer was a file and a
`tab: true` subtask if it was code, and neither was called an aspect when it
was code. A `subtasks.json` left in the notes is no longer read; its entries
were always ordinary tasks.

`/aspect <work>` in a task's session (or one of its aspects' tabs) adds
`notes/<p>/<t>/<task>/aspects/<name>/` — `aspect.json` and a self-contained
`brief.md` — and checks nothing out. `aspect.json` carries a **kind**:

| kind | done when | state |
|---|---|---|
| `research` | its answer is in `findings.md` | `open` → `active` → `reported` |
| `code` | its PR has merged and its done-when holds (`## code-aspect-done`) | `open` → `active` → `pr` → `done` |

`hwt aspect close` sets `closed` from any of them. Research stays one kind
of aspect beside code (`## research-kind`).

**An aspect can be planned for later in the task.** An idea that is part of
the task but outside the aspect being read is added so it is not forgotten,
and run before the task is finished (`## aspect-lifecycle`). So adding and
starting are two steps: `hwt aspect new` writes the notes, `hwt aspect start`
fetches, cuts `<task>@<name>` from the current tip of the aspect's **base** —
the default branch unless one was named — or rebases the branch onto it when
it already exists, and opens the tab. The PR goes to the same base. `hwt
done` refuses a task while any aspect is not `reported`, `done` or `closed`
(`## done-refuses-open-aspects`), unless `-f`. Commits never move between
branches (`## no-commit-picking`), which is why `hwt subtask --take` has no
successor.

**Every kind starts the same way** (`## aspect-start`): the tab's Claude pane
runs `haspect`, which starts `/task:aspect` while the aspect is `open`, resumes
the tab's conversation with `/task:aspect` while it is unfinished, and resumes
it idle once it is finished. `/task:aspect` reads the kind: a
research aspect is answered into `findings.md`; a code aspect runs the full
`/task:build` pipeline in its worktree (`## code-aspect-pipeline`), with its
state file, plan and journal in the aspect's notes dir, the task's
`business-context.md`, `is_aspect` skipping the split, and the PR against the
base. `/aspect` is allowed under `auto: true` (`## aspect-under-auto`).

**An aspect is not a task**: no `task.json`, so `scan_tasks` and `known-tasks`
never enumerate it and it has no card. It is listed on its task's card with
kind and state, and a click on one not yet started starts it.

**Its worktree lives under `~/fun_aspects`, not beside the task.** About a
dozen consumers read the third component under `~/fun_worktrees` as the task —
`hcdev-task-parts`, `all-tasks` (orphans, `hwt sync`), hdash's `task_of` and
orphan scan, `hauto`, `track-pr`, `zj-notes` — so a sibling `<task>@<name>`
dir would have been an orphan, a sync target, and a phantom notes dir on the
first PR. Under a separate root each of them says "not a task" unchanged, and
aspect awareness is added only where it is wanted: `hresearch` resolves an
aspect worktree to the aspect's notes dir. `@` is in the branch name only;
`<task>/<name>` cannot coexist with a branch `<task>`. `track-pr` therefore
does not record an aspect's PR; `/task:build` writes it into `aspect.json`.

**A tab, not a session.** `hcdev tab <name>` opens a started aspect with
`zellij --session <task's> action new-tab`, or switches to it; with the
session down it starts it from a rendered layout. `hcdev-dev-panes` spells the
two panes for the rendered layouts; `hobby-dev.kdl` and `hobby-claude.kdl`
stay the layouts of a session with no tabs to restore and of `hcdev new` (by
its `--claude`), and `hobby-dev.kdl` of `hcdev here`/`dir`, so they have to
stay alike.

**Every tab shares the task's zellij session name**, so two things that keyed
on it alone had to learn the tab. hdash credits a session-state record to
what its project dir lies in — an aspect worktree, else a task worktree — and
only falls back to the session name for a dir in neither (a coordinator's main
checkout); it badges the aspect's line; a task's ordering counts its
aspects, so an aspect waiting on you raises the card. The ctrl-g dump keys on
the session name plus `$ZELLIJ_PANE_ID`. That the pane id is distinct per tab
and reaches the hooks and the ctrl-g editor was established by the first
research aspect run (`aspects/pane-id-reach/findings.md` in the
`llm_workflow/task_aspects` notes).

**The tabs outlive the session and the checkout.** `hcdev wt` starting a new
session renders a layout with the task's tab first and one tab per aspect
that is not closed and whose worktree is checked out — the zellij session is
scratch like the worktree, and `hcdev close`, a reboot or a second machine
would otherwise lose which tabs a task had. `hwt rm` removes the task's aspect
worktrees before its own (a dirty one stops it, unless `-f`) and keeps their
state; `hwt restore` re-creates every started one not closed, also for a task
already present, and leaves an `open` one for `hwt aspect start`. `hcdev
close` sweeps the processes under the task's aspect root as well as its
worktree. `hwt sync` never sees `~/fun_aspects`, so aspect scratch is never
wip-committed or pushed.

### A split task's state is in `aspect.json`, and a message says when to read it

These decisions are mine, recorded in
`notes/hobby-tooling/skills/a_split_tasks_notes_and_follow_ups_go/business-context.md`.
After a split, the parent's `issue.md`, `implementation.md` and journal went on
listing merged work as open, and the parent answered "is this done?" from what it
saw when it started the aspects.

**No aspect writes the task's notes** (`## who-writes-parent`,
`## parent-issue-body`). `aspect.json` is the static place: `/task:aspect finish`
adds `outcome` and `open_items` beside `state`. `hwt aspect list` reads them, and
asks `gh` for the live state of a PR still at `pr`. An open item a task of its own
covers carries that task in `issue`, which makes it done from this task's view
(`## open-items-to-follow-up`).

**The child messages the parent, and the parent closes itself**
(`## who-writes-parent`, `## parent-done`). The message says only to read the
record. The peer is found by name: a default session name starts with its
directory's basename, its non-alphanumerics as `-`, which is why an aspect's
session (`<aspect>-…`) cannot match its task's. A handed-off parent then runs
`hwt done`, whose refusal means an aspect is still unfinished. No parent listed
loses nothing: the record is on disk, and the task-context hook points any later
session of that task at `hwt aspect list`, including after a compaction.

**A respawn is what notices a merge nobody watched** (`## merge-without-session`).
An unfinished aspect's tab starts as `--continue "/task:aspect"`, and
`/task:aspect` reads a `pr`'s state once: merged goes to `/task:build journal`.

**A follow-up waits on a PR through its block** (`## waits-on-meaning`):
`blocked: {pr, pr_state}`, set with `hwt block --pr`. A merged PR is `ready`, the
same as a passed date, so the card floats up and stops reading as waiting; a closed
one holds, because nothing will land. `hdash` asks `gh` every five minutes and
`hwt blocked` once per run, and neither asks about a `MERGED` or `CLOSED` state,
which cannot change. A follow-up filed at GATE S has no PR yet, so its block starts
as a `--reason` and it is named in the aspect's `blocks`; the aspect's `pr` phase
turns that into `--pr`.

## Two session modes, chosen each time a session starts

A task's session is one of two shapes, and every tab in it has the same one. **Editor
mode** (`hobby-dev`) is Helix beside Claude. **Claude mode** (`hobby-claude`, `hcdev wt
--claude`) is Claude beside `htab`, a ratatui overview of that tab's task or aspect — why it
was started, what it builds and where it stops, the steps open and done, its PR and plan —
with no editor pane; Alt n opens a shell when one is needed. The mode is picked at every
session start, from two buttons on the hdash card, and not remembered (business-context
`notes/hobby-tooling/llm_workflow/two_dev_modes/business-context.md`, `## mode-choice`,
`## editor-on-demand`).

`htab` reads only files on disk, so what it can show is what the pipeline records
(`## aspect-context`); a question it cannot answer is answered by recording more in the
notes, not by asking a model. It names its own tab `<name> · <phase>`, which is why tab
lookup in `hcdev` matches a prefix. A running session's mode is read back from the session
itself — a pane named `overview` — so an aspect tab opened later matches it without a
record that could go stale. `hcdev auto`, the coordinator and the notes tab keep their
layouts (`## other-layouts-unchanged`).

## Opening a session from a dashboard: two things that kill the terminal

Both failures looked identical from the outside — the click opened an alacritty
window that vanished before it drew a frame — and both are worth knowing, because
either one silently comes back the moment a session name is spelled somewhere new.

**A session name has to fit the socket path.** A session's IPC socket is
`<runtime-dir>/zellij/contract_version_1/<session>`, and that path has to fit
`sockaddr_un.sun_path` — 108 bytes on Linux. With the socket dir at 40 characters
zellij rejects any name over 66, and `hcdev-<project>-<topic>-<task>` reaches 67 for
an issue-derived task like `gh-6-simulate-food-production-and-city-growth`. So
`hcdev-session-name` is the **only** place a session name is spelled: it measures the
budget from the real socket dir, and when the three components overflow it, truncates
and appends six hex characters of the full name's sha256. The hash is not decoration —
plain truncation would land two long task names sharing a prefix on one session, and
`hcdev wt A` would drop you into B's editor and Claude pane. `hwt sessions` imports the
function rather than deriving a second copy of the rule. `hdash` still spells the
*window* title the long way, which is fine: a title has no length limit, and it is not
what zellij is asked for.

**A launched terminal has to leave zellij behind.** Run from inside a zellij pane, a
dashboard inherits `ZELLIJ`, `ZELLIJ_SESSION_NAME` and `ZELLIJ_PANE_ID`, and `setsid`
does not undo that — the terminal it opens believes it is already inside that session.
For the task you are *currently* in, `hcdev` then asks zellij to attach a session to
itself, which zellij answers with a **panic**. `hwt_core::spawn_detached` therefore
removes the three variables from every process it starts, and every nu-side launch of
a detached terminal (`hcdev auto -w`, `hwt sessions`) goes through `hcdev-spawn-window`,
which does the same with `with-env`. `hcdev-launch` also refuses the self-attach in so
many words, for the case nothing spawned: a `hcdev wt <this task>` typed by hand.

**And a command that fails kills it the same way.** `nu -c` exits on the first error and
takes the window with it, so a failing `hwt restore` was indistinguishable from both
bugs above — nothing on screen either way. Every nu launch now goes through
`hold_on_error`, which wraps the pipeline in `try`/`catch` and waits on `input`, so the
message that says what went wrong is still there to read. The cost is a keypress after a
session that exits non-zero; that is worth paying to never lose an error again.

**And an inherited stdout keeps the *caller* alive, not the window.** `setsid -f`
returns the instant it forks, so `hcdev auto -w` was never blocking on its child —
alacritty inherits the caller's stdout and stderr, so a caller that reads that pipe to
EOF (`nu -l -c "…"` from a tool, `$(…)` in a shell) waits for the window to *close*
instead. `hcdev-spawn-window` redirects the terminal's stdio to `/dev/null`
(`o+e> /dev/null`) and `hcdev auto -w` then polls `hcdev-session-live` for up to 20s so
its own exit code means "the window is up", not just "the fork succeeded".

**Closing a session has the opposite failure: a name pattern can match the caller
itself.** `hcdev close <task>` tears the session back down — stop what it started,
`zellij kill-session` + `delete-session`, close the window — and the obvious way to
find "what it started" is `pkill -f <worktree path>`. That kills the shell that
typed the command, because the pattern is sitting right there in its own command
line: exit 144, cleanup half-done. `hcdev-worktree-pids` matches structurally
instead — a process whose `/proc/<pid>/cwd` or `/proc/<pid>/exe` resolves under the
worktree — which is a fact about the process, not a string the caller's own argv can
happen to contain. `hcdev-protected-pids` walks `$nu.pid` and its ancestors so the
calling shell, its terminal and the Claude process above it are never swept even
though their cwd is the worktree too. `hcdev close` also refuses a coordinator
session (the reserved topic is a hand-rolled kill away from someone else's task
list) and refuses the session you are currently in — ending your own session out
from under yourself is `#48`'s problem, not this verb's.

`Restore` also runs `hcdev wt` after `hwt restore`, as `Start task` does after `hwt new`.
When the card was only rescanned by the **Refresh** button, a restore that stopped at the
worktree left the card reading `Removed` and a second click could report nothing but
"already present".

## Resolving the Claude pane: `dump-layout` lies about the command, `list-panes` doesn't

`hcdev auto`'s Claude pane has no dependable handle by construction: `hobby-auto`
names it `claude`, but `zellij action dump-layout` reported that pane's *command*
as `rust-analyzer` — its sibling's language server — in three of four live sessions
checked (2026-09-16, zellij 0.44.3). The pane **names** it printed were right in
all four, so the layout is not crossed; `dump-layout`'s command discovery just
cannot be trusted, and why it attributes a descendant process to the wrong pane was
not dug into.

`zellij action list-panes -a --json` and `$ZELLIJ_PANE_ID` are not subject to that
bug — checked against ground truth (`dump-screen --pane-id <id>`, which reads any
pane without focusing it) in this task's own session, both agreed. So `hcdev
claude-pane <task>` resolves via that JSON rather than the layout dump, and prefers
an id the pane recorded for itself over inferring one: `hauto` writes
`claude-pane.json` (`session`, `pane_id`, `worktree`, `recorded`) into the task's
notes dir from `$ZELLIJ_PANE_ID` on every start, which is the issue's suggested
"stable id recorded when hauto starts" — the *pane* stating its own id is
authoritative in a way no derivation after the fact can be. The resolver falls back
to the single live pane whose `terminal_command` is `hauto`/`chobby`, then to the
single one whose `pane_command` starts with `claude`, and refuses rather than
guessing when more than one pane matches a step.

**The startup tip is suppressed in `config.kdl`, not the layout**, because it is a
property of the zellij *client*, not of a layout: a floating `zellij:about` plugin
pane comes up focused in every fresh window regardless of which layout opened it,
and it swallows keystrokes until dismissed. `show_startup_tips false` is one line
in the vendored config that both accounts share, so it silences the tip everywhere
this config is used, work `cdev` sessions included — there is no per-session
config short of rendering one, which `hobby-auto` (installed, not generated) does
not do.

## HMML is the review delta of one PR

`lsps/hobby-mml-lsp/grammar.md` is the **source of truth** for HMML (v3) and
what `/hmml-extract` reads. An `.hmml` file reads one branch's diff back as a
change to the mental model: `delta-<branch>.hmml` in the task's (or aspect's)
notes dir, with `## intent`, the touched `## data`, one `## unit` per entry
point the change reaches, `## api` signatures and a `## tests` summary. Every
line has a diff-style gutter (` ` `+` `-` `~`, the last written `old → new`),
unchanged steps stay as context, and every changed line links to its line in
the diff, which `--lint` checks against the checkout.

**v3 is the work MML v2 rewrite, taken over on purpose** (recorded in
`notes/hobby-tooling/llm_workflow/sync_from_mml/business-context.md`,
`## sync-from-work`). The per-unit spec model it replaces wrote every touched
unit, collaborator and module as a full spec, with `# +` / `# -` comment
markers that had to reconstruct the baseline — at work one PR's delta came to
20 files and 1,800 lines for ~2,450 changed code lines. Nothing plans in HMML
any more, so baseline recoverability had no consumer. `/hmml-new`,
`/hmml-review` and `/hmml-validate`, the per-unit kinds (`plugin`, `system`,
`module`, `endpoint`, `workflow`, `job`), the world sections, access blocks and
their role checks, `use` imports, `@ spec` refs and `@ <path>` type references
went with it. A v2 spec left in a notes dir is a note, not an input, and does
not parse.

**The Bevy layer is light** (`## bevy-layer`): a unit names what runs it —
`in <Schedule>` for a system, `on <Event>` for an observer — and a `## data`
type may carry its ECS role (`component`, `resource`, `event`, `message`,
`state`). Neither is checked against the code or against each other. They make
a changed system or a changed ECS shape read as one; type-checking access was
what a hand-maintained spec needed, and a delta derived from the diff does not.

**`head: worktree` is the second divergence** (`## delta-head-worktree`).
`/task:build` and `/task:auto` derive the delta before anything is committed —
the commits are made after the last gate — and work's v2 resolves every link
against a `head` commit, so the delta of an uncommitted change could not lint.
With `head: worktree` the head side is the files on disk, untracked ones
included, and the delta keeps resolving after the commits land.

The parser and LSP are the work crates with four changes: `head: worktree`, the checkout is
resolved from the hobby layout (`~/fun_aspects`, then `~/fun_worktrees`, then
`~/fun_repos`, the first two only when `repo` is the delta's project), the
trigger and the role, and a Bevy fixture in place of the work one.

Hobby files are **`.hmml`**, never `.mml`. The extension is what keeps the two
dialects apart: Helix routes `.hmml` to `hobby-mml-lsp` by file type, and work
`.mml` to `mml-lsp`.

The **binaries keep their `hobby-mml-*` names** (`hobby-mml-lsp`,
`hobby-mml-parser`), as do the crate directories: they never collided with the
work `mml-lsp` / `mml-parser` on `PATH`, so renaming them would buy nothing and
cost a `cargo uninstall` dance plus stale `~/.cargo/bin` entries.

`tree-sitter-hmml` lives in the Helix fork and still parses v2, so a v3 delta
highlights only partly until it is ported from the work `tree-sitter-mml`.

## HSTRUCT records the standards a repo holds itself to

`lsps/hobby-struct-lsp/grammar.md` is the source of truth. A repo's file is
`structure.hstruct` at its root — or at each crate root in a workspace — and
`just structure` checks every repo that has one.

**It exists because the `CLAUDE.md` files are descriptive and nothing was
prescriptive.** `wusel`'s is 1383 lines narrating how the world generator and
the market work; the section saying how to write new code is ten lines at the
bottom. The standards that keep a repo maintainable and performant were written
nowhere, so nothing could hold the code to them or say how far away it was.

### The standard is durable; the instances are not

This is the whole design, and the obvious approach gets it backwards.

A standard — *"state has one owner"*, *"per-frame work is proportional to what
changed"* — is as true on a project's last day as its first. The **files that
violate it** are renamed, split, and edited constantly. So the file stores the
standard, its argument, and a **budget**: how many offenders exist today.

```
## A file fits in one head

A file that cannot be read in one sitting gets edited by guessing, and the
guesses accumulate.

check: files over 900 lines
budget: 10
```

Eleven is an error. Nine is a nudge to tighten. Renaming a file, splitting one,
or adding two hundred lines to one already over changes nothing in here.

**Nothing in an `.hstruct` file names a module, a file, or a type.** A rule
needing an enumeration to be expressed is the wrong rule for this format. The
offenders are still available — `--detail` prints them — they are simply not
*stored*, because storage is where the churn was.

**What the first draft stored, and why it failed.** A record per module with its
`@ path`, a layer-membership list per layer, and a waiver per violation —
including
`? gameplay/growth.rs 2758 lines`. Editing `growth.rs` at all, even to improve
it, made that waiver stale *and* raised a new violation. The file punished
touching the code, and 236 of its lines were largely a staler copy of the
directory listing. It encoded **measurements**, which churn, in place of
**standards**, which do not.

### The ratchet is the improvement loop

`--ratchet` rewrites every budget the code has moved below, in place, and **only
downward** — a budget that rose would record a regression as the new normal.

That is what lets a repo adopt this on its worst day, which was the one idea the
first draft got right and is kept: write the standards with `budget: 999`, ratchet once,
and the file records exactly how far the code is from each rule. Every later
refactor that improves a number is one command from being recorded.

### A budget is what makes a rough metric usable

`unfiltered queries` and `allocating systems` are heuristics. Some unfiltered
queries are correct; the allocation check matches names rather than types, so it
over-counts a `Vec` built at startup and under-counts one behind a helper.

Under per-instance waivers each would have to be adjudicated and re-adjudicated
on every edit — unusable, which is exactly why the first draft had **no
performance metric at all** despite performance being half of what the standards
are for. Against a budget the count only has to be monotone: getting worse is
caught, and the noise is constant.

### Not everything is countable, and the file says so

A standard declares its verification: `check:` + `budget:`, or `review:` +
`audited:`. A reviewed standard is honest about not being mechanical, which is
the right home for the rules that matter most and that no parser can settle —
*"every spawn in an `OnEnter` carries a `DespawnOnExit`, or it leaks across
screens"* is the most load-bearing rule in `wusel` and is not statically
decidable.

The first draft exiled those to prose, on the reasoning that the file may only
contain what a checker can verify. That was the wrong cut and it is why it
drifted toward whatever was easy to count — line numbers and import edges. The honest
requirement is only that a rule **declares how it is verified**; a dated
`review:` says plainly that a person last looked on that day, which prose cannot
say and which can be seen to go stale.

### Two things the discarded draft got right

**Rust is parsed, and an import resolves by path, not by its last segment.** Text
matching read `use crate::gameplay::{city, growth};` as one import of a module
called `gameplay`. Leaf-segment matching then blamed
`use crate::terrain::brush::Brush` on an unrelated top-level `brush.rs`, and read
`use bevy::app::…` as a local module named `app` — which had put five fabricated
cycles in `wusel`'s first file.

**Workspaces get one file per crate, at the crate root.** `root:` names one
source tree and two files at a repo root collide on the filename. The manifest is
then a sibling, which is what lets `crate:` be verified rather than decorative.

## The vendored language servers

`lsps/` holds seven servers. `hobby-mml-lsp` and `hobby-struct-lsp` implement this
repo's own two languages (above), and `hobby-research-lsp` opens a `.hresearch`
file's links and `[k]` refs in the browser on goto-definition; the other four — `claude-lsp`, `claude-skills-lsp`, `treesitter-class-lsp`,
`path-lsp` — are **copies** of crates that also exist in the work tooling checkout,
vendored so a hobby-only machine can build them at all.

Three consequences worth knowing before touching them:

- **The binary names are unchanged** (`claude-lsp`, `claude-skills-lsp`,
  `treesitter-class-lsp`, `path_lsp`), unlike the deliberately-renamed `hobby-mml-*`.
  There is one Helix and one `languages.toml` shared by both accounts, so a
  `hobby-`-prefixed second name would not isolate anything — it would just register a
  second server that runs on every buffer alongside the first. One binary is correct.
- **Building from here replaces the work build** in `~/.cargo/bin`, and vice versa.
  The two copies will drift; this one is the copy that has the hobby fix.
- **The hobby fix is `CLAUDE_CONFIG_DIR`.** `claude-skills-lsp` used to anchor its
  user tier on `$HOME/.claude`, which in a hobby session offered the work skills and
  none of the 18 hobby ones. It now resolves `$CLAUDE_CONFIG_DIR` first, falling back
  to `$HOME/.claude`, so one binary serves both accounts — `hcdev` exports the
  variable before launching zellij, so the Helix pane inherits it. Keep that property
  if you re-sync from the work copy.

`treesitter-class-lsp` needs no hobby change: its `~/worktrees/TOPIC/TASK` widening
does not match `fun_worktrees`, and falling back to the client root is already the
right scope when the task directory *is* the repo.

Nothing in `lsps/` may name a work repo, worktree, or ticket — the copies were
scrubbed, and a re-sync has to be scrubbed again.

## The vendored MCP servers

`mcps/` holds copies of the work tooling repo's general-purpose MCP servers —
`just-mcp`, `lsp-mcp`, `tree-extract` — vendored so `just install-mcps` builds on a
hobby-only machine instead of skipping all three.

**They are `hobby-`-prefixed, and this is where they diverge from `lsps/`.** The two
accounts run separate builds that coexist in `~/.cargo/bin`:

| | chobby (`~/.claude-hobby`) | claude (`~/.claude`) |
|---|---|---|
| just | `hobby-just-mcp` | `just_mcp` |
| lsp | `hobby-lsp-mcp` | `lsp-mcp` |
| tree-extract | `hobby-tree-extract-mcp` | `julian-tree-extract-mcp` |

The argument for keeping upstream names in `lsps/` does not carry over: there is one
Helix and one `languages.toml`, so a second LSP name would run a second server on
every buffer — but each account has its **own MCP registry**, so a second name
isolates instead of duplicating. Nothing is shared to collide over.

The prefix goes all the way down — package, lib and binary — and the **package** name
is the load-bearing part. Cargo tracks installs by package name in
`~/.cargo/.crates2.json`, so a hobby build sharing the work package name would evict
the work binaries on every `cargo install`, which is exactly what this is meant to
prevent. Renaming only the binary would not be enough.

Consequences: the two copies drift independently (neither overwrites the other), a
re-sync has to be re-scrubbed *and* re-prefixed, and `hobby-tree-extract` writes its
own name into the `generator:` field of `.project-description.md` and the
`.git/info/exclude` comment. That last one is safe because the exclude dedup keys on
the path, not the comment.

Only the *general-purpose* three are here, and the omissions are deliberate rather
than pending work:

- `db-mcp` and `jira-mcp` speak to work systems. Hobby projects have no databases,
  and issues are `issue.md` files in the notes.
- `experts-mcp` assumes a repo level *below* the task, which this layout does not
  have — the same reason `CLAUDE.md` gives for there being no experts at all.
- `constellation-mcp` is not mine and is work-specific besides.

`tree-extract` is the one crate with a path dependency: it reuses the Helix language
registry and grammar cache from `treesitter-class-lsp`, repointed at `lsps/` here.
Vendoring one without the other does not build.

The *server* names stay unprefixed (`just-mcp`, `lsp-mcp`, `tree-extract`) even
though the binaries do not, so a skill or prompt calling `mcp__tree-extract__*` works
on either account. Only the command behind the name differs.

Registration is `-s user` scope, not the default `local` — `local` binds a server to
the single directory it was added from, so every hobby project outside that one
directory would see no MCPs. `install-mcps` removes before adding, so re-running it
repoints an entry that still names a work binary instead of skipping it as already
registered.

## The AI policy stack was removed, and rebuilt on content instead of authorship

Everything that constrained who writes commit messages, PR bodies, issue text and
code comments is **gone from this repo**. This section records what was there, so
the rebuild is not designed blind.

### What was deleted

| Deleted | Was |
|---|---|
| `githooks/` (`commit-msg`, `pre-push`, `policy-lib.sh`) | a global `core.hooksPath` refusing machine attribution, the work git identity, and unfilled placeholders on an upstream-tier push |
| `hooks/human-authored-text/` | a `PreToolUse` gate that intercepted every message-bearing `git`/`gh` command, opened an editor, and denied until a human had typed the text |
| `scripts/ai-policy-tier.sh`, `just ai-policy`, `just check-placeholders` | the `upstream` / `private` tier resolver and the `TODO(jb-comment)` / `TODO(jb-doc)` placeholder convention it gated |
| `skills/pr-facts`, `skills/ai-use-facts` | facts-not-drafts assembly for a PR body and its mandatory `## AI use` disclosure |
| `skills/explain-back`, `skills/comment-review` | a hunk-by-hunk comprehension quiz, then a clause-by-clause trimming pass over every comment in the diff |

`just install-git-hooks` is gone with them, `bootstrap` no longer calls it, the
global `core.hooksPath` is unset, and `doctor` no longer has an "AI policy gates"
section. `install-hooks` gained the same stale-symlink prune `install-skills`
already had, so the retired hook's link in `~/.claude-hobby/hooks/` does not
linger.

### The one rule that survived

**No machine attribution.** No `Co-Authored-By: Claude`, no `Claude-Session:`
trailer, no `Generated with Claude Code` footer, no 🤖. It is now an instruction
in `dotfiles/claude-md/hobby.CLAUDE.md` and nothing else — the mechanism that made
it non-optional was `githooks/`, which is deleted. Kept because Bevy CI rejects the
trailer outright, so it is a correctness fact about where the code goes, not a
preference about authorship.

The work-identity check went with the hooks. `~/.gitconfig`'s `includeIf` on
`gitdir:~/fun_repos/` and `gitdir:~/fun_worktrees/` is now the only thing giving a
hobby clone the hobby identity, and `just doctor` / `just git-identity` the only
things that notice when one escapes it. Nothing blocks a push any more.

### Why it came down

Two reasons, and they are different in kind.

**The gate was wrong in practice.** It classified by substring against the whole
Bash command text, so any command whose *argument* contained `git commit`,
`gh pr create` or `gh issue comment` was intercepted as if it were that command —
a journal entry mentioning the phrase, a heredoc, a `python3 -c`, a grep pattern.
Issue #24 reports it. That is worse than an ordinary bug because of what the layer
did when it fired: block, and wait on a human. It had already been disarmed at
`policy_guards_enabled` for exactly this.

**The policy was wrong in aim.** Its whole content was *who is allowed to write*.
It had nothing to say about whether a comment explained anything, whether a commit
message was readable, or whether a PR description was the right size — so what it
produced was placeholder markers that described nothing (19 still in this tree; the
policy's own note claimed 56 at its peak) and PR bodies whose only guaranteed
section was a disclosure. Forbidding bad prose from one author does not produce
good prose from the other.

The replacement is a quality question, not an authorship one, and it is the open
work of this task.

### Comments: written for a caller, and the default is none

The rule lives in `dotfiles/claude-md/hobby.CLAUDE.md`. **A module header, and a
public element's comment when it tells the caller something the names and types
do not. Nothing else** — no body comment, no comment on a private element or a
test. Every comment has to read correctly to someone who never saw the
conversation or the diff. `/comment-pass` enforces it, over a branch diff or a
path, as the `comments` phase of `/task:build`.

**What I want a comment for** is recorded in
`notes/hobby-tooling/skills/review_too_strict/business-context.md`
(`## intent-comments`, `## module-headers`): on a public item, the abstracted
behaviour at the level the caller cares about, carrying what the signature does
not; and a module header as the highest-level public description of the module.
The failure it names is the model's default output — comments that "served more
as part of the llm conversation", went into too much detail, or said the obvious.

**The first replacement for the AI policy aimed at the wrong failure.** It kept
three places — module, public element, test — made the pass write as well as
cut ("a diff that comes out with fewer comments and no new ones has probably
failed"), required a justification line above every test, and took any *why* it
could not verify from `business-context.md`, reporting the rest as "context
gaps". Its concern was where a reason came from. Measured on 2026-11-07, the
findings it was meant to drive down had grown instead — undocumented public
items 299 → 460, private doc comments 248 → 316 — and the comments it did write
showed the failure: `/// The project the tasks belong to.` on
`pub project: String`, and a `///` on `DEFAULT_BOUND` citing a
`business-context.md` handle. The push to write produced the restatements, and
matching reasons from the notes put the conversation into the code.

So the pass is now mostly a filter. `scripts/comment-candidates.py` finds four
placement states — `BARE-COMMENT`, `PRIV-DOC`, `PUB-UNDOC`, `MOD-UNDOC` — and
`PUB-UNDOC` is a candidate, not a defect: leaving a self-describing item
uncommented is correct. A comment above a test is a `BARE-COMMENT`, because a
test's name says what it pins. Whether a comment is conversational is not
detected: here gates, issues and notes files are the domain, and a pattern over
those words flags legitimate contracts.

A comment states what the code does, so the code is its only source and the pass
needs no recorded context. **A decision about why a body is shaped one way
rather than the obvious one belongs in the commit message**, dated and attached
to the diff that made it, where `git log -L` finds it; the pass hands those on as
notes. `business-context.md` still feeds the plan, the code and the commit
messages.

The module clause carries the one constraint that is easy to violate: a module
header sits far from what it describes and is read long after, so it may hold
only what stays true as the contents change. Naming the responsibility survives a
refactor; listing the contents is wrong the first time something moves.

### Commit messages: the policy made them measurably worse

The AI policy landed on 2026-08-06 and required that every commit message be
hand-written. Split this repo's history there:

| | commits | with a body | mean body | mean subject |
|---|---|---|---|---|
| before | 35 | **35 / 35** | 18 lines | 55 chars |
| after | 9 | **0 / 9** | — | 42 chars |

The obvious confound — that the later commits are smaller — does not hold. Four of
the nine touch 23, 17, 15 and 5 files and carry no body at all, and two shipped a
typo in the subject (`Rewoork`, `mchine`). The other half of the picture: all 20 of
the pre-policy commits worth imitating carry a `Co-Authored-By: Claude` trailer.
The best commit messages in this repo were machine-written, and the rule requiring
them to be hand-written took the body rate from 100% to 0%.

That is a statement about cost, not about writing. An 18-line body per commit is
affordable when producing it is near-free and the work is editing it; hand-written
it is a tax, and the tax was paid in bodies not existing.

**So the rule now runs on content, and the model writes them.** It is in
`dotfiles/claude-md/hobby.CLAUDE.md`. Two things in it are load-bearing.

**Terse, and a record of decisions.** Not the 18-line narratives the pre-policy
commits carried — those are more than is wanted. The body says what was decided and
what justifies it, and nothing else. The diff is already in the commit; a body that
narrates it is spending the reader's attention on what they can already see.

**A justification has exactly three permitted sources**: something said in the
task's prompts, something settled in discussion before the change, or a fact
verified in the code, diff or test output. The third is an *observation* and is
free to state. The first two are *justifications* and cannot be derived — they live
in the conversation, not in the tree, so a model that was not told the reason does
not have it and must ask. Never supply a plausible one. This is the same constraint
`/comment-pass` runs on, sharpened: a comment's reason can at least be checked
against the code, and a commit message's cannot be checked against anything, which
is exactly why an invented one survives.

The attribution ban stays, and its actual reason is narrower than the old policy's:
**a `Co-Authored-By: Claude` trailer is advertising for Anthropic**, and these repos
do not carry it. Bevy CI rejecting the trailer is a second, practical reason that
applies only to Bevy contributions.

History is not rewritten. 20 commits here carry the trailer; the rule is forward-only,
the same call already made about the ~600 work-identity commits, and for the same
reason — the alternative is a force-push across public repos.

Conventional Commits was considered and rejected: `feat:` / `fix:` prefixes carry
less than subject lines like `Drop hobby-mml-index: installed, tested, and never
once run` already do, and nothing here consumes them.

### PR descriptions: the template is what failed, and the artifact survived

Same split, sharper. Median PR body before the policy (#1-#21): 4316 characters,
81 lines. After (#22-#28): 257 characters, 16 lines. #28 has no body at all on a
five-file change.

The proof is #27, which is the authoring gate's template with the blanks left in:

```
## Why
too many false positives, therefore not worth the hassle

## Expected behavior after merge

## Verification

## Scope / out of scope

## AI use
LLM written
```

Three sections empty and the mandatory disclosure reduced to two words. Mandating
headings produced headings, not content.

The headings themselves were not the mistake. Across the 17 pre-policy bodies,
unprompted, `Why` appears 15 times, `Verification` 15, `Scope / out of scope` 12,
`Expected behavior after merge` 12 — alongside one-offs written for the specific
change (`Branch matters as much as path`, `The two mechanisms, and why they're in
this PR rather than a later one`, `Drive-by`). They emerged, and then died the
moment they became a form to fill.

**What replaced them is three sections, each capped.** *What do we change* and
*Why do we change* are one or two sentences each; *How can you test it by hand* is
an ordered bullet list ending in what should be seen. The cap is the point: the
old template failed by asking for five open-ended sections, and the pre-policy
bodies, good as they were, ran to 81 lines that nobody re-reads.

Two rules keep it from decaying back into #27. **No section may be left empty** —
a change with nothing to test by hand says so in one bullet and names what does
cover it, because a blank heading is worse than no heading. And **`## AI use` is
gone, not optional**: the tier resolver that could have told an owned repo from an
upstream one was deleted with the rest of the stack, so a disclosure rule would
have had nothing to key on.

The PR carries the argument for the whole change; the commit bodies carry the
per-decision record. Merges here are merge commits rather than squashes, so both
persist, and restating one in the other is the failure mode to avoid.

PR bodies were also an attribution leak path — #14's body ends with a
`Claude-Session:` URL — so the ban is stated there specifically and not only for
commits.

### What the tree still owes the rule

The rule was written against the code, not applied to it. Outstanding:

`comment-candidates.py .` reports, on 2026-11-07 after the rule changed:

| category | count |
|---|---|
| `BARE-COMMENT` | 1536 (now including comments above tests) |
| `PUB-UNDOC` | 460 — candidates only |
| `PRIV-DOC` | 316 |
| `MOD-UNDOC` | 50 |

Deleting is mechanical. The 50 module headers are the real writing; the public
candidates mostly want no comment.

19 `TODO(jb-comment)` / `TODO(jb-doc)` markers also remain, in
`tools/hdash/README.md`, `hooks/edit-before-execute/README.md`,
`skills/task/build/README.md` and `dotfiles/nushell/hwt.nu`. They are unwritten
documentation, not policy artifacts, so deleting the convention did not delete
them. `just check-placeholders` no longer exists to find them:
`rg 'TODO\(jb-(comment|doc)\):'`.

[bevy-website#2551]: https://github.com/bevyengine/bevy-website/pull/2551

## A goal file is what an unattended run is built and accepted against

Seven coordinator runs and 48 merged PRs produced work that matched its
description and missed what I pictured. The description I write is three to
six lines of intent; every question, issue and acceptance after it was the
model's reading of its own answers, and my picture of the screen never entered
the record. Every divergence those runs left behind was an unstated screen, a
spec at odds with itself, or a bound nobody named — and the one gate that would
have shown it, a person at the running app, is off in every unattended run.

So the front of the pipeline gains a **goal file**:
`notes/<project>/planning/<blurb>/goal.md`, written once with me in one
sitting, in screens (captured, then drawn as ASCII with `changes / unchanged /
hides`) and scenarios (a gesture per step, one `->`, `hand` when nobody can
script it), plus the widget inventory, my decisions verbatim, and what is
deliberately left to the build. `skills/task/goal/README.md` is the format;
`hcoord goal lint` checks the cross-references, and with `--tasks` that every
scenario is owned by exactly one task. Nothing downstream paraphrases a
scenario: a brief quotes it (`hcoord goal show`), and it is the acceptance.

Two things the format decided against. **Not a grammar** — HMML and HSTRUCT
each cost a parser and an LSP, and this is markdown with a linter that can
grow into one if the shape holds. **No `Recommended` on a UX question** — in
the planning sessions I picked the recommended option in 12 of 14, which is
the model choosing dressed as a question; anything that is a picture is asked
open.

The first goal (`bevy_pbr_generator`, texture projects, 2026-09-19) added four
sections the plan had not foreseen — Decisions inside the goal, Current state,
a directory tree as a screen, Left to the build — which is why the format was
written by writing one before any skill automates it. The three pieces that
automate it exist: the interview (`/task:goal`), the seed from scenarios
(`hcoord seed`), and `hand` in `verify`.

A fourth, the judgement pass (`/task:judge`, `hcoord judge`), walked a finished
run scenario by scenario and recorded `ok` / `wrong` / `tune` in
`judgement.md`. It was dropped: it was meant to structure looking at the result,
and in practice it was more cumbersome than that is.

`/task:goal` encodes two decisions. **Its questions are asked open**, with no
option list, because an answer picked from offered options is the model's
choosing and not my words, and Decisions are recorded verbatim. The skill lists
`AskUserQuestion` under `disallowed-tools`, but that removes it only for the
turn that invokes the skill — `allowed-tools` never restricted anything, it
only pre-approves — so after the first gate the skill's own rule is what keeps
it out. **Decisions are made at that gate and never become a task in the
split**: `hcoord goal lint --tasks` requires every task to own a scenario, so a
decision task would be rejected, and a question still open by the time of the
split is a question the interview did not finish.

The one task allowed to own no scenario is an **`enabling`** one — Task shaping
rule 1's labelled escape. Its `note` says `enabling -> <task>`, the lint checks
that the named task is in the list and has it in `depends_on`, because the
enabling task is done before the next one starts, and `hcoord seed` gives it no
acceptance checks: its PR's CI going green is the acceptance. Its brief quotes
the scenarios it unlocks, marked as the other task's.

## Task shaping lives in CLAUDE.md, not in the skill that applies it

The rules for carving a project into issues — five of them, with one labelled
escape — are in `dotfiles/claude-md/hobby.CLAUDE.md` under **Task shaping**. The
skill that runs the pass is `/task-split`, and `/issue-create` checks the three
text-decidable rules before it files anything.

**The rules are in the always-loaded file on purpose.** That file is symlinked to
`~/.claude-hobby/CLAUDE.md`, so it governs every hobby session in every repo,
including the cases that never invoke a skill: a task proposed mid-conversation, an
`/aspect` split, an issue typed by hand. A skill is opt-in, and opt-in enforcement
would have covered only the one case that was already being done deliberately.

So the split is: `CLAUDE.md` holds the rules, `skills/task-split/skill.md` holds
the procedure for applying them and the re-cuts to reach for when one fails, and
`skills/task-split/README.md` holds the argument for why each rule is shaped the
way it is. Nothing is duplicated across the three, because the failure mode of a
rule stated twice is the two copies drifting.

`/task-split` deliberately **files nothing.** Its output is a proposed list to
edit; an accepted entry becomes an issue through `/issue-create`, one at a
time, because the topic, the task name and the body are per-issue decisions and a batch
would make both of them mine to fix afterwards rather than mine to make.

`/task:build` applies them too, in its `assess-split` phase — the one-time
question of whether this task is really one task, asked before any planning and
ending at a hard gate. A piece it cuts off is an **aspect** of this task — a
separate branch and PR, worked in a tab now or later — or a **task** for later,
filed with `/issue-create`; everything below that floor is one pass of the
pipeline on one branch. Commits never move between branches, so a branch that
already holds work is not split at the gate, and work found later is added
with `/aspect`. A code aspect runs the same pipeline with `is_aspect` set,
which skips the split, so depth is bounded at one by the path rather than by
asking the model to remember. Most tasks
are one task, and the phase is expected to say so in a line.

## Installing

`just bootstrap` is the fresh-machine entry point; `just doctor` diagnoses an
existing install and names the recipe that fixes each gap. Every recipe is
idempotent.

Anything more involved than a symlink lives in `scripts/` rather than inline in the
justfile: `preflight.sh` (dependency check), `wire-nushell.sh` (append the `use`
lines once), `link-dotfile.sh` (install a dotfile without clobbering a real file),
`doctor.sh`.

## No machine-specific paths in this repo

This repo is checked out on more than one machine and they do not agree on where
anything outside `~/fun_repos` lives. A hardcoded default is therefore a guaranteed
breakage somewhere, so **paths are derived, never assumed** — from `$HOME`, from
the repo root, or from an environment variable.

The rule bites hardest in formats with no variable expansion. `dotfiles/zellij/config.kdl`
binds `Alt a` to the tool-audit log, and KDL cannot expand anything — so the binding
runs `sh -c` and lets the shell do it:
`tail -f ${CLAUDE_CONFIG_DIR:-$HOME/.claude}/tool-audit.log`. Since `hcdev` exports
`CLAUDE_CONFIG_DIR` before launching zellij, a hobby pane follows the hobby log and
every other session follows the work one — one committed line, correct on both
accounts. Reach for a shell before reaching for a hardcoded path.

## Dotfiles are installed, never merely present

Every symlink in `install-*` goes through `scripts/link-dotfile.sh`, not `ln -sf`.
The difference is the one case `ln -sf` cannot handle: it replaces a stale symlink
but **refuses a regular file**, and a regular file is what the retired home dotfiles
repo (`github.com:julianbieber/config.git`, see #9) left at these paths. The helper
moves it to `<target>.pre-hobby-tooling.<timestamp>` and prints where it went —
nothing is overwritten in place, and no backup is silent.

`doctor` tests these with `-h` before `-e` for the same reason. `-e` is true for a
file some *other* repo installed, which is exactly how `~/.config/zellij/config.kdl`
passed as this repo's for as long as it did: the layout was linked, the config was
merely there, and `default_shell "nu"` — what makes `hwt`/`hcdev` reachable in a pane
— came from somewhere `just bootstrap` had never heard of. A real file at a path this
repo owns is a gap, not a pass.

`link-dotfile.sh` is also where per-machine variants will land (#9): prefer
`<source>.$HOBBY_MACHINE` over `<source>` there, and every recipe inherits it.

## The conversation under ctrl-g

**ctrl-g** in Claude Code opens `$EDITOR` on the prompt buffer. Here that shows
the conversation so far beside it, so the context is readable while writing the
next prompt. Two pieces, and they only work as a pair:

- `hooks/claude-conversation/hook.sh` renders the transcript to
  `/tmp/claude-conversation-<key>.md` on every `PostToolUse` and `Stop`
- `bin/hx-claude` is `$EDITOR`, and vsplits that file next to the prompt buffer
  ctrl-g hands it

`<key>` is the sanitized `$ZELLIJ_SESSION_NAME-p$ZELLIJ_PANE_ID`, so ctrl-g in one
pane opens that pane's conversation and not a neighbour's — the pane id is what
separates a task's tab from its aspects' tabs, which share the session
name. **The key derivation is duplicated in
both scripts and must stay identical** — diverge and each silently uses a
different file.

This is not about the zellij layout. The `editor` pane stays a plain shell; the
conversation view is a ctrl-g thing, transient, and gone when you close it.

`settings.json` calls the hook **by absolute path**
(`$HOME/.claude-hobby/hooks/claude-conversation.sh`), like every other hook, not
by a bare `claude-conversation-dump` on `PATH`. That bare name was the bug: it
resolved only on a machine that also had the work checkout, and there only
through a symlink into a work *worktree* that any `wt rm` deletes. On a
hobby-only machine it failed after every single tool call, silently, because
`PostToolUse` registers it with an empty matcher. Only `hx-claude` needs
`~/.local/bin`, because `$EDITOR` names it.

`hx-claude` **keeps its upstream name**, for the reason `lsps/` keeps theirs and
`mcps/` does not: `$EDITOR` is one variable shared by both accounts, so a
`hobby-`-prefixed second name would isolate nothing — it would just be a name
nothing points at. Installing from here replaces any link from the work checkout,
which is harmless because the script is account-agnostic. The dump path is safe
to share for the same reason: `hcdev` names sessions
`hcdev-<project>-<topic>-<task>` and the work `cdev` names its own `cdev-…`, so
the two accounts already have disjoint keys inside
`/tmp/claude-conversation-*.md`.

## Helix is built and configured from its own repo

Nothing here builds `hx`, writes its config, or holds a grammar. The fork
(`julianbieber/helix`, branch `custom`) owns all of it: its `languages.toml` is the
config and is copied to `~/.config/helix/`, the `.hmml` / `.hstruct` / `.hresearch`
tree-sitter grammars live under its `grammars/`, their queries under its
`runtime/queries/`, and its own `justfile` builds and installs the lot. Its
`CLAUDE.md` records how.

This repo used to do that work from the outside: `wire-helix` rewrote the Helix
checkout's `languages.toml` in place, copied queries into it and generated a
per-machine overlay, `helix-paths.sh` went looking for the checkout, and the grammars
sat in `lsps/`. The result was a Helix whose grammars could only be built with
this repo checked out, and a Helix checkout whose working tree held edits no commit
there had made. The config is managed in the Helix repo, so that is where all of it now
lives.

What stays here is the language servers that config names — `hobby-mml-lsp`,
`hobby-struct-lsp`, `hobby-research-lsp` and the vendored four — and the things that
merely *use* an installed `hx`: `bin/hx-claude`, the zellij layouts, and the MCPs
that read `~/.config/helix/languages.toml` or the runtime. A server the config names
but this machine has not installed is fine, so nothing checks the two lists against
each other.

## Paths

```
~/fun_repos/<project>                          main checkouts
~/fun_worktrees/<project>/<topic>/<task>       the worktree itself
~/fun_repos/hobby-mimisbrunnr/notes/
    <project>/<topic>/<task>                   task notes
~/.claude-hobby                                CLAUDE_CONFIG_DIR
```

## Account Isolation

`CLAUDE_CONFIG_DIR=~/.claude-hobby` gives a fully separate login, MCP set,
settings, skills, and history. Two ways in:

- `chobby` — wrapper on `PATH` that sets the var and execs `claude`
- `hcdev` — exports the var before launching zellij, so every pane inherits it

A bare `claude` always means the **work** account. If you're in a hobby worktree
and unsure, check `echo $CLAUDE_CONFIG_DIR`.

## Conventions

- Every tool is a folder with a `README.md`, never a loose file.
- `skill.md` is the canonical skill filename.
- No `julian-` prefix — this repo has one author, so provenance prefixes are noise.
- Hobby projects track work in the notes repo — `issue.md` beside `task.json` — not
  in GitHub issues or Jira. Anything reaching for a tracker reads and writes there.
- No secrets, no work identifiers, no company hostnames. This repo is private but
  it is not a work repo.

## Installing

`just` lists every recipe. There is no single `install`: `just bootstrap` runs the
core set, `just bootstrap-full` adds every optional Rust build and the MCP servers,
and each `install-*` recipe is independently re-runnable.
