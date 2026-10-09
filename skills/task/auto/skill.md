---
description: Run a task unattended in two separate processes — a headless plan stage that reads the code once and writes plan.md, then a fresh build stage that implements from the plan alone and, unattended, runs the acceptance observation as commands before the PR opens, or hands me the running app when I am there to look
argument-hint: 'plan | build [<description> | #<issue>]'
allowed-tools: Skill, Read, Write, Edit, Bash
---

Run a task **unattended**, in two processes that never share a context.
`hauto` — the Claude pane of an `hcdev auto` session — starts each one from the
phase in `build-task.json`:

| stage | process | reads | writes |
|---|---|---|---|
| `plan` | `chobby -p "/task:auto plan"`, headless | the brief, the code | `plan.md`, the state file |
| `build` | `chobby "/task:auto build"`, interactive, fresh | `plan.md`, the brief | the code, the PR |

The split is the point: the plan stage's reading of the code dies with its
process, and the build stage starts from a document instead of a conversation.
There is **no HMML** in this pipeline and no spec review loop. `/task:build`
governs nothing here; where this file is silent, do the obvious thing and note
it in the journal.

The stops are named so `hcoord wait` sees them, and which ones exist depends on
`auto` in the state file:

| | `auto: true` — a coordinator seeded it, nobody is at the machine | `auto` false or absent — I typed `hcdev auto` |
|---|---|---|
| start | `await-acceptance`, only when the brief does not say what done looks like | the same |
| end | none. The plan writes the acceptance observation as commands and the `verify` phase runs them; the PR opens when every scripted check passes; a `hand` scenario is captured, not run | `await-play`, always |
| failure | `await-fix` — the checks would not pass, the PR could not be merged, or the repo's CI — run locally before the push, or on the PR after it — spent its bound and would not go green | `await-fix` — the repo's CI would not go green, locally or on the PR |

An unattended run has no reader, so a report it prints is not read: the window
is off-screen and the pane is about seventeen columns wide. The checks replace
the report because a failing command stops the pipeline and a report does not.

## Rules that override everything else

1. **Never write a reason I did not give you.** Three sources: the brief, what
   is recorded in `business-context.md`, a fact verified in the code. Nothing
   is asked mid-run. A *why* none of them supplies is a **gap**, listed at the
   play gate, never filled in.
2. **The state file is a contract.** Write `build-task.json` at every
   transition. The waiting phases are `await-acceptance`, `await-play`
   (interactive only) and `await-fix` (unattended, or CI that would not go
   green), and there are no others; `planned` and `done` are what `hauto` and the coordinator key on.
   Every write — including one that leaves the phase unchanged — sets
   `written` to the time of that write; `phase_since` is set to the same
   value only when the write changes the phase, and otherwise carried forward
   from what the file already held. Get the time with `date -u +%FT%TZ`.
3. **The build stage reads what the plan names, and nothing else.** No
   `journal:resume`, no directory listings, no neighbouring modules "for
   context", no git history. A symbol the plan did not mention is looked up
   with the `tree-extract` MCP, not by opening files.
4. **Every build and test command is truncated.** Never let a full compiler or
   test log into the conversation:
   ```bash
   cargo build 2>&1 | grep -E "^(error|warning)" -A6 | head -80
   cargo test 2>&1 | grep -E "test result|FAILED|panicked|^error" -A4 | head -60
   ```
   The plan writes the exact commands for the repo; the build stage runs those.
5. **Commit and push only after the play gate's `continue` — or, unattended,
   after every acceptance check has passed — then do it.** A PR cannot open
   without commits. Messages to the rule in `~/.claude-hobby/CLAUDE.md`. The
   notes repo is never committed.
6. **No machine attribution**, anywhere.

## Step 0 — Context and state

