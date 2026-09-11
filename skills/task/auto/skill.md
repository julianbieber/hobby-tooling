---
description: Run a task unattended — the /task:build pipeline with its four gates collapsed to two, acceptance criteria at the start only if the brief does not already say what done looks like, and the running app handed to me at the end before the PR opens
argument-hint: '[<description> | #<issue>] [<phase>] [--no-hmml]'
allowed-tools: Skill, Read, Write, Edit, AskUserQuestion, Bash
---

Run a task **unattended**. This is `/task:build` with the gates it stops at
reduced to the two a person actually needs: **acceptance** at the start, only
when the brief does not already say what done looks like, and **play** at the
end, where the running application is put in front of me and the PR waits for
my `continue`. Everything between runs without stopping.

**Read `~/.claude-hobby/commands/task/build.md` first** and run its phases with
their sub-skills exactly as it describes them. This file says only what differs.
Where the two disagree, this file wins; where this file is silent, `/task:build`
governs.

## Five rules that override everything else

1. **Two hard stops, and no others.** GATE A (acceptance, conditional) and
   GATE P (play, always). GATE S is skipped (`no_split: true` — a task under a
   coordinator was split already, and a task typed by hand is taken as one
   task). GATE C's questions fold into GATE A. GATE 1 does not stop: the spec
   review's plan is taken and implemented. GATE 2 becomes GATE P. A "hard stop"
   still means: write the state file, print the summary, end the turn.
2. **Never write a reason I did not give you.** Same three sources as
   `/task:build` — my prompts, what is recorded in `business-context.md`, a
   fact verified in the code. Nothing is asked mid-run, so a *why* the recorded
   context does not supply is a **gap**, reported at GATE P, never filled in.
3. **The state file is a contract with the coordinator.** `build-task.json` is
   written at **every** transition, and every waiting phase is named
   `await-<something>`: `hcoord wait` returns to `/task:coordinate` on exactly
   those names, plus `done`. A gate whose phase name does not start with
   `await-` hangs the coordinator.
4. **Commit and push only after GATE P's `continue`, and then do it.** A PR
   cannot open without commits, and I am not at the keyboard in between, so this
   pipeline commits — unlike `/task:build`. Messages to the rule in
   `~/.claude-hobby/CLAUDE.md`: terse, decisions and their justifications,
   nothing invented. The notes repo is never committed here.
5. **No machine attribution**, anywhere.

## Step 0 — Context, brief, phase

As `/task:build` Step 0, with these differences:

- **The brief** may come from a **seeded state file**: a coordinator writes
  `build-task.json` with `brief_source: "coordinator"`, `brief`, `acceptance`,
  `auto: true`, `coordinator: <its notes dir>`, `no_split: true`,
  `is_subtask: true` before this session starts. With no argument and such a
  file, the brief is that — do not run `journal:resume` over an empty notes
  dir. Otherwise: free text, `#<n>`, or `journal:resume`, as in `/task:build`.
- **State file** phases here: `acceptance`, `await-acceptance`, `plan`,
  `implement`, `comments`, `standards`, `play`, `await-play`, `pr`, `journal`,
  `done`. Extra fields: `auto: true`, `acceptance` (the done-when observation,
  verbatim), `coordinator` (notes dir, or absent), `play` (how the app was
  launched, and the log path), `feedback` (list of GATE P rounds).
- **Starting phase**: `phase` from the state file; else infer as `/task:build`
  does, mapping `await-hmml-review` → `implement` and `await-review` → `play`.
  A brand-new task starts at `acceptance`.
- **Rehydrate** on resume as `/task:build` does, and additionally read
  `acceptance` and `feedback`.

Announce `{project, topic, task, brief_source, phase}` in one line and run.

______________________________________________________________________

## Phase `acceptance` → GATE A (conditional)

Decide whether the brief already says **what done looks like**: one concrete
thing a person can do afterwards, and what they should see (Task shaping rule 2
in `~/.claude-hobby/CLAUDE.md`). It does when the state file carries
`acceptance`, when the issue body has an acceptance section, or when the free
text states an observation. Quote it, record it as `acceptance`, set
`phase: plan`, and carry on **without stopping** — this is the expected path
under a coordinator.

Otherwise, and only then, ask. Derive **at most three** questions:

- the acceptance question itself — *what will you do to check this, and what
  should you see?* — always first;
- up to two of the GATE C kind, only where the delta genuinely cannot be
  designed without an answer (which rule sets a bound, what the obvious
  approach must not break). Read the notes, the issue and the code first and
  ask nothing recoverable from them.

**→ GATE A: HARD STOP.** Set `phase: await-acceptance`, write
`context_questions` and the state file, print the numbered questions and one
line: *"Answer in your own words — `N: skip` is an answer. Then say continue and
I'll run the rest unattended; the next time I stop, the app will be running for
you."* End your turn.

On my answers: record them **verbatim** in `business-context.md` exactly as
`/task:build`'s `context` phase does (one `## <handle>` block each; the
acceptance answer under `## acceptance`), write `acceptance` into the state
file, set `phase: plan`, and continue in the same turn.

______________________________________________________________________

## Phase `plan` — no stop

