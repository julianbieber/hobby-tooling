---
description: Run a task end to end as one human-gated pipeline with a recorded stage — brief, HMML baseline, annotated delta, spec review loop, implement and test, comment pass, my review, PR, journal
argument-hint: '[<description> | #<issue> | --notes] [<phase>] [--no-hmml] [--resume-deep]'
allowed-tools: Skill, Read, Write, Edit, Bash(pwd:*), Bash(ls:*), Bash(git branch:*), Bash(git -C * status:*), Bash(git -C * diff:*), Bash(git -C * log:*), Bash(git -C * rev-parse:*), Bash(git -C * ls-files:*), Bash(git status:*), Bash(git diff:*), Bash(git log:*), Bash(gh pr list:*), Bash(gh pr view:*), Bash(gh issue view:*), Bash(gh issue list:*)
---

Run a task **end to end** as one predictable, human-gated pipeline. The individual
skills already exist (`journal:resume`, the HMML suite, `comment-pass`,
`journal:write`); this is
the **thin orchestrator** that runs them in a fixed order, stops hard where I take over, and resumes in a
later session exactly where it left off.

Supersedes `/dev-flow`.

**You are glue, not a reimplementation.** Never re-do what a sub-skill owns (HMML
grammar, delta-vs-full-generation). Invoke it and trust its contract. Keep the sequential spine in *this* main-loop context so each
phase reuses the last one's understanding — do not fan work out to subagents
except where a sub-skill already does (`journal:resume`).

## Three rules that override everything else

1. **Hard-stop at the human gates** — after the HMML spec review, and once the
   change is implemented, tested and comment-passed. "Hard stop" means: finish the
   phase, write the state file, print a compact summary of what I need to do, and
   **end your turn**. Do not start the next phase until I say so. **Reading the
   code is mine.** Nothing in this pipeline reviews the diff — `hmml-review` looks
   at the spec and `/comment-pass` at the comments, and that is deliberately all of
   it. Do not add a review pass of your own, and do not print a critique of your
   own change at GATE 2; give me the diff and the evidence.
2. **Never auto-commit** — not the source repo, not the notes repo where the
   `.hmml` baseline lives. That is my call, same contract as every sub-skill.
   Read-only git, `gh pr list` and `gh issue view` are fine. You do write the
   commit *message* when I ask for one, to the rule in `~/.claude-hobby/CLAUDE.md`
   — terse, a record of the decisions, and **no justification you were not
   given**. If a decision in the diff has no justification from my prompts or from
   what we settled, ask rather than inventing one.
3. **No machine attribution** in anything you write — no `Co-Authored-By:
   Claude`, no `Claude-Session:` trailer, no `Generated with Claude Code` footer,
   no 🤖. It is advertising and these repos do not carry it. Comments and commit
   messages have rules in `~/.claude-hobby/CLAUDE.md` and you follow them; PR
   bodies and issue text do not yet, so write those normally.

## Step 0 — Resolve context, brief and phase

1. **Task context** from the worktree path (`pwd`), which is
   `~/fun_worktrees/PROJECT/TOPIC/TASK`: first dir project, second topic, third
   the task ID, verbatim, blurb suffix and all. There is no repo level — the task
   dir is the worktree. Fall back to `git branch --show-current`, skipping
   `master`/`main`/`develop`.

2. **The brief**, from the first argument:

   | argument | brief |
   |---|---|
   | free text | the text itself, recorded verbatim |
   | `#<n>` or `gh-<n>` | `gh issue view <n>` — title, body, acceptance criteria |
   | `--notes`, or nothing | `journal:resume` over the task's notes dir |

   With no argument and a task named `gh-<n>-…`, do both: `journal:resume` **and**
   the issue. Record `brief_source` and the brief itself in the state file, and
   summarize the goal and next step in 2–4 lines. That framing carries through
   every later phase.

3. **State file** at
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/build-task.json`:

   ```json
   { "phase": "plan", "brief_source": "issue|description|notes",
     "brief": "<the brief>", "hmml_unit": "<unit>",
     "no_hmml": false, "size_budget": "", "prior_art": "",
     "rounds": 0 }
   ```

   A leftover `dev-flow.json` in the same directory is read once for its `phase`
   and `hmml_unit`, then ignored — never written to again.

4. **Starting phase**, in this precedence: an explicit phase argument forces it;
   else `phase` from the state file; else **infer** from artifacts and say what
   you inferred — no `.hmml` → `plan`; an **unannotated** `.hmml` (baseline
   captured, no `# +` / `# -` markers yet) → `plan`, resuming at the annotate
   step; an **annotated** `.hmml` (`rg -c '^\s*# [+-]'` on it is non-zero) **and
   a clean worktree** → `await-hmml-review` — annotations are never stripped, so
   an annotated spec alone does not mean the change is unimplemented;
   uncommitted code changes → `implement`; an open PR for the branch → `done`.

