---
description: Run a task end to end as one human-gated pipeline with a recorded stage and a live todo list — assess once whether it splits into aspects of the task, brief, the business context only I hold, a short plan, implement and test, read the implemented change back as an HMML delta derived from the diff, comment pass, code quality standards, my review, PR, journal
argument-hint: '[<description> | --issue | --notes] [<phase>] [--no-hmml] [--no-context] [--no-split] [--resume-deep]'
allowed-tools: Skill, Read, Write, Edit, AskUserQuestion, Bash(hobby-struct-parser:*), Bash(pwd:*), Bash(date:*), Bash(ls:*), Bash(rg:*), Bash(git branch:*), Bash(git -C * status:*), Bash(git -C * diff:*), Bash(git -C * log:*), Bash(git -C * rev-parse:*), Bash(git -C * ls-files:*), Bash(git -C * merge-base:*), Bash(git status:*), Bash(git diff:*), Bash(git log:*), Bash(git merge-base:*), Bash(git add:*), Bash(git commit:*), Bash(git push:*), Bash(gh pr list:*), Bash(gh pr view:*), Bash(gh pr create:*), Bash(gh repo view:*), Bash(hcoord ci:*), Bash(hcoord merged:*), Bash(gh run list:*), Bash(gh run rerun:*), Bash(nu -c:*), Bash(cargo:*), Bash(just:*)
---

Run a task **end to end** as one predictable, human-gated pipeline. The individual
skills already exist (`journal:resume`, `hmml-extract`, `comment-pass`,
`structure`, `journal:write`); this is
the **thin orchestrator** that runs them in a fixed order, stops hard where I take over, and resumes in a
later session exactly where it left off.

**You are glue, not a reimplementation.** Never re-do what a sub-skill owns (HMML
grammar, how a delta is derived from a diff). Invoke it and trust its contract. Keep the sequential spine in *this* main-loop context so each
phase reuses the last one's understanding — do not fan work out to subagents
except where a sub-skill already does (`journal:resume`).

**Only facts are settled before code**: what the task is, and the business
context only I hold. Design is not reviewed up front. Once the change is
implemented and tested, it is read back as an HMML delta derived from the diff —
the HMML exists for me to understand what the code change is doing, so it
represents the implemented change, never a plan.

## Four rules that override everything else

1. **Hard-stop at every human gate** — GATE S (the split assessment, once per
   task), GATE C (the business-context questions, before anything is designed),
   and GATE 2 (once the change is implemented, tested, read back as a delta and
   comment-passed). "Hard stop" means: finish the
   phase, set `waiting_on`, write the state file, print a compact summary of what I need to do, and
   **end your turn**. Do not start the next phase, do not touch code, **do not
   create a worktree**, until I say so. **Reading the
   code is mine.** Nothing in this pipeline reviews the diff — the derived delta
   is for me to read, `/comment-pass` looks at the comments and `/structure`
   counts, and that is deliberately all of it. Do not add a review pass of your
   own, and do not print a critique of your own change at GATE 2; give me the
   diff, the delta and the evidence.
2. **Commit the task branch only, and only after GATE 2.** On my `continue` at
   GATE 2 the `pr` phase commits the change to this task's branch, pushes it and
   opens the PR; a CI repair in `ci` commits and pushes the same way. Nothing is
   committed before GATE 2, and **the notes repo is never committed** — the
   state file, the `.hmml` delta, `business-context.md`, the journal: I run
   `hwt sync` for that. Nothing is ever pushed to the default branch; every
   change reaches it through a PR, unless I say otherwise in chat. Commit
   messages follow the rule in `~/.claude-hobby/CLAUDE.md` — terse, a record of
   the decisions, and **no justification you were not given**. If a decision in
   the diff has no justification from my prompts or from what we settled, ask
   rather than inventing one.
3. **Never write a reason I did not give you.** The commit-message and PR rules
   in `~/.claude-hobby/CLAUDE.md` allow exactly three sources for a
   justification: my prompts, what we settled in discussion, or a fact you
   verified in the code. GATE C is where the first two get written down,
   verbatim, in `business-context.md`. A commit that needs a *why* none of them
   supplies is **asked about at GATE 2**, never filled in.