Task context from `pwd` (`~/fun_worktrees/PROJECT/TOPIC/TASK`). State file at
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/build-task.json`:

```json
{ "phase": "acceptance", "auto": true, "model": "sonnet",
  "coordinator": "<notes dir, or absent>", "merge": "mine|coordinator",
  "goal": "<path to goal.md, when seeded from one>", "scenarios": ["S1"],
  "brief_source": "coordinator|description|issue", "brief": "<verbatim>",
  "acceptance": "<the done-when observation, verbatim>",
  "context_questions": [], "plan": "plan.md",
  "acceptance_checks": [ { "scenario": "S1", "command": "", "expect": "" },
                         { "scenario": "S6", "hand": true } ],
  "acceptance_result": { "checked": "<RFC 3339>", "passed": false, "rounds": 0,
    "checks": [ { "scenario": "S1", "command": "", "exit": 0, "state": "pass",
                  "note": "" },
                { "scenario": "S6", "state": "hand",
                  "screen": "screens/S6.png", "note": "" } ] },
  "merge_result": { "by": "session", "outcome": "merged|blocked", "pr": "",
    "merge_commit": "", "reason": "", "check": "", "log": "", "tried": [],
    "base": "", "base_updated": true, "checked": "<RFC 3339>" },
  "ci_result": { "checked": "<RFC 3339>", "outcome": "green|blocked", "rounds": 0,
    "case": "", "check": "", "run": "", "log": "", "pr": "" },
  "play": { "command": "", "log": "" }, "feedback": [], "rounds": 0,
  "written": "<RFC 3339>", "phase_since": "<RFC 3339>" }
```

`acceptance_checks` is written by the plan stage, `acceptance_result` by
`verify`; both are absent from an interactive run. A check is keyed by
`scenario` when the run was seeded from a goal and by `name` otherwise; `state`
is `pass`, `fail` or `hand`, and a `hand` check carries `screen` instead of a
command and never decides `passed`. **`auto` is true only when a
coordinator seeded it.** A state file this skill writes for itself sets
`auto: false`, so a `hcdev auto <task>` typed by hand keeps the play gate.
**`merge`** is seeded by the coordinator from its `tasks.json`; **absent means
`mine`**, so a hand-typed `hcdev auto` never merges. `merge_result` is written
by the `merge` phase below. `goal` and `scenarios` are written by `hcoord seed`
and absent otherwise.

Phases: `acceptance`, `await-acceptance`, `plan`, `planned`, `build`,
`implement`, `comments`, `standards`, then either `verify` and `await-fix`
(unattended) or `play` and `await-play` (interactive), then `pr`, `merge`,
`fix-ci`, `journal`, `done`.

The stage argument is required. `plan` runs from whatever phase is recorded up
to `planned`; `build` runs from `planned` to `done`. A `build` invoked with no
`plan.md` on disk says so and stops. A phase past `planned` given `build`
resumes at that phase (this is `hauto`'s `--continue` path).

**The brief** is the seeded state file's `brief` when `brief_source` is
`coordinator`; otherwise the argument after the stage — free text, or `#<n>`
via `gh issue view`. Record it verbatim.

Announce `{project, topic, task, stage, phase}` in one line and run.

______________________________________________________________________

## Stage `plan`

Headless: there is no one to answer a question, so this stage asks nothing
except by stopping at `await-acceptance`.

**P1 — Acceptance.** Does the brief say what done looks like — one thing a
person can do afterwards, and what they should see? Yes when the state file
carries `acceptance`, the issue has an acceptance section, or the text states
an observation. Record it as `acceptance` and go on.

Otherwise: derive **at most three** questions — the acceptance question first
(*what will you do to check this, and what should you see?*), then up to two
the design cannot proceed without. Write them to `context_questions`, set
`phase: await-acceptance`, print them numbered with the line *"Answer in your
own words; `N: skip` is an answer. Then exit this session and the plan
finishes on its own."*, and **end the turn**. `hauto` reopens this session
interactively for me. When my answers arrive: record them verbatim in
`business-context.md` (one `## <handle>` block each, acceptance under
`## acceptance`), write `acceptance`, set `phase: plan`, and continue in the
same turn. Finish by telling me to exit the session.

