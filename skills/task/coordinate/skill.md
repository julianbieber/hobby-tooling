---
description: Work a project's task list unattended from a coordinator session — carve the description into tasks, record them in tasks.json, and run each one through /task:auto in its own hcdev window, one at a time, each task's PR merged before the next one starts
argument-hint: '[<description>] [--blurb <name>] [--go]'
allowed-tools: Skill, Read, Write, Edit, AskUserQuestion, Bash
---

Run a body of work from start to a series of merged PRs with one conversation at the
start and, for most tasks, nothing but a merge at the end. This session is the
**coordinator**:
it owns no code and opens no PR. It carves the description into tasks, writes
them to `tasks.json`, and works the list by opening one `hcdev auto` window at
a time, blocking on that task's pipeline state until it needs me or is done,
and then on its pull request until that PR is merged.

The pane on the left shows `tasks.json` (`hcoord tui`); it re-reads the file on
every change, so the list you write is the list I see.

## Six rules that override everything else

1. **One task at a time, and it is not over until its PR is merged.** Never
   start the next task while one is `running`, `awaiting-human` or
   `awaiting-merge`. Parallel sessions on one repo are what this skill exists
   to avoid, and a task whose PR is still open is not finished: the next task
   branches off the default branch, so an unmerged predecessor is work the next
   task would not have.
2. **`tasks.json` is the record, and it is written at every change** — a task
   started, a gate reached, a PR opened, a task done. A crash or an interrupt
   loses nothing that was written; `/task:coordinate` with no arguments picks
   up from the file.
3. **Wait with `hcoord wait`, never with a prompt.** One shell call per wait,
   ten minutes at most, then call again. Do not read `build-task.json` in a
   loop from here.
4. **Merging is mine unless the list says otherwise.** `merge` in `tasks.json`
   is `mine` — report the green PR and hold the list until I merge it — or
   `coordinator`, which I set when I trust the run. With `coordinator` the
   merge is made by the task's own session, not this one — it watches its own
   checks and merges on green; this session only confirms the merge landed.
   Either way a red check, a conflict or a closed PR blocks the task and stops
   the loop: it is a decision, not a wait.
5. **Nothing is invented.** The description is recorded verbatim; a task's
   brief is assembled from the description, the split and my answers, and
   says which is which; a PR is recorded from `prs.json` or `gh`, not from
   expectation. The same three sources as every other skill.
6. **The coordinator knows no task's specifics.** It never reads a task's
   code, plan, diff or journal. Planning and building happen in the task's
   own window, in processes that end with their context; what reaches this
   session is a phase name, a PR URL, and the acceptance result the task
   recorded about itself. Keeping the coordinator's context free of every task
   is what lets it run a whole list in one session.

## Step 0 — Which coordinator, and what state

1. **Where.** `pwd` must be `~/fun_repos/<project>` — a main checkout. Anywhere
   else, say so and stop: a coordinator runs from the project, not from a task
   worktree.
2. **Which.** The SessionStart hook prints the coordinator's notes directory
   when `hcdev coordinator` started this session (`HCOORD_NOTES_DIR`), or lists
   the open ones. Failing that, `--blurb <name>` names it, and with exactly one
   open coordinator under `notes/<project>/coordinator/` that one is it. Several
   and no flag: ask which. The notes dir is
   `~/fun_repos/hobby-mimisbrunnr/notes/<project>/coordinator/<blurb>/`.
3. **Read `tasks.json`** there. Its shape:
   ```json
   { "project": "wusel", "blurb": "food-economy",
     "description": "<verbatim>", "created": "2026-09-11",
     "model": "sonnet", "merge": "mine",
     "goal": "../../planning/food-economy/goal.md",
     "tasks": [
       { "task": "food-economy-fields", "topic": "food-economy",
         "title": "Fields grow wheat over the seasons",
         "done_when": "place a field, watch it fill by autumn",
         "scenarios": ["S1"],
         "depends_on": [], "status": "todo",
         "pr": null, "note": null, "updated": null } ] }
   ```
   `status` is one of `todo`, `running`, `awaiting-human`, `awaiting-merge`,
   `done`, `blocked`. `model` is what the build stage runs on; a task entry may
   carry its own `model` to override it. The plan stage always runs on the
   account default. `merge` is `mine` or `coordinator` and defaults to `mine`.
   `goal` is optional; when present every task names the scenario ids it owns.