4. **No machine attribution** in anything you write — no `Co-Authored-By:
   Claude`, no `Claude-Session:` trailer, no `Generated with Claude Code` footer,
   no 🤖. It is advertising and these repos do not carry it. Comments, commit
   messages, PR descriptions and issue text have rules in
   `~/.claude-hobby/CLAUDE.md`, and you follow them.

## Step 0 — Resolve context, brief and phase

1. **Task context** from the worktree path (`pwd`), which is
   `~/fun_worktrees/PROJECT/TOPIC/TASK`: first dir project, second topic, third
   the task ID, verbatim, blurb suffix and all. There is no repo level — the task
   dir is the worktree. Fall back to `git branch --show-current`, skipping
   `master`/`main`/`develop`.

   **A code aspect** runs this pipeline too, from its worktree
   `~/fun_aspects/PROJECT/TOPIC/TASK/NAME` (started there by `/task:aspect`).
   Then the task is TASK and the aspect is NAME, and for the whole run:
   - **the notes dir** is `notes/PROJECT/TOPIC/TASK/aspects/NAME/` — the state
     file, the plan, the journal and the `.hmml` delta go there, wherever this
     skill says `notes/{project}/{topic}/{task}/`;
   - `business-context.md` stays the **task's** — one per task, read and
     appended to at `notes/PROJECT/TOPIC/TASK/`;
   - the brief is the aspect's `brief.md`, with the task's `issue.md` as its
     context;
   - **the base** is `aspect.json` `base`, else the default branch — every
     place below that says the default branch means the base, and the PR
     targets it;
   - `is_aspect: true` in the state file, which skips `assess-split`.

2. **The brief**, from the first argument:

   | argument | brief |
   |---|---|
   | free text | the text itself, recorded verbatim |
   | `--issue` | the task's `issue.md` — title, body, acceptance criteria, progress |
   | `--notes`, or nothing | `journal:resume` over the task's notes dir |
   | nothing, in an aspect | the aspect's `brief.md` |

   With no argument and an `issue.md` in the task's notes dir, do both:
   `journal:resume` **and** the issue. Record `brief_source` and the brief itself in the state file, and
   summarize the goal and next step in 2–4 lines. That framing carries through
   every later phase.

