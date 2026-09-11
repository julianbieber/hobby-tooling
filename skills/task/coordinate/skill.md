---
description: Work a project's task list unattended from a coordinator session — carve the description into tasks, record them in tasks.json, and run each one through /task:auto in its own hcdev window, one at a time, knowing nothing about any task but its phase
argument-hint: '[<description>] [--blurb <name>] [--go]'
allowed-tools: Skill, Read, Write, Edit, AskUserQuestion, Bash
---

Run a body of work from start to a stack of PRs with one conversation at the
start and one visit per task at the end. This session is the **coordinator**:
it owns no code and opens no PR. It carves the description into tasks, writes
them to `tasks.json`, and works the list by opening one `hcdev auto` window at
a time and blocking on that task's pipeline state until it needs me or is done.

The pane on the left shows `tasks.json` (`hcoord tui`); it re-reads the file on
every change, so the list you write is the list I see.

## Four rules that override everything else

1. **One task session at a time.** Never start the next task while one is
   `running` or `awaiting-human`. Parallel sessions on one repo are what this
   skill exists to avoid.
2. **`tasks.json` is the record, and it is written at every change** — a task
   started, a gate reached, a PR opened, a task done. A crash or an interrupt
   loses nothing that was written; `/task:coordinate` with no arguments picks
   up from the file.
3. **Wait with `hcoord wait`, never with a prompt.** One shell call per wait,
   ten minutes at most, then call again. Do not read `build-task.json` in a
   loop from here.
4. **Nothing is invented.** The description is recorded verbatim; a task's
   brief is assembled from the description, the split and my answers, and
   says which is which; a PR is recorded from `prs.json` or `gh`, not from
   expectation. The same three sources as every other skill.
5. **The coordinator knows no task's specifics.** It never reads a task's
   code, plan, diff or journal. Planning and building happen in the task's
   own window, in processes that end with their context; what reaches this
   session is a phase name and a PR URL. Keeping the coordinator's context
   free of every task is what lets it run a whole list in one session.

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
     "model": "sonnet",
     "tasks": [
       { "task": "food-economy-fields", "topic": "food-economy",
         "title": "Fields grow wheat over the seasons",
         "done_when": "place a field, watch it fill by autumn",
         "depends_on": [], "status": "todo",
         "pr": null, "note": null, "updated": null } ] }
   ```
   `status` is one of `todo`, `running`, `awaiting-human`, `done`, `blocked`.
   `model` is what the build stage runs on; a task entry may carry its own
   `model` to override it. The plan stage always runs on the account default.
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
3. **Turn the list into entries.** Per task: `task` is `<blurb>-<slug>` where
   the slug is the title lowercased, `[a-z0-9-]` only, at most 40 characters;
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
Say `go` and I'll start working the list — you'll hear from me once per task,
when it is ready to play."* End your turn.

On `go`: re-read `tasks.json` (it may have been edited), record every answer
**verbatim** in `business-context.md` in the coordinator's notes dir (one
`## <handle>` block each, the `/task:build` format), then *work*.

______________________________________________________________________

## Work — the loop

Repeat until nothing is left:

**W1 — Current task.** A task that is `running` or `awaiting-human` is the
current one; skip to W4 with its notes dir. Otherwise pick the **first `todo`
whose `depends_on` are all `done`**. None, with `todo`s remaining → they are
blocked on something not done: mark each `blocked` with the reason in `note`
and stop with a summary. None, with none remaining → *finish*.

**W2 — Base branch.** Fast-forward the main checkout:
```bash
git pull --ff-only
```
A refusal is reported in the note and does not stop the loop. Then, for each
task in `depends_on`, check its PR: `gh pr view <pr> --json state,mergedAt`.
Every dependency merged → base is the default branch. Any dependency still
open → base is **that task's branch** (its `task` name), and the note says
`based on <task>` so the PR's reviewer knows the stack. Two open dependencies
→ stop and ask; a three-way base is a decision, not a default.