**P2 — Read the code once.** Find the units the change lands in — the plugin,
system, module or command — and read them, plus whatever they call that the
change touches. Prior art check: `gh pr list --search` and
`gh issue list --search` on the repo, reported in the plan, never a stop.
A `structure.hstruct` at the root is read and its relevant standards named.

**P3 — Turn the acceptance observation into commands.** Unattended only; an
interactive run writes `interactive — play gate` in the section below and skips
this. Each check is a name, one shell command, and what in its exit status or
output means pass. Together they have to amount to the observation itself, not
to a proxy for it: a check that passes while the observation would fail is worse
than no check, because nothing downstream looks again.

When the state file carries `goal` and `scenarios`, there is exactly one check
per owned scenario, keyed by its id. A scenario the goal marks `hand` — the
marker is the word `hand` on a line of its own in the scenario's verbatim text,
which is what `acceptance` holds — gets `{ "scenario": "S6", "hand": true }` and
**no command**. It is not a check that cannot be written; it is one the goal
already said nobody can run, so the `await-acceptance` stop below is not
reached for it and no proxy is invented.

If a scenario the goal did **not** mark `hand` cannot be expressed as commands
at all, **do not invent a proxy**. Write the question — *what command would show me this, and what should
it print?* — to `context_questions`, set `phase: await-acceptance`, print it as
P1 does, and end the turn. That is the gate the coordinator already handles, so
it costs no new kind of stop.

**P4 — Write `plan.md`** into the task's notes dir. It is the build stage's
entire input, read by a session that has never seen this conversation:

```markdown
---
title: "<task> — plan"
created: <YYYY-MM-DD>
tags:
  - plan
---

## Goal
<two or three sentences: what changes for a person>

## Acceptance
<the observation, verbatim>

## Acceptance checks
needs: run            <- only when a check requires the application running
1. `S1` — `<command>` — expect: <what in its output or exit status means pass>
2. `S6` — hand — screen: <the screen the scenario names, and how to reach it>
(unattended only; an interactive run writes `interactive — play gate` here)

## Files
- `path/to/file.rs` — <what changes here, concretely; new file: say so>
(only these are read by the build stage — name every one)

## Steps
1. <ordered, concrete, each naming its file and the shape of the edit>

## Verify
- build: `<the truncated command for this repo>`
- test: `<the truncated command>`
- run: `<how a person starts the app, from the worktree>`

## Standards
<the hstruct standards this touches and which way, or "none">

## Risks
<what was unclear and how it was decided; what might not work; prior art>
```

Decisions in `Risks` are decisions, not reasons: a *why* goes in the plan only
when the brief or `business-context.md` gave it.

Copy the checks into the state file as `acceptance_checks` alongside
`phase: planned` — `verify` runs them from there rather than re-reading prose.
Write the state file, print the plan path and its `Files` list, and end. The
process exits; `hauto` starts the build.

______________________________________________________________________

## Stage `build`

Fresh session. Read `build-task.json`, `plan.md`, and `business-context.md` if
it exists. Nothing else until a step names a file.

**Implement.** Follow `Steps` in order, reading each file when its step comes.
Write comments to the rule as you go — module header, public contract, test
justification, nothing else — taking any *why* from the plan's `Goal` and
`business-context.md` only. Run the plan's `build` command after each step
that compiles, `test` once at the end, both truncated. A plan step that turns
out wrong is fixed in the code, and the deviation is noted for the play gate;
the plan file is not edited.

**No end-to-end run here.** The end-to-end run is the play gate, or `verify`
when unattended. Build and test are the evidence.

Set `phase: comments` and fall through.

**Comments.** `/comment-pass` over the branch diff, naming
`business-context.md` when it exists. Carry its shape-decision notes, declined
findings and context gaps to the play gate. `phase: standards`.

**Standards.** Skip when the repo has no `structure.hstruct`. Otherwise
`/structure`: ratchet a budget the change moved below, try to fix a regression,
carry an unfixed one to the gate. `phase: verify` when `auto` is true, `play`
otherwise.

______________________________________________________________________

## Phase `verify` — the unattended acceptance run