4. **Branch on state.** Empty list and a description → *carve*. Empty list and
   no description → ask for one and stop. Non-empty list → *work*, unless a
   description was given as well, in which case ask whether it replaces or
   extends the list before touching it.

Announce `{project, blurb, tasks: n, next step}` in one line.

______________________________________________________________________

## Carve → GATE L (the one conversation)

1. Write the description **verbatim** into `tasks.json` `description`.
2. Run `/task-split` with the description. It files nothing and hands back an
   ordered list with a `Done when`, `Depends on` and `Revert` per task, the
   decisions to make first, and the rejected cuts.
3. **Turn the list into entries.** Per task: `task` is `<blurb>_<slug>` where
   the slug is the title lowercased, words joined by `_`, `[a-z0-9_]` only, at most 40 characters;
   `topic` is the blurb (so every task of this coordinator sits under
   `~/fun_worktrees/<project>/<blurb>/`); `title`, `done_when`, `depends_on`
   (by `task` name) from the split; `status: todo`. An `enabling` task keeps its
   `enabling -> <task>` in `note`.
4. **Decisions to make first** are the questions this gate exists for. Ask them
   now, all of them, numbered, alongside the list.
5. Write `tasks.json`.

**→ GATE L: HARD STOP.** Print the list as a table — number, task, title, done
when, depends on — the rejected cuts with the rule each failed, the sequence
checks, and the decisions as numbered questions. Then one line: *"Edit
`tasks.json` or tell me what to change; answer the questions in your own words.
Say `go` and I'll start working the list — each task runs its own acceptance and
you'll hear from me when its PR is waiting for you to merge, or sooner if it
needs acceptance criteria or could not make its checks pass. Say `go, you merge`
and I'll merge each PR myself once its checks are green."*
End your turn.

On `go`: re-read `tasks.json` (it may have been edited), set `merge` to
`coordinator` if I handed you the merges and `mine` otherwise, record every
answer **verbatim** in `business-context.md` in the coordinator's notes dir
(one `## <handle>` block each, the `/task:build` format), then *work*. I may
hand you the merges at any later point too; that is a `merge` write and takes
effect on the next PR.

______________________________________________________________________

## Work — the loop

Repeat until nothing is left:

**W1 — Current task.** A task that is `running` or `awaiting-human` is the
current one; skip to W4 with its notes dir. One that is `awaiting-merge` is
the current one too; skip to W5 with its PR. Otherwise pick the **first `todo`
whose `depends_on` are all `done`**. None, with `todo`s remaining → they are
blocked on something not done: mark each `blocked` with the reason in `note`
and stop with a summary. None, with none remaining → *finish*.

**W2 — Base branch.** Every task branches off the **default branch**, because
every earlier task's PR is merged before this one starts. Resolve it once and
fast-forward the main checkout:
```bash
gh repo view --json defaultBranchRef -q .defaultBranchRef.name
git pull --ff-only
```
A refusal is reported in the note and does not stop the loop; it means the base
is stale, not that the task cannot run. No `depends_on` check here — W1 only
picks a task whose dependencies are `done`, and `done` means merged.

**W3 — Start it.**

1. Create the worktree and notes dir:
   ```bash
   nu -l -c "hwt new '<task>' -p '<project>' -t '<topic>' -b '<base branch>' --no-cd"
   ```
2. Seed the task's pipeline state. **When `tasks.json` names a `goal`**, run

   ```bash
   hcoord seed <coordinator notes dir> <task>
   ```

   It writes `build-task.json` and `business-context.md` into the task's notes
   dir, and the brief is its: the coordinator's description, the goal's intent
   and the decisions that name the task or its scenarios, the scenarios the task
   owns exactly as `hcoord goal show` prints them (that is `acceptance`), the
   screens they name, and what depends on the task. Do not write either file
   yourself, and skip step 3. Exit 1 — the goal does not lint, the task owns no
   scenario, or it is already seeded — is `status: blocked` with `note` from its
   stderr line; stop the loop, the same shape as W4's exit 1.

   **When it names no `goal`**, seed by hand, at
   `~/fun_repos/hobby-mimisbrunnr/notes/<project>/<topic>/<task>/build-task.json`:
   ```json
   { "phase": "acceptance", "auto": true,
     "merge": "<mine or coordinator, from this list>",
     "model": "<the task's model, else the list's>",
     "coordinator": "<this coordinator's notes dir>",
     "brief_source": "coordinator", "brief": "<see below>",
     "acceptance": "<done_when, verbatim>",
     "feedback": [], "rounds": 0,
     "written": "<now, RFC 3339>", "phase_since": "<now, RFC 3339>" }
   ```
   This is how the task's session learns who merges its PR, and it is read
   once, at task start — a later change to this list's `merge` takes effect on
   the next task's seed, not on one already running.

   The **brief** is read by a session with no memory of this one, so it is
   self-contained and says where each part came from: the coordinator's
   description (verbatim, labelled as such), this task's title and behaviour
   from the split, its `done_when`, what it depends on and what it must not
   break, and the recorded answers that concern it — copied verbatim under
   their handles, never summarised.