**W3 — Start it.**

1. Create the worktree and notes dir:
   ```bash
   nu -l -c "hwt new '<task>' -p '<project>' -t '<topic>' -b '<base>' --no-cd"
   ```
2. Seed the task's pipeline state at
   `~/fun_repos/hobby-mimisbrunnr/notes/<project>/<topic>/<task>/build-task.json`:
   ```json
   { "phase": "acceptance", "auto": true,
     "model": "<the task's model, else the list's>",
     "coordinator": "<this coordinator's notes dir>",
     "brief_source": "coordinator", "brief": "<see below>",
     "acceptance": "<done_when, verbatim>",
     "feedback": [], "rounds": 0 }
   ```
   The **brief** is read by a session with no memory of this one, so it is
   self-contained and says where each part came from: the coordinator's
   description (verbatim, labelled as such), this task's title and behaviour
   from the split, its `done_when`, what it depends on and what it must not
   break, and the recorded answers that concern it — copied verbatim under
   their handles, never summarised.
3. Seed the task's `business-context.md` with those same answer blocks, copied
   verbatim from the coordinator's file, so `/comment-pass` in that session
   can cite them. Skip the file when no answer concerns the task.
4. Open the session in its own window:
   ```bash
   nu -l -c "hcdev auto '<task>' -p '<project>' -t '<topic>' --window"
   ```
5. `tasks.json`: `status: running`, `updated: <now, RFC 3339>`, `note` as W2
   decided. Append one line to today's journal (below).

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

- `outcome: timeout` (exit 2) → wait again. Nothing to report.
- `outcome: human` → the session stopped at a gate. Set
  `status: awaiting-human` and `note` from the phase — `await-acceptance`:
  *needs acceptance criteria*, `await-play`: *ready to play*, anything else:
  the phase name. `notify-send "<blurb>" "<task>: <note>"`. Print one line
  naming the window (`hcdev-<project>-<topic>-<task>`) and what it wants. Then
  wait again with `--after <that phase>` so the call blocks until I have
  answered and the session has moved on (or subscribe to the idle notice again).
- `outcome: done` → read `prs.json` in the task's notes dir for the PR URL
  (fall back to `gh pr list --head <task> --json url`). Set `status: done`,
  `pr`, `updated`, clear `note`. `notify-send` and a journal line. Back to W1.
- exit 1 → the state file is unreadable. `status: blocked`, `note` with the
  error, stop and report.

`hwt`, `hcdev` and `hcoord` are all on `PATH`; each `nu -l -c` is a fresh
login shell so the modules are loaded.

## Finish

Every task `done` (or the loop stopped at a `blocked` one): print the list
with each PR link, mark the coordinator done when nothing is blocked
(`task.json` → `done: true`), write the journal, and end. A blocked task is
left blocked with its reason on the card; unblocking is a decision for me.

## Journal

The coordinator has no worktree, so `journal:write` cannot find it. Write
`<coordinator notes dir>/YYYY-MM-DD.md` directly, in the same front matter
as any task journal (`title: "<blurb> — coordinator"`, `created`, tag
`journal`), appending one timestamped line per event: task started (with
base), gate reached, PR opened, task done, anything refused. Terse; the task's
own journal has the detail.

## Notes on scope

- **Interrupting is fine.** Esc out of a wait, come back with
  `/task:coordinate` and no arguments, and the loop resumes from `tasks.json`:
  a `running` task is waited on again, an `awaiting-human` one is waited on
  with `--after`.
- **Editing `tasks.json` by hand is fine** between tasks — reorder, drop, add.
  The file is re-read at every W1. A running task's entry is the one thing not
  to edit.
- **No GitHub issues are filed.** `/gh-issue-create` is an editing session,
  and this skill is meant to run without one. File them afterwards from the
  PRs if you want the record.
- **Merging is mine.** The coordinator never merges, never pushes to the
  default branch, and bases a dependent task on an unmerged branch rather than
  waiting for a merge that may be days away.