3. **State file** at
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/build-task.json`:

   ```json
   { "phase": "assess-split", "brief_source": "issue|description|notes",
     "brief": "<the brief>", "plan": "<plan file name>",
     "hmml_units": ["<unit>"],
     "no_hmml": false, "no_context": false, "no_split": false,
     "is_aspect": false,
     "split": { "assessed": "<YYYY-MM-DD>", "outcome": "none|split",
                "reason": "<one line>" },
     "prior_art": "",
     "context_questions": [], "context_recorded": null,
     "evidence": [ "<command run, and what it showed>" ],
     "commits": [ { "subject": "<subject>", "body": "<body>" } ],
     "pr": { "title": "<title>", "body": "<body>" },
     "todo": [ { "text": "<step>", "status": "pending" } ],
     "waiting_on": null,
     "ci_rounds": 0,
     "written": "<RFC 3339>", "phase_since": "<RFC 3339>" }
   ```

   `phase` is one of `assess-split`, `plan`, `context`, `implement`, `delta`,
   `comments`, `standards`, `review`, `await-review`, `pr`, `ci`, `journal`,
   `handed-off` (this task's work lives in its aspects), `done`. `context` is the waiting state GATE C leaves: the pipeline
   has asked me something and stops until I answer.
   Every write sets `written` to that write's time; `phase_since` moves with it
   only when the write also changes `phase`, and otherwise keeps its old value.

   **`is_aspect`** marks a run in an aspect's worktree. It is set from the path
   at Step 0, never inferred from anything else, and it permanently skips
   `assess-split` — an aspect does not split. **`split`** records the one-time
   assessment so it is never re-asked; it is absent until `assess-split` runs.
   What a split produced is recorded under this task's `aspects/`, written by
   `hwt aspect new`, and by the tasks it filed, and nowhere else.
   **`evidence`**, **`commits`** and **`pr`** are written in `implement` and
   `review`, and are what `pr` commits and opens.

   **Migrate an old record** before anything else, and say so in one line: a
   phase of `await-hmml-review` or `revise` becomes `implement` (its annotated
   `.hmml` and review plan stay in the notes dir as notes; nothing is
   re-reviewed), `hmml_unit` becomes `hmml_units`, and `rounds` is dropped. A
   leftover `dev-flow.json` in the same directory is read once for its `phase`,
   then ignored — never written to again. A state file with no
   `split` record and a phase past `plan` gets
   `split: {"outcome": "none", "reason": "task already in flight"}` rather
   than a back-filled assessment: work found once a task is under way is added
   with `/aspect`. An `is_subtask` key from an older record is dropped, its
   task treated as having `split` recorded; a `subtasks` key is dropped too,
   and a `subtasks.json` is no longer read.

4. **Starting phase**, in this precedence: `phase: assess-split` outranks
   everything below — the split question is answered before any planning, and
   is skipped only when `is_aspect` is true, `no_split` is set, or `split` is
   already recorded. Else an explicit phase argument forces it; else `phase`
   from the state file; else **infer** from artifacts and say what you inferred
   — `business-context.md` has no section for this task and there is no
   `plan-*.md` → `plan`; a section but no plan file → `context`, resuming at the
   plan step (my answers are on disk, the plan is not); uncommitted code changes
   or commits ahead of the default branch → `implement`; an open PR for the
   branch → `done`.

   **`phase: handed-off` is terminal.** This task's work lives in its aspects
   now. Run `hwt aspect list`, and print each aspect's state, live PR state and
   outcome, its `open_items` with no `issue`, and `hwt aspect start <name>`
   beside each still `open`. Then run `hwt done`: it refuses while an aspect is
   unfinished, which only means the task is still waiting, and it needs no
   confirmation from me because the aspects are the whole of this task. End the
   turn. Do not plan, implement, or open a PR here — this task owns no code.

   **The same applies whenever this session is asked.** A status question and a
   message from an aspect are both answered by running `hwt aspect list` again,
   never from what this session saw earlier. A message is a prompt to re-read
   the record, not a report to pass on. In any other phase, report what it says
   and carry on: `hwt done` is only this skill's to run when the task owns no
   code.

5. **Flags:** `--no-hmml` skips the `delta` phase, for a change I can read
   straight from the diff. `--no-context` skips GATE C — for a change I have no
   domain background to give.
   `--no-split` skips `assess-split` for this task, for when I already know it
   is one thing. `--resume-deep` uses `journal:resume-deep`. Record each as its
   state-file flag so a resume does not silently re-add the phase. A
   `size_budget` or `context_gaps` key from an older record is dropped.

6. **Rehydrate before running a phase that is not `plan`.** A resumed session has
   none of the understanding the earlier phases built, and does not need to guess
   — read, in this order: the state file's `brief`, `todo` and `evidence`; the plan file; `business-context.md` — the domain facts I
   dictated at GATE C, in my own words, which are what the change was designed
   from and the only text a commit message may take an unverifiable *why* from;
   the derived `.hmml` delta when `delta` has run; and `git status --short` /
   `git diff`. Then say in two lines what the task is doing and where it stopped.

Announce the resolved `{project, topic, task, brief_source, phase}` in one line,
then run from that phase onward. Never redo an earlier phase. Write the
state file at **every** transition; do not `git add`/`commit` it.

## Live state

`hdash` shows `todo` and `waiting_on` on the task's card, so keep both current
while you work — they are how I see where a session is without opening it.

- **`todo` is the plan.** Seed it from the plan file's steps when the plan is
  written, one line each, every item `pending`. Items only change status —
  `active`, `done`, `dropped`; a step found on the way is inserted where it
  runs. Exactly one item is `active` while work is under way; flip it to `done`
  in the same write that starts the next one. Open items that outlive the task
  go to the journal at `journal`, never into `todo`.
- **`waiting_on`** is one line naming what I decide or do, set at every hard
  stop and when a sub-skill asks me something; `null` again in the write that
  starts the next step.

______________________________________________________________________

## Phase `assess-split` → GATE S (once per task)

The **first** phase of a new task, before any planning. Ask once whether this is
really one piece of work, or several that each want their own worktree, branch,
PR and tab. Then record the answer so it is never asked again.

Two homes for a piece that is cut off, and no third:

- **an aspect** — part of this task, worked in a tab of this task's session,
  started **now** or **later** in the task, but before it is done (`hwt done`
  refuses while one is unfinished). A code aspect is cut from the current tip
  of its base when started and delivers through its own PR to that base.
- **a task** — work planned for some later point that is not part of this
  task. It gets its own card through `/issue-create`, and no tab.

Skip the phase entirely — set `phase: plan` and continue — when `is_aspect` is
true, `no_split` is set, or `split` is already recorded.

**S1 — See whether the branch already holds work.**

```bash
git status --short
git log --oneline "origin/$(gh repo view --json defaultBranchRef -q .defaultBranchRef.name)..HEAD"
```

Commits are never moved between branches. A task whose branch already has
commits or a dirty tree is in flight: record
`split: {"outcome": "none", "reason": "task already in flight"}`, say so in one
line, and continue to `plan`. Work found later is added with `/aspect`.

**S2 — Read the whole task.** Step 0 already resolved the brief; if it came from
a bare description, also run `journal:resume` over the notes dir. This is the
only phase that sees the task before it is carved up, so read all of it.

**S3 — Draft the cut as deliverables, not as components.** Write the candidate
list as the order in which a player or a caller gains capability, *before*
looking at the code layout. Starting from the architecture is what produces
layer tasks, and shaping rule 1 exists to reject exactly those.

**S4 — Test every candidate against the five Task shaping rules** in
`~/.claude-hobby/CLAUDE.md`. That file is the standard and this skill
deliberately does not restate it. State each failure rather than silently fixing
it — a candidate that had to be re-cut is information about the shape of the
work, and I want to see it at the gate. The re-cuts that fix nearly everything:

- **Layer candidate → vertical slice.** "Add the component" becomes one narrow
  thing that is visible in the running game.
- **Carries an open question (rule 4) → make the decision its own item,** and
  make whatever depends on the answer wait. A question is not a deliverable —
  but one whose answer is a file is a research aspect.
- **Leaves state to be remembered (rule 5) → pull the cleanup into the same
  piece,** or make it an aspect that exists now.

Then give each survivor its home: an aspect (code or research; now or later)
when it is part of this task, a task when it is not.

**S5 — Check the set, not just the items.** Two properties the list as a whole
must have, both printed at the gate. Overlapping *files* are allowed — each
aspect has its own worktree and conflicts are resolved at PR time — but say
which files two pieces share.

- **Every piece stands alone** — if backing one out forces another out, they
  are one piece cut on the wrong axis (rule 3).
- **Nothing is left to remember** (rule 5) once every piece has merged — no
  flag, no dual path, no half-done migration.

**S6 — Decide whether to split at all.** Fewer than two surviving candidates
means this is one task: record `split: {"outcome": "none"}`, say so in one line,
set `phase: plan`, and continue into the normal pipeline. **This is the expected
answer for most tasks** — do not manufacture a decomposition to justify the
phase.

**→ GATE S: HARD STOP.** Write the state file with `phase: assess-split` still
set, then print:

- the **end state** in one or two sentences: what is true once every piece has
  landed;
- a table of proposed pieces — name, aspect (`code`/`research`, `now`/`later`,
  base when not the default branch) or task, the user-visible behaviour, the
  done-when observation (rule 2), the files it owns, and any `enabling` label
  with the piece it unlocks;
- the **rejected candidates**, with the rule each failed and how it was re-cut;
- the two set-level checks from S5, each `pass` or the specific pair that
  fails;
- the exact commands the approval will run, one line each, so I can see the
  blast radius before it happens;
- *"Edit, merge or drop any piece, then say continue and I'll create them. Say
  `no split` and I'll carry on as one task."*

End your turn. **Create no worktree, file no issue.**

______________________________________________________________________

## Phase `handed-off` — on my go at GATE S

Only after I approve the decomposition. Per approved aspect, in order:

1. **Add it** — the decisions were made at the gate, so run the primitive
   rather than the interactive `/aspect`:
   ```
   nu -c "use ~/.config/nushell/hwt.nu *; hwt aspect new <name> --kind <kind> --goal '<one line>' [--base <branch>]"
   ```
   Then overwrite its `aspects/<name>/brief.md` in the shape `/aspect` step 6
   gives — goal, done when, why the task needs it, already known, where to
   look, out of scope — read by a session that has no memory of this
   conversation.

Per approved task: run `/issue-create` for it, naming this task in its context.
One that waits on an aspect gets a block, not a sentence in its body:
`nu -c "use ~/.config/nushell/hwt.nu *; hwt block --reason 'waits on aspect <name> of <this task>' --task <t> --project <p>"`,
and `<p>/<t>` in that aspect's `blocks` list in `aspect.json`, so the aspect can
hand it on.

Then, once per task:

2. Set `split: {"outcome": "split", …}` and `phase: handed-off`, and write the
   state file. The aspects themselves are already recorded: `hwt aspect new`
   wrote each one under `aspects/`.
3. Run `journal:write` once, recording the decomposition and *why* each cut fell
   where it did. The rejected candidates belong here too — that reasoning is the
   part that would otherwise be lost, and the aspects' journals will not have it.
   Aspect status is not journal content: `## Current State` says to read
   `hwt aspect list`, so it cannot go stale.