3. Only when seeding by hand: seed the task's `business-context.md` with those
   same answer blocks, copied verbatim from the coordinator's file, so
   `/comment-pass` in that session can cite them. Skip the file when no answer
   concerns the task.
4. Open the session in its own window:
   ```bash
   nu -l -c "hcdev auto '<task>' -p '<project>' -t '<topic>' --window"
   ```
5. `tasks.json`: `status: running`, `updated: <now, RFC 3339>`, `note` empty
   unless W2's pull was refused, in which case it says so. Append one line to
   today's journal (below).

**W4 — Wait.** Two ways, cheapest first.

*Idle notice.* The task's window is a Claude session on this machine, so
`ListAgents` lists it once it has started: its name begins with the task name.
Subscribe with `SendMessage` to that name, `notify_when_idle: true` and **no
message** — a pure subscription, one notice when the session next ends a turn,
which is exactly a gate or `done`. Never send the session text. When the
notice arrives, read the phase without blocking:
```bash
hcoord wait <task notes dir> --timeout 1
```
and act on the line as below; a `timeout` outcome means the session is still
mid-pipeline (the plan process exited and the build one started, say) —
subscribe again.

*Polling.* When the session is not listed yet, or after a resume, block
instead:
```bash
hcoord wait <task notes dir>
```
Run it with the Bash tool's full budget (`timeout: 600000`), never the default two
minutes — the command blocks for up to 570 seconds by design. It prints one JSON
line:

- `outcome: timeout` (exit 2) → wait again. When the line's `phase` is the same
  as the previous timeout on this task, set `note` from its `unchanged_for`
  (rendered as a duration) instead of leaving it blank — *"implement, unchanged
  for 1h 12m"* — so a resumed coordinator or a person reading `tasks.json` sees
  that the session is still on the same phase rather than nothing at all. A
  single timeout, or one whose phase differs from the last, reports nothing:
  the loop is about to call again either way.
- `outcome: human`, phase `await-fix`, no `merge` on the line → the task ran
  its own acceptance and could not make it pass, so it opened no PR. Set
  `status: blocked` with the failed check names from the line's
  `acceptance.failed` in `note`, and **stop the loop** with a summary. Like a
  red check in W5 this is a decision, not a wait: nobody is going to answer it
  in that window, and the tasks behind it would branch off work that is not
  landing.
- `outcome: human`, phase `await-fix`, `merge.outcome == "blocked"` → the PR
  opened and passed acceptance, but the session could not merge it itself. Set
  `status: blocked` with `note`: `merge blocked: <merge.reason>`, plus the
  line's `merge.check` when there is one, and **stop the loop** with a
  summary — the same decision as a red check in W5, reached one phase later.
- `outcome: human`, any other phase → the session stopped at a gate. Set
  `status: awaiting-human` and `note` from the phase — `await-acceptance`:
  *needs acceptance criteria*, `await-play`: *ready to play*, anything else:
  the phase name. Print one line naming the window
  (`hcdev-<project>-<topic>-<task>`) and what it wants. Then
  wait again with `--after <that phase>` so the call blocks until I have
  answered and the session has moved on (or subscribe to the idle notice again).
- `outcome: done`, `merge.outcome == "merged"` → the session has already merged its own PR.
  Record `pr` from `merge.pr` (falling back to `prs.json`), `status: done`,
  journal line naming the PR and that it merged itself, and go straight to
  **W6** — no `hcoord merged` call, no `awaiting-merge`.