`auto: true` only. This is what stands in for the play gate: the same
observation, run as commands by the session that wrote the code, because there
is nobody to hand a running application to.

1. When the plan's `## Acceptance checks` says `needs: run`, launch the app the
   way the play gate does — detached, output to `<notes dir>/play.log` — wait a
   few seconds and `head` the log. A build error there is fixed before any check
   runs.
2. Run each check in `acceptance_checks` that carries a command, in order,
   capturing its exit status and at most a few lines of its output. Truncate as
   rule 4 requires: a check that prints a compiler log is filtered, not pasted.
3. For each check with `hand: true`, capture the screen its scenario names,
   while the application is still running when the screen is its window:

   ```bash
   hcoord capture <notes dir> <scenario> --window '<window name>'
   <command> | hcoord capture <notes dir> <scenario> --text -
   hcoord capture <notes dir> <scenario> --text <file>
   ```

   The window form is for a screen that is the running application's, the text
   forms for a screen that is a file or a command's output. Record the check as
   `{ "scenario": .., "state": "hand", "screen": "screens/<scenario>.png" }`.
   **A capture that fails is recorded with the failure as its `note` and no
   `screen`, and the run goes on**: the scenario said nobody can run it, so
   there is nothing for a failure to mean beyond a missing picture.
4. Kill the application (`pkill -f` on the recorded command) if one was launched.
5. Write `acceptance_result` into the state file — `checked`, `passed`,
   `rounds`, and one object per check with `scenario` (or `name`), `command`,
   `exit`, `state` and a `note` of at most one line of evidence, plus `screen` on
   a hand check. **Write it whether the checks passed or not**: it is what
   `hcoord wait` reports and what a coordinator has instead of the pane.
   `passed` is true when every check that ran passed; a hand check does not
   count either way.
6. **Every check that ran passes** → `phase: pr`, and continue to the PR section
   in the same turn.
7. **Any fail** → the failure is this session's own, like a failing test. Fix it
   reading only the files the failure names, then build, test, `/comment-pass`
   over what changed, and run the checks again. A hand check is not run again
   and is not a failure to repair. **At most three repair rounds**, counted in
   `rounds`; the cap exists because nobody is at the keyboard to stop a loop
   that is not converging.
8. **Three rounds spent and something still fails** → write the final
   `acceptance_result` with `passed: false`, set `phase: await-fix`, print
   each failed check with its command and evidence, and **end the turn**. No
   commit and no PR: a change that cannot show its own acceptance is not one
   to open a PR for, and a coordinator reading `await-fix` stops its list
   rather than waiting.

______________________________________________________________________

## Phase `play` → the play gate

`auto` false or absent only — I am at the machine and the running application is
what I am handed.

1. Launch the app with the plan's `run` command, detached, output to the
   notes dir:
   ```bash
   setsid -f sh -c '<run command> > <notes dir>/play.log 2>&1'
   ```
   Wait a few seconds, `head` the log: a build error is yours to fix before the
   gate. No `run` in the plan (a library, a tool without a UI) → the play step
   is the acceptance observation run as a command, its output printed.
2. Record `play` in the state file, set `phase: await-play`, and write it.
   **Nothing of the report is printed until that write has happened** — the
   report is what I read and the phase is what the coordinator reads, so a
   report without the write tells nobody who is waiting.
3. Print:
   - **what to try** — the acceptance observation verbatim, and where the app is;
   - the diff, one line per file, and any deviation from the plan;
   - build and test evidence, truncated as run;
   - context gaps with `file:line`, declined comment-pass findings, standards
     delta and any regression, prior art from the plan;
   - the shape-decision notes and the `business-context.md` handles a commit
     body may cite;
   - *"Play with it. Say continue to commit and open the PR, or tell me what
     to change and I'll do it and relaunch."*

   End the turn.

   `hcoord wait` also reports a `play` phase untouched for three minutes as a
   gate on its own — a backstop for this write being missed, not a licence to
   skip it.