4. Start the aspects worked now:
   `nu -c "use ~/.config/nushell/hwt.nu *; hwt aspect start <name>"` for each —
   it checks the aspect out from its base's current tip and opens its tab. A
   rebase conflict is aborted and reported; stop there and tell me. Print the
   handoff: each aspect with its kind and whether its tab is open or it waits
   as `open` (`hwt aspect start <name>`, or a click in hdash, starts it later),
   each task filed, a note that `hdash` lists the aspects on this task's card,
   and a reminder that each code aspect runs its own `/task:build` and gates
   independently.

**Only the aspects worked now are started here.** **This task implements
nothing** — it owns no code, opens no PR, and does not poll its aspects: each
one messages this session when it finishes (`/task:aspect` step 7). End your
turn.

______________________________________________________________________

## Phase `plan` → GATE C

**Gate 0 — prior art.** Before any code. No sweeping changes without prior
discussion, and do not accelerate past someone else's active work.

- `gh pr list --search "<keywords>" --state all` on the repo this task targets,
  and `rg -il "<keywords>" ~/fun_repos/hobby-mimisbrunnr/notes/<project>/*/*/issue.md`
  for the issues in the notes. Report anything overlapping, with its state.
- **Stop and ask** if an open PR or active issue covers this work.

Record `prior_art`. On my own repo this is usually one empty
search; on a repo I do not own it is the difference between a welcome PR and a
closed one.