- `outcome: done`, no `merge` on the line → the session has finished and
  pushed but did not merge it itself (`merge: mine`, or nothing recorded).
  Read `prs.json` in the task's notes dir for the PR URL (fall back to
  `gh pr list --head <task> --json url`). No PR anywhere → the task opened
  none: `status: done`, journal line, back to W1. Otherwise record `pr`, set
  `status: awaiting-merge`, `updated`, `note: PR open, waiting for merge` plus
  the line's acceptance tally when it carried one, and go to W5.

The `acceptance` object on a wait line — `{passed, checks, failed}`, each failure
naming its command and exit status — is the evidence a task ran on itself. It is
the whole of what this session learns about a task's correctness, and reading it
is the one thing that takes the place of looking at the window.
- exit 1 → the state file never became readable within the timeout. `status:
  blocked`, `note` with the error, stop and report.

**W5 — Merge.** Reached only for a PR the session did not merge itself —
`merge: mine`, or a session that recorded no `merge_result`. The list holds
here until the PR has landed. The task is not `done` while it is open, and no
`todo` starts behind it.

1. Print one line naming the PR URL and that it is ready to merge, when
   `merge` is `mine`, once, on arriving here — not on every wait.
2. Block on the PR. One shell call, ten minutes, then call again:
   ```bash
   hcoord merged <pr url>            # merge: mine
   hcoord merged <pr url> --ready    # merge: coordinator
   ```
   Run it with the Bash tool's full budget (`timeout: 600000`). It prints one
   JSON line `{pr, outcome, state, checks}`:

   - `outcome: timeout` (exit 2) → nothing has resolved. Wait again, silently.
   - `outcome: mergeable` → only with `--ready`: checks pass and GitHub reports
     no conflict. Merge it, then wait again to confirm:
     ```bash
     gh pr merge <pr url> --merge
     ```
     A refusal from `gh` blocks the task: record what it said in `note` and
     stop. Merge commits, not squashes — that is how these repos merge.
   - `outcome: blocked` → a red check, a conflict, or a PR closed unmerged. Set
     `status: blocked` with the line's `reason` in `note`, and **stop the
     loop** with a summary. It is a decision for me, and the tasks
     behind it would branch off work that is not landing.
   - `outcome: merged` → go to W6.
   - exit 1 → `gh` failed. `status: blocked`, `note` with the error, stop.

**W6 — Done.** `status: done`, `updated`, clear `note`, keep `pr`. Print one
line naming the task and its PR, and a journal line naming the PR. Back to W1.

`hwt`, `hcdev` and `hcoord` are all on `PATH`; each `nu -l -c` is a fresh
login shell so the modules are loaded.

## Finish

Every task `done` — every PR merged — or the loop stopped at a
`blocked` one: print the list with each PR link, mark the coordinator done when
nothing is blocked (`task.json` → `done: true`), write the journal, and end. A
blocked task is left blocked with its reason on the card; unblocking is a
decision for me.

When `tasks.json` names a `goal`, the last line printed names the pass: *"every
task is done; `/task:judge` walks the goal's scenarios and records the verdict in
`judgement.md`"*. This session does not run it. The judgement needs me, and this
session is the one place I am already sitting.

## Journal

The coordinator has no worktree, so `journal:write` cannot find it. Write
`<coordinator notes dir>/YYYY-MM-DD.md` directly, in the same front matter
as any task journal (`title: "<blurb> — coordinator"`, `created`, tag
`journal`), appending one timestamped line per event: task started (with
base), gate reached, PR opened, PR merged and by whom,
task done, anything refused. Terse; the task's own journal has the detail.

## Notes on scope

- **Interrupting is fine.** Esc out of a wait, come back with
  `/task:coordinate` and no arguments, and the loop resumes from `tasks.json`:
  a `running` task is waited on again, an `awaiting-human` one is waited on
  with `--after`, and an `awaiting-merge` one goes straight back to `hcoord
  merged`.
- **Editing `tasks.json` by hand is fine** between tasks — reorder, drop, add.
  The file is re-read at every W1. A running task's entry is the one thing not
  to edit.
- **No issues are written.** `/issue-create` is an editing session, and this
  skill is meant to run without one. The tasks it creates are recorded by their
  `task.json` and the brief in `tasks.json`, which is the record.
- **The coordinator never pushes to the default branch.** A merge it makes is
  `gh pr merge` on a PR whose checks are green, and only when `merge` says
  `coordinator`.
- **A merge that needs a decision stops the list.** Red checks, a conflict and
  a closed PR are all `blocked`, never waited out. The loop is resumed with
  `/task:coordinate` once the PR is merged or the entry is edited.