Gate 0 (prior art, size) and Gate 0b (standards) as `/task:build`, but they
**report rather than stop**: an overlapping open PR or issue is named in the
journal and at GATE P; a change over the size budget is noted and carried on.
The one exception: an open PR **by someone else** covering this work on a repo I
do not own — stop with `await-acceptance` and say so, because continuing would
be accelerating past their work.

Then, unless `--no-hmml`: baseline (`hmml-extract` / `hmml-new`), annotate the
delta with `# +` / `# -` off the brief, the acceptance observation and whatever
`business-context.md` holds, lint it, run `hmml-review` naming the context
file. **Do not stop.** Take the plan it wrote and set `phase: implement`. With
`--no-hmml`, plan inline in a few lines.

The acceptance observation is an invariant the delta must honour — write it
into the spec as one, the way `/task:build` says a recorded answer belongs in
the spec and not in your head.

## Phases `implement`, `comments`, `standards` — as `/task:build`

Verbatim, without the stops. Three things to carry to GATE P: `hmml-translate`'s
declined `# -` flags, `/comment-pass`'s context gaps and declined findings and
shape-decision notes, and a standards regression the fix did not clear. Record
each in the state file as `/task:build` names them, then fall through to
`play`.

______________________________________________________________________

## Phase `play` → GATE P (always)

The point of this pipeline: I arrive to a running application, not to a diff.

1. **Find how a person runs it**, in this order: a `just run` recipe in the
   worktree; `cargo run` when `Cargo.toml` has a binary; whatever `README.md`
   or `CLAUDE.md` in the repo says. If nothing opens — a library, a script, a
   tool with no UI — the play step is the acceptance observation itself: run
   it, capture the output, and print it.
2. **Launch it detached from this pane**, from the worktree, with its output in
   the notes dir so a crash is readable afterwards:
   ```bash
   setsid -f sh -c '<run command> > <notes dir>/play.log 2>&1'
   ```
   A dev build is fine; a release build is not worth the wait unless the repo's
   run recipe already says so. Wait a few seconds and check the log's first
   lines: a build error here is your problem to fix before the gate, not mine.
3. **Notify**: `notify-send "<task>" "ready to play"` when `notify-send` exists.
   Ignore its failure.
4. Record `play: { "command": …, "log": … }` in the state file.

**→ GATE P: HARD STOP.** Set `phase: await-play`, write the state file, print:

- **What to try**: the acceptance observation, verbatim, and where the app is
  (window title, or the command that was run, and the log path);
- the diff summary, one line per file;
- test evidence and the validate verdict, as `/task:build` GATE 2 lists them;
- the context gaps with `file:line`, the comment pass's declined findings, the
  standards delta and any regression, the size against Gate 0, and the prior
  art — each in a line or two, none omitted;
- the shape-decision notes and the `business-context.md` handles a commit body
  may cite;
- *"Play with it. Say continue to commit and open the PR, or tell me what to
  change and I'll do it and relaunch."*

End your turn.

### Feedback at GATE P

Anything but `continue` is feedback. Record it **verbatim** as a new
`## feedback-<n>` block in `business-context.md` (it is a thing I said, and the
commit message may cite it), append `{round, said}` to `feedback` in the state
file, and return to `implement` with the feedback as the delta: edit the `.hmml`
annotations if the spec is affected, implement, test, `comments`, `standards`,
then `play` again. Kill the previous app instance first (`pkill -f` on the
recorded command) so I am not looking at two windows. No cap on rounds.

______________________________________________________________________

## Phase `pr` — on my `continue`

1. **Stop the running app** (`pkill -f` on the recorded command; ignore failure).
2. **Commit.** Inspect `git status --short` and stage the change — every file
   the task touched, and nothing it did not: no build artefacts, no editor
   files, no `play.log`. Prefer one commit; split only where two decisions would
   otherwise share one message. Subject and body to the rule in
   `~/.claude-hobby/CLAUDE.md`. A justification comes from the brief, the
   acceptance answer, `business-context.md` (feedback blocks included), or a
   verified fact — and from nowhere else. A decision none of those cover gets
   no justification in the body, and is named at the end of your GATE P
   summary next time rather than invented now.
3. **Push** with `git push -u origin <branch>`.
4. **Open the PR** with `gh pr create` — title in commit-subject shape, the
   three sections and nothing else, exactly as `/task:build`'s `pr` phase and
   `~/.claude-hobby/CLAUDE.md` describe. The manual-test bullets are the
   acceptance observation plus what you ran at `play`, in order.
5. Record it in `prs.json` if the `track-pr` hook did not.

Never push to the default branch: an unattended pipeline opens a PR every time,
and merging is mine.

## Phase `journal`

`journal:write` — goal, what changed, evidence, the feedback rounds, the open
items with the context gaps under them. Set `phase: done`, write the state
file (this is what releases the coordinator), and print one line: task, PR
link, anything still open. End.

______________________________________________________________________

## Notes on scope

- **A coordinator may be waiting on this file.** Every phase change is a write
  to `build-task.json`, and the two gates are named `await-acceptance` and
  `await-play`. Do not add a third stop; if something needs me that neither
  gate covers, set `await-acceptance` with the question and end the turn.
- If a sub-skill asks its own clarifying question, answer it from the brief,
  the acceptance answer and the recorded context where you can; where you
  cannot, that is a GATE A question, and it stops there.
- Anything off-pipeline I ask for mid-run: do it, then re-anchor on `phase`.