**Gate 0b — the standards this repo holds.** If the target repo has a
`structure.hstruct` (at its root, or at the crate root in a workspace), read it
and run `hobby-struct-parser <repo>` for the current numbers. State in one line
each:

- the standards, and which the planned change is likely to move, in which
  direction;
- any reviewed (`review:`) standard whose subject the change touches.

This is the cheap end of the loop. "This adds a module importing eight others and
the standard says six" is a design choice at plan time and rework at review time.
Carry the reading into the plan file.

A repo with no `structure.hstruct` skips this silently — most have none, and a
task is not the place to argue for one.

**Read the code the change will touch.** Enough to know what it does today —
the units, the data, the call paths the brief lands in. No spec is written here:
the delta is derived from the diff once the change exists, and that is where the
baseline comes from.

**Ask for the business context.** The change is not designed yet. This is the
moment to collect what only I have — the context the change has to be designed
inside: which rule sets a bound, what an upstream system guarantees, what a
later reader will break by tidying a shape away, why the obvious approach was
rejected before. Ask now and the answers shape the plan, the code *and* the commit
message. Ask after the code exists and they can only decorate
it.

Skip this step entirely on `--no-context`: set `phase: context` and carry on to
the plan step in the same turn.

First **read what is already written down**: the brief, the task's `issue.md`,
and the task notes. Ask for nothing recoverable from those — quote it instead.

Then derive the questions from the goal read against the code: what does
building this need to know that the current code cannot tell you? **At most
seven questions**, each

- anchored to something named — a file, a unit, a field, a rule in the brief;
- about the **system**: what produces this, what consumes it, which rule forces
  it, what goes wrong without it;
- open, with no draft answer and no leading guess — a proposed sentence with a
  question mark gets rubber-stamped, and then the reasoning is yours, not mine;
- carrying the one line you found while looking (`what I found: …`), or an
  explicit blank (`nothing in the notes or the issue covers this`).

**Nothing worth asking?** Say so in one line (`the brief is self-contained — no
business context needed`), set `phase: context`, and carry straight on to the
plan step **without** stopping. A gate with nothing behind it is friction.