**Feedback** is anything but `continue`. Record it verbatim as
`## feedback-<n>` in `business-context.md`, append to `feedback`, kill the
running app (`pkill -f` on the recorded command), implement the change reading
only the files it touches, build, test, comments, standards, play again. No
cap on rounds.

______________________________________________________________________

## Phase `pr` — on `continue`

1. Stop the app (`pkill -f`; ignore failure).
2. **Run the repo's CI locally, before anything is pushed.** Read
   `.github/workflows/*.yml` and `*.yaml` — the one read rule 3 allows that
   the plan did not name, because the workflows are what the PR will be
   judged by. For every job triggered by `pull_request` or `push`, run its
   `run:` steps in order from the worktree root, with the job's `env:` and
   each step's `working-directory:`, output truncated per rule 4. A matrix
   runs once, on this machine's toolchain. Skip `uses:` steps, and any step
   that reads `secrets.` or needs a `services:` container or another OS;
   the skipped steps are listed in the journal, because the PR's checks are
   the only run they get. No workflows → nothing to run.
   A failing step is this session's own, like a failing test: repair it
   reading only the files its output names, re-run the workflow steps from
   the top, increment `ci_result.rounds`. At the bound (three,
   `business-context.md` `## fix-loop-bound`) write `ci_result` with
   `outcome: blocked`, `case: own-diff`, `check` (the step's name) and `log`
   (at most three lines), set `phase: await-fix`, print one line, and **end
   the turn** — nothing is committed or pushed.
3. `git status --short`; stage the task's files and nothing else — no
   artefacts, no `play.log`. One commit unless two decisions would share a
   message. Subject and body to the rule; a justification comes from the
   brief, the acceptance answer, `business-context.md`, or a verified fact.
4. `git push -u origin <branch>`.
5. `gh pr create`: title in commit-subject shape, the three sections and
   nothing else per `~/.claude-hobby/CLAUDE.md`. The manual-test bullets are
   the acceptance observation plus the `run` command. A task that has an issue
   — a `gh-<number>` in the task or branch name, or the issue the brief was
   read from — opens the body with `Closes #<number>`, which is what closes it
   on merge. No issue, no line; never a guessed number. Unattended, the
   manual-test bullets are the acceptance check commands, which are known to
   pass because the PR would not be open otherwise.
6. Record the PR in `prs.json` if the `track-pr` hook did not, and as `pr` in
   the state file — so `merge` is resumable after an interrupt without
   re-reading `prs.json`.

Never push to the default branch; merge only your own PR, only through
`gh pr merge --merge`, and only when `merge` says `coordinator` — `merge:
mine` or absent reports the PR and stops at `done`, which is what holds it
for a person. **Either way the PR's checks are green before `done`**: a PR
handed to me red is a PR I have to fix. Set `phase: merge` and continue in
the same turn.

## Phase `merge` — the session merges its own PR

1. `merge` is `mine` or absent → set `phase: fix-ci` and continue in the
   same turn. The checks are watched and repaired as for `coordinator`;
   only the merge itself is not this session's.
2. `coordinator` → write the state file with `phase: merge`, then
   `hcoord merged <pr url> --ready` with the Bash tool's full budget
   (`timeout: 600000`). At most **four** calls; a `timeout` on the fourth is a
   block with reason `checks still pending after ~38 minutes`.
3. `mergeable` → `gh pr merge <pr url> --merge`. Never `--squash`, never
   `--rebase`, never `--admin`, never a force of any kind. `merged` already →
   go to 4. `blocked` → 6.
4. Confirm, never assume: `hcoord merged <pr url> --timeout 60` must say
   `merged`; take the commit from
   `gh pr view <pr url> --json mergeCommit -q .mergeCommit.oid`. Then
   `git fetch origin <default branch>` and
   `git merge-base --is-ancestor HEAD origin/<default branch>` — a non-zero
   exit is a block, because a `done` reported before the merge landed starts
   the next task off a commit without this work. Finally
   `git -C ~/fun_repos/<project> pull --ff-only` as best effort: a refusal is
   recorded in `base_updated` and does not block.
5. Write `merge_result` with `outcome: merged`, the PR, the merge commit,
   `base`, `base_updated`, `checked` and `tried`; set `phase: journal` and
   continue in the same turn.
6. Blocked — a red check, a conflict, a `gh` refusal, or the wait running
   out: write `merge_result` with `outcome: blocked`, `reason` (one line),
   `check` (the failing check's name when there is one), `log` (at most three
   lines, truncated as rule 4 requires), `pr` and `tried`. **A red check**
   (`hcoord merged`'s `reason` starts with `checks failing:`) sets
   `phase: fix-ci` and continues in the same turn — the failing checks are
   this session's own to look at before anyone is asked. **Everything else**
   (a conflict, a closed PR, a `gh` refusal, or the wait running out) sets
   `phase: await-fix`, prints one line naming the PR and the reason, and
   **ends the turn**. Never rebase, never force, never
   close the PR: `business-context.md` `## merge-conflicts` says a conflict
   means something pushed to the default branch outside the run, which is
   exactly the assumption being violated.

## Phase `fix-ci` — the session repairs its own red checks

A red check on a task's own PR is almost always that task's own diff, and this
session holds what a coordinator does not: the branch, the plan, and the
failing log. `hcoord ci` classifies the failure and applies the bound so a
session that loses count still stops (`business-context.md`
`## fix-loop-bound`).

1. `hcoord ci <pr url> --spent <ci_result.rounds>`, Bash `timeout: 600000`.
2. `outcome: green` → write `ci_result` with `outcome: green`. With
   `merge: coordinator`, back to `merge` step 2, in the same turn: the checks
   have to be watched again from the top, since `hcoord ci` only classifies a
   failure it already saw. With `mine` or absent, set `phase: journal` and
   continue. **`checks: none` is not green while the repo has workflows** —
   the run may not have registered yet. `gh run list --commit <head sha>
   --json status`: a run listed means call 1 again; none listed means no
   workflow triggers on this PR, and `none` stands.
3. `outcome: fix` (`case: own-diff`) → the failure is this session's own,
   like a failing test. Repair it reading only the files `log` names, build
   and test truncated per rule 4, `/comment-pass` over what changed, commit
   to the rule, push. Increment `rounds`, write `ci_result`, go back to 1.
4. `outcome: rerun` (`case: flake`) → `gh run rerun --failed <run>` and
   nothing else: the log says the runner died, not the diff. Increment
   `rounds`, write `ci_result`, go back to 1.
5. `outcome: blocked` → write `ci_result` with `outcome: blocked`, `case`,
   `check`, `log` (at most three lines) and `rounds`; set `phase: await-fix`,
   print one line naming the PR, the case and the reason, and **end the
   turn**. `case: not-mine` and `case: decision` land here regardless of
   `rounds`: neither resolves by trying again, so the bound never applies to
   them.
6. `outcome: timeout` → call 1 again. At most **four** calls per round, the
   same cap the `merge` phase uses; a `timeout` on the fourth is step 5 with
   reason `checks still pending after ~38 minutes`.

`ci_result.rounds` counts CI-fix attempts and `acceptance_result.rounds`
counts the pre-PR repair rounds from the `verify` phase — two counters that
happen to share the number three by decision (`business-context.md`
`## fix-loop-bound`), never one counter: a CI failure is discovered only
after the PR is open, which is after the acceptance rounds have already
finished spending themselves.

## Phase `journal`

`journal:write`: goal, what changed, evidence, feedback rounds, open items
with the context gaps under them. Set `phase: done`, write the state file
(this is what releases the coordinator), print one line, end.

## Notes on scope

- A coordinator may be waiting on the state file. Do not add a stop; anything
  that needs me and fits neither gate becomes an `await-acceptance` with the
  question, and the turn ends.
- A sub-skill's clarifying question is answered from the brief and the
  recorded context, or it is a gap. It never becomes a third gate.
- A task that turns out unimplementable — its done-when cannot be satisfied,
  or the split was wrong — writes `await-fix` with the reason at the point
  that becomes clear, rather than building to a PR and stopping anyway.