5. **Flags:** `--no-hmml` skips the spec phases entirely (plan the change inline in
   a sentence or two, then go to `implement`). `--resume-deep` uses
   `journal:resume-deep`.

Announce the resolved `{project, topic, task, brief_source, phase}` in one line,
then run from that phase onward. Never redo an earlier phase. Write the
state file at **every** transition; do not `git add`/`commit` it.

______________________________________________________________________

## Phase `plan` → GATE 1

**Gate 0 — prior art and size.** Before any spec or code. No sweeping changes
without prior discussion, and do not accelerate past someone else's active work.

- `gh pr list --search "<keywords>" --state all` and
  `gh issue list --search "<keywords>" --state all` on the repo this task targets.
  Report anything overlapping, with its state and author.
- State an expected size: files touched, rough lines changed.
- **Stop and ask** if an open PR or active issue covers this work, or the expected
  change exceeds **10 files or 500 lines**.

Record `size_budget` and `prior_art`. On my own repo this is usually one empty
search; on a repo I do not own it is the difference between a welcome PR and a
closed one.

**The HMML baseline: spec the *current* version first.** Before working out what
the change should be — before any design thinking at all — capture what the unit
does **today**. That spec is the baseline the whole task is stated against;
skipping it leaves the delta with nothing to be a delta *of*. Pick the entry from
what the task is:

- adjusting existing code → `hmml-extract` on that unit
- new behaviour → still baseline first: `hmml-extract` on the code that will
  **host** it (the plugin, the system, the endpoint it lands in), or `hmml-new`
  asked for a blank template when there is genuinely nothing to extract. A sparse
  baseline is a fine baseline.

If genuinely unclear which unit to baseline, ask **one** question. The sub-skill
writes
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.hmml` —
**unannotated**, describing current behaviour only — and stops. Record
`hmml_unit`.

Do not think about the change while doing this, and do not let the intended
behaviour leak into the baseline: a baseline that already contains the change
cannot show what the change is.

**Annotate the intended delta on top of the baseline.** *Now* think about the
change. Edit the `.hmml` so it also expresses the behaviour **this task
intends**, marking every difference with the annotation convention from grammar
§10 — the same file then holds both versions:

- `# - <text>` — a baseline line the change **removes**: comment it out in place,
  keeping its text verbatim (the marker's `-` stands in for the bullet, where the
  line had one) at its original indentation. **Never delete a baseline line
  outright.**
- `# +` — introduces **added** content: the line directly beneath it, plus any
  lines nested deeper than that line. Adjacent added siblings each take their own
  marker; a `struct`/`enum` head is covered through its closing brace.
- A **modified** line is a `# -` immediately followed by a `# +` — never an
  in-place rewrite.

The binding rule: dropping every `# +` block and un-commenting every `# -` line
must reproduce the baseline **exactly**. Re-lint with
`hobby-mml-parser <path> --lint` — annotations are plain comments, so the file
must still parse clean, and the linter checks recoverability directly, reporting
`unrecoverable-baseline` when un-annotating no longer gives a coherent spec (plus
`empty-change-removal` / `empty-change-addition` / `redundant-change-bullet` for
a marker that says nothing).

Cover every section the change touches, not just `## behavior` — an added
component, a new `## systems` entry with its access block, a removed invariant.
A `# +` step whose new component or new invariant is missing gives the review an
incomplete delta.

**Review the delta.** Run `hmml-review` on the annotated `.hmml`: parallel
lens reviewers over the delta and the current code, synthesized into a prioritized
implementation plan in the task notes, scoped to what the annotations add and
remove. Not `hmml-validate` — the annotations are deliberately ahead of the code,
so a conformance check would report the intended change as drift.

**→ GATE 1: HARD STOP.** Set `phase: await-hmml-review`, write the state file,
print:

- the spec path;
- **the annotated delta itself** — every `# -` and `# +` line, grouped by
  section, with a count. This is the change I am approving, so show it rather
  than describing it;
- the implementation plan (path plus an inline summary);
- *"Review the baseline, the `# +`/`# -` delta on it, and the plan. Refine any of
  them, then say `continue` — or say `revise` and I'll re-review what you
  changed."*

End your turn.

## Phase `await-hmml-review` and `revise` — the loop

- `revise` re-runs `hmml-review` on the edited spec, increments `rounds`, returns
  to `await-hmml-review`, and stops again. No cap: this loops until I am happy.