**→ GATE C: HARD STOP.** Otherwise set `phase: context`, write
`context_questions` verbatim (so a resumed session re-asks *these* rather than
inventing new ones), set `waiting_on` to `answer the GATE C questions`, write
the state file, then print the numbered questions and one line: *"Answer any of
these in your own words — `N: skip` if there is nothing to say, and a skip is a
real answer. Then say continue and I'll plan and build off your answers."* End
your turn.

______________________________________________________________________

## Phase `context` → `implement`

The waiting state written at GATE C. Entered when my answers come back.

**The `context` phase argument re-opens it at any point** — the pipeline's one
loop, and how more of what I know gets in after I have seen something. Invoked
later, it derives its questions from whatever prompted the top-up (usually the
delta I just read at GATE 2, at a `file:line` you can now show me), appends the
answers to the same section, and returns to `implement`.

**Record my answers, verbatim.** Append them to
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/business-context.md`
(create it with the notes front matter — `title: "{task} — business context"`,
today's `created`, tag `context`), one `## <short handle>` block per answer:

```markdown
## wave-cap
Q: why cap concurrent spawners rather than queueing the overflow?
The spawn budget is what keeps the frame time under the 16ms target on the
Steam Deck; a queue would hide the cap and stutter instead of dropping a wave.
```

**Copy my wording. Never paraphrase, summarize, complete or tidy it.** This file
is the only text a commit message may take an unverifiable *why* from, so a
sentence you smoothed here becomes a claim I never made. Record a skip too — the
handle with the body `skipped — no context to give`. A question I leave unanswered when I say continue is a
skip: record it as one rather than asking again.

Write `context_recorded` (today's date), clear `waiting_on`, and carry on in the
same turn.

**Write a short plan.** One file,
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/plan-{YYYY-MM-DD}-{slug}.md`,
in the plan format of `~/.claude-hobby/CLAUDE.md` — Goal, Approach, Steps, Key
Files, Open Questions — and short: the steps are the order you will work in,
one line each, naming files. A recorded answer the change has to honour is
cited in the step it shapes by its `business-context.md` handle, so it is not
carried in your head to be remembered at the commit. A new dated file per
planning session; never overwrite an earlier one. Record its name as `plan` and
seed `todo` from its steps (**Live state**). The plan is not a gate: say in one
line where it is, set `phase: implement`, and carry on.

______________________________________________________________________

## Phase `implement`

Entered from `context` in the same turn, or on a resume. Work from the plan
file and keep `todo` moving as you go. A design question only I can answer is
asked when it comes up, as one line with the options you see — set
`waiting_on`, stop, and resume at the same `todo` item. It is not saved for a
gate, and it is not answered on my behalf.

Write comments to the rule as you go — a module header, and a public element's
comment only where it tells the caller something the signature does not. Write
for a reader who never saw this conversation. The `comments` phase is a check,
not a laundry: do not write comments expecting it to delete them.

**Test.** Drive it end to end: run the app or the affected flow, plus the repo's
own test command (`cargo test`, `cargo clippy`, `just test`, whatever it uses).
Then **run the repo's CI locally**: for every job in `.github/workflows/*.yml`
and `*.yaml` triggered by `pull_request` or `push`, its `run:` steps in order
from the worktree root, with the job's `env:` and each step's
`working-directory:`. A matrix runs once, on this machine's toolchain. Skip
`uses:` steps and any step that reads `secrets.` or needs a `services:`
container or another OS, and carry the skipped ones into GATE 2. A failing step
is fixed here like a failing test, so the diff I review is one CI is expected to
pass. No workflows → nothing to run.

Record the evidence under `evidence`, one line per command run, then fall
through to `delta` without stopping. **Commit nothing here.**

## Phase `delta`

Skip with `--no-hmml`, and for a diff with no behaviour change — docs, tests
only, config values, a mechanical rename — saying so in one line.

Run `/hmml-extract --against <ref>`, with `<ref>` the merge base of the
worktree and the default branch
(`git merge-base HEAD "origin/$(gh repo view --json defaultBranchRef -q .defaultBranchRef.name)"`).
It picks the units the diff touches, writes each as the worktree leaves it with
what the change did marked `# -` / `# +`, and lints the set; if it asks which
units, surface the question. Record `hmml_units`.

The delta is **derived, never maintained**: do not edit it, and rerun this phase
after any later change to the code's behaviour — a fix I ask for at GATE 2, a
CI repair. It is for me to read; it is not a review of the change and it does
not gate anything. Fall through to `comments`.

______________________________________________________________________

## Phase `comments`

Run `/comment-pass` over the branch diff. It deletes what the comment rule
disallows, cuts comments pitched at the wrong level or written for the
conversation, and writes missing module headers and the public comments a caller
needs.

Carry two parts of its report to GATE 2:

- the **shape-decision notes** it pulled out of deleted body comments — facts
  about why the code looks the way it does, whose home is the commit message;
- anything it **declined to act on** — a module with no single responsibility, a
  name that needs changing, a behaviour it could not verify. Each is a decision
  for me.

Then fall through to `standards`.

______________________________________________________________________

## Phase `standards`

Skip entirely if the target repo has no `structure.hstruct`.

Run `/structure` on the target repo. Three outcomes, and only one needs you to
do anything:

- **Unchanged** — nothing to report.
- **Slack** — the change improved a number. Run the ratchet; the tightened budget
  joins this task's diff, which is how an improvement gets recorded by the change
  that made it.
- **Exceeded** — the change moved the repo further from a standard it states.
  **Try to fix it**: read `--detail` to find what this change added, and repair
  it. A new cycle usually wants an event or a read-only query rather than an
  import; a file crossing the line usually wants the split its responsibility
  already implies. Re-run after the fix.

  **If the fix does not work, carry it to GATE 2 as a regression** — the standard,
  the count before and after, and what you tried. Do not route around it and do
  not raise the budget. Raising is a hand edit I make deliberately, because it
  says in the diff that this project got further from its own standard.

Then check the **reviewed** standards. A `review:` standard is only re-audited if
something asks, so read each one against this diff: if the change touches what it
covers, say so and ask whether to re-audit and bump `audited:`. Do not bump a date
for a standard the change did not touch, and do not treat an old date on an
unrelated standard as a finding.

Record the delta, then fall through to `review`.

______________________________________________________________________

## Phase `review` → GATE 2

**→ GATE 2: HARD STOP.** Set `phase: await-review`, set `waiting_on` to
`read the change at GATE 2`, write the state file.

GATE 2 is **layered**: it prints the overview, and I zoom in on request. The
overview is what I read every time, so keep it short enough to read in one go.

**The overview — printed:**

- **what changes**, in two or three sentences, for a person using the program,
  not in terms of files;
- **the derived delta**, at behaviour level — each `.hmml` path, then every
  `# -` and `# +` line in its behaviour, invariant and access sections, grouped
  by section. It is the cheapest place to catch a change that does something the
  brief did not ask; show it, do not describe it. With no delta (skipped, or
  `--no-hmml`), say so in one line;
- **evidence** in one line — what was run and that it passed — plus any CI step
  skipped locally;
- **what needs my decision**, and only that: a standards regression you could not
  fix (what you tried), anything `/comment-pass` declined, a commit that needs a
  justification no handle or verified fact gives. None → say `nothing to decide`;
- **the proposed commits** — subject and the files each takes; the full bodies
  are in the state file and shown on `messages`;
- **the proposed PR title**;
- *"Say `design` for the module and API level, `diff` for the files, `messages`
  for the full commit and PR text. Say continue to commit, push and open the PR,
  or tell me what to change."*

**On request — printed only when I ask:**

- `design` — the program's shape: per module touched, its header and every
  public signature added, changed or removed, with its comment; how the modules
  now depend on each other where that changed; and the `kind: module` `## api`
  delta from the `.hmml` set when `delta` extracted one. Then the standards delta,
  each budget before and after;
- `diff` — the files, one line each on what changed, then whatever part of the
  diff I name;
- `messages` — every proposed commit message in full and the PR body, to the
  rules (see `pr`), with the shape-decision notes and the `business-context.md`
  handles they cite.

After a zoom, end the turn again; the gate still holds.

Record the proposals as `commits` and `pr` in the state file, so a resumed
session opens exactly what I approved. A change I ask for here goes back
through `implement` and `delta` before the gate is printed again; an edit to a
message only rewrites the proposal. End your turn.

______________________________________________________________________

## Phase `pr` — on my go only

**The PR rule**, which the proposal at GATE 2 is written to: a title in the
commit-subject shape, then **What do we change** and **Why do we change** at one or
two sentences each, then **How can you test it by hand** as an ordered bullet list
whose last bullet says what I should see. No other sections, no `## AI use`, no
empty section, no attribution.

The *why* is sourced, not invented — from my prompts, from what we settled, or from
something you verified in the `implement` phase. If you do not have one, ask at
GATE 2 rather than proposing one.

The manual-test bullets come from what you actually ran in `implement`. If nothing
there was run by hand, say so in one bullet and name what does cover it.

On my `continue`, from the approved `commits` and `pr` in the state file:

1. Make each commit in order — `git add` its files, `git commit` with its
   message. The source repo only; the notes repo is never committed.
2. `git push -u origin HEAD` — the task branch, never the default branch.
3. `gh pr create --title '<title>' --body '<body>'` against the default branch.

The `track-pr` hook records the PR to `prs.json`. If it did not fire, append the
PR there yourself (create with `[]` first; never store the PR's live state).
**In an aspect** the hook does not fire and nothing goes to `prs.json`: open
the PR with `--base <the base>`, and set `aspect.json` `state` to `pr` and its
`pr` to the URL (Edit, keep every other key). Then, for each `<p>/<t>` in its
`blocks`: `nu -c "use ~/.config/nushell/hwt.nu *; hwt block --pr <url> --task <t> --project <p>"`.
Set `phase: ci` and continue.

**Every change reaches the default branch through a PR.** Push to the default
branch only when I say so in chat for this task, and then skip `ci` for
`journal`. An aspect's PR targets its base, and the aspect is done only once
that PR has merged.

## Phase `ci` — the PR is green before the pipeline ends

The local run in `implement` cannot reproduce `uses:` steps, secrets, service
containers or another OS, so the PR's own checks are the authority. Watch them
and repair them; never hand me a red PR as finished.

1. `hcoord ci <pr url> --spent <ci_rounds>`, Bash `timeout: 600000`. At most
   four calls on `timeout`; a fourth is step 5 with
   `checks still pending after ~38 minutes`.
2. `outcome: green` → done here, on to `journal`. `checks: none` while the repo
   has workflows is not green yet: `gh run list --commit <head sha> --json
   status` — a run listed means call 1 again; none listed means no workflow
   triggers on this PR.
3. `outcome: fix` → the failure is the diff's own. Repair it reading the files
   `log` names, run the failing step locally, `/comment-pass` over what changed,
   commit to the message rule, push, and rerun `delta` when the repair changed
   behaviour. Increment `ci_rounds`, back to 1. The repair is shown in the
   wrap-up, because it landed after GATE 2.
4. `outcome: rerun` → `gh run rerun --failed <run>` only. Increment
   `ci_rounds`, back to 1.
5. `outcome: blocked` (`not-mine`, `decision`, or the bound of three spent) →
   print the PR, the case, the failing check and at most three log lines, set
   `waiting_on`, and **stop**: the next step is mine.

## Phase `journal`

**In an aspect**, wait for the merge first: `hcoord merged <pr url>`, Bash
`timeout: 600000`, again on `timeout`. `blocked` → print the reason, set
`waiting_on`, and stop; a respawned tab re-checks. An aspect's done-when was read
at GATE 2, so the merge is what is left.

Run `journal:write` — goal, what changed, evidence, open items.

**In an aspect**, run `/task:aspect finish`: it records the outcome and open items
in `aspect.json` and tells the task.

Set `phase: done`.
Print a one-line wrap-up: task, PR link if any, anything still open. End.

______________________________________________________________________

## Notes on scope

- If a sub-skill asks its own clarifying question, surface it — do not answer on
  my behalf. The pipeline asks for wording of its own in exactly one place,
  GATE C, and what I answer there is recorded verbatim: no phase and no
  sub-skill may smooth it into prose of yours.
- **This pipeline is one branch.** Work found on the way that is part of this
  task but outside this branch's change is an aspect — `/aspect`, now or later;
  work that is not part of this task is a task for later, `/issue-create`.
- If artifacts and the state file disagree, trust the artifacts, say so, and
  correct the state file.
- If I ask for something off-pipeline mid-run, handle it, then re-anchor on the
  current `phase`.