- Each round reports what changed in the **delta** since the previous one — which
  markers were added, dropped or reworded — so I can see whether my edit did what
  I meant.
- `continue` moves to `implement`. Committing the annotated `.hmml` is mine to do;
  if it is still uncommitted when I say continue, say so once and proceed.
  Committing it does not erase the delta the way an unannotated in-place edit
  would — the markers are in the file.

______________________________________________________________________

## Phase `implement`

Write comments to the rule as you go — a module header, a public element's
contract, a test's justification, nothing else. The `comments` phase is a check,
not a laundry: do not write comments expecting it to delete them.

**Implement.** Unless `--no-hmml`, run `hmml-translate <unit>`: it reads the
`# +` / `# -` annotations as the delta — the unannotated baseline around them is
code that already exists — and applies the **minimal** code change, compiles,
tests, and stops. It owns delta-vs-full-generation — do not second-guess it. A
`# -` it declines to delete (the spec is lossy, so absence ≠ dead code) comes back
as a flag for me, not as a silent skip: carry it into GATE 2. With `--no-hmml`,
make the change directly.

**Test.** Drive it end to end: run the app or the affected flow, plus the repo's
own test command (`cargo test`, `cargo clippy`, `just test`, whatever it uses).
Then, unless `--no-hmml`, `hmml-validate` once more — the code should report
**`CONFORMS`** to the annotated spec: every `# +` claim present in the code, every
`# -` behaviour gone from it. Report residual `SPEC-AHEAD` as unfinished work.

**Leave the annotations in place.** The `.hmml` keeps its `# +` / `# -` markers
after the code lands — the spec is the record of *this* change, and stripping it
back to a plain baseline would erase what the task did. Never edit the spec here
at all; committing it stays my call at GATE 2.

Record the evidence, then fall through to `comments` without stopping.

## Phase `comments`

Run `/comment-pass`. It enforces the comment rule over the branch diff in both
directions — deletes what the rule disallows, relocates what carried real
information, and writes the module headers, public contracts and test
justifications that are missing.

Two parts of its report are inputs to the next phase rather than side effects:

- the **shape-decision notes** it pulled out of deleted body comments. Those are
  facts about why the code looks the way it does, and their home is the commit
  message. Carry them into the gate verbatim.
- every finding it **declined to act on** — a test whose justification could not
  be written, a module with no single responsibility, anything it could not verify
  from the code. Each is a decision for me.

Record both, then fall through to `review`.

## Phase `review` → GATE 2

**→ GATE 2: HARD STOP.** Set `phase: await-review`, write the state file, print:

- the diff summary — files, one line each on what changed;
- the test evidence and the validate verdict, plus the spec's `# +` / `# -` delta
  as it now stands — it keeps its annotations, so any marker validate still
  reports as `SPEC-AHEAD` is outstanding work, not a leftover;
- what `/comment-pass` changed, and what it declined to act on;
- the shape-decision notes it extracted, verbatim, for the commit message;
- the size against the Gate 0 budget, and by how much it was crossed;
- *"Review the above. Say continue to open the PR, or tell me what to change."*

End your turn.

______________________________________________________________________

## Phase `pr` — on my go only

Write the description to the rule in `~/.claude-hobby/CLAUDE.md`: a title in the
commit-subject shape, then **What do we change** and **Why do we change** at one or
two sentences each, then **How can you test it by hand** as an ordered bullet list
whose last bullet says what I should see. No other sections, no `## AI use`, no
empty section, no attribution.

The *why* is sourced, not invented — from my prompts, from what we settled, or from
something you verified in the `implement` phase. If you do not have one, ask before
opening the PR.

The manual-test bullets come from what you actually ran in `implement`. If nothing
there was run by hand, say so in one bullet and name what does cover it.

The `track-pr` hook records the PR to `prs.json`. If it did not fire, append the
PR there yourself (create with `[]` first; never store the PR's live state).

Hobby repos are mine, so a PR is optional — for a small solo change, pushing to
the default branch may be right. Ask if it is not obvious, and skip to `journal`
when I say no PR.

## Phase `journal`

Run `journal:write` — goal, what changed, evidence, open items. Set `phase: done`.
Print a one-line wrap-up: task, PR link if any, anything still open. End.

______________________________________________________________________

## Notes on scope

- If a sub-skill asks its own clarifying question, surface it — do not answer on
  my behalf.
- If artifacts and the state file disagree, trust the artifacts, say so, and
  correct the state file.
- If I ask for something off-pipeline mid-run, handle it, then re-anchor on the
  current `phase`.
