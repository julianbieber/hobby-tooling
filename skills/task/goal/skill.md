---
description: Interview me toward a goal file — record my intent verbatim, capture the screens that exist, draft the target screens and scenarios with every guess marked, ask those questions open, record my answers verbatim — then split the scenarios into the coordinator's task list without filing anything
argument-hint: '<blurb> [<intent>]'
allowed-tools: Skill, Read, Write, Edit, Bash
---

The one session where I work closely with the model, at the front of a body of
work: it produces `goal.md`, the file every later artefact quotes and never
paraphrases, and the coordinator's `tasks.json` split from it. The format, and
what each section closes, is `skills/task/goal/README.md`; `hcoord goal lint` is
the check.

## Rules that override everything else

1. **My words are recorded verbatim.** `## Intent` and `## Decisions` hold my
   prose byte for byte — no rewrap, no tidying, no spelling fixes.
2. **Every guess is a `?`, and every `?` is a question.** A `?` is never
   resolved by deciding. `hcoord goal lint` fails while one is left, and its list
   of them is the question list.
3. **Open where the answer is a picture.** No option list and no `Recommended`
   on a UX question. A closed question is for a binary technical choice only, and
   says that it is one. `AskUserQuestion` is not available here on purpose: an
   answer picked from options is not my words.
4. **A screen that exists is captured, never drawn from the code.** Only a
   `(target)` screen is drawn.
5. **Nothing is filed and nothing is built.** The output is `goal.md` and
   `tasks.json`. `/issue-create` writes an entry up; `hcdev coordinator` runs the
   list.
6. **The notes repo is never committed**, and there is no machine attribution
   anywhere.

## Step 0 — Where, and which blurb

`pwd` must be `~/fun_repos/<project>` — a main checkout. A worktree or anywhere
else: say so and stop.

The blurb is the first argument and must match `[A-Za-z0-9._-]+`. Absent or
malformed: say so and stop.

```
goal          ~/fun_repos/hobby-mimisbrunnr/notes/<project>/planning/<blurb>/goal.md
screens       <planning dir>/screens/<name>.png
the split     ~/fun_repos/hobby-mimisbrunnr/notes/<project>/coordinator/<blurb>/tasks.json
```

A `goal.md` already there is **resumed, never replaced**: one goal per blurb.
Read it, report which sections are filled and where the `?`s are
(`hcoord goal lint <goal.md>`), and continue at the first step with work left.

Announce `{project, blurb, goal path, step}` in one line.

## Step 1 — The intent, verbatim

The rest of the arguments is the intent. Empty: ask for it in one line — a few
lines of what I want, which will not be edited — and end the turn.

Write the file's frontmatter (`title: "<blurb> — goal"`, `created:`, `tags:` with
`goal`) and `## Intent` holding my prose as a blockquote, one `> ` per line and
nothing added.

## Step 2 — Current state, and the screens that exist

Find how the app runs — the project's `justfile`, `README.md`, `Cargo.toml`
binaries — and **print the command before running it**. Launch it detached, its
output in the planning dir:

```bash
setsid -f sh -c '<run command> > <planning dir>/goal.log 2>&1'
```

Wait a few seconds and `head` the log; a build error is reported to me, not
repaired here. A project whose screens are all files or printed output launches
nothing and says so in one line.

Capture every screen the intent touches with `hcoord capture`, the one mechanism
this skill uses. It covers a window and a file with the same command and refuses
a title matching two windows rather than guessing:

```bash
hcoord capture <planning dir> <screen name> --window '<title substring>'
hcoord capture <planning dir> <screen name> --text <file>
<command> | hcoord capture <planning dir> <screen name> --text -
```

It writes `<planning dir>/screens/<name>.png` and prints the relative path. A
screen name may hold letters, digits, `.`, `_` and `-`. Stop the app when the
captures are taken (`pkill -f` on the launched command).

Each capture becomes `### <name> (exists — screens/<name>.png)` under
`## Screens`, its ASCII **transcribed from the picture**. A directory tree the
intent is about is a screen too, under `## Project on disk`.

`## Current state (<date>, captured before anything was asked)` is written from
the code, in the areas the intent touches. It is the one part of the file that
is not a picture.

## Step 3 — The draft, with every guess marked

Write the rest of the file to the format in `skills/task/goal/README.md`:

- one `### <name> (target)` per screen the change alters: the drawing, then
  `changes:` / `unchanged:` / `hides:`;
- `## Scenarios`: `### S<n> <title>    screen: <name>`, numbered steps of one
  gesture each, exactly one `->` holding the single observation, and a
  `widgets:` line. `hand` on a line of its own **only** when no command can
  produce the observation: it says the screen is worth a picture at judgement,
  and is not a way out of acceptance;
- `## Widgets`: one row per affordance a scenario names, `exists` as `yes`,
  `no` or `in feathers, not in this app`; anything not `yes` names what provides
  it;
- `## Left to the build`, `## Tunables`, `## Not this time`.

Every uncertainty is a `?` **in the place it belongs** — inside the drawing, on
the step, in the row — and is not decided. Then:

```bash
hcoord goal lint <goal.md>
```

Each `?` comes back as `open question` with its line. That is the list for the
gate.

## GATE 1 — The questions

Print them numbered and grouped by screen, each quoting the drawing line or step
it sits in with its `file:line`. A question whose answer is a picture is asked
open — *what should this look like?* — with nothing offered. A closed question is
asked only for a binary technical choice and says so.

End with one line: *answer in your own words; `N: skip` is an answer and moves
that `?`'s subject to `## Left to the build`.* **End the turn.**

## Step 4 — The answers, and the lint

Record each answer verbatim under `## Decisions (my words, verbatim)` as
`- **<handle>** — "<my words>"`, the handle kebab-case and naming what it
settles. Resolve each `?` the answer covers — in the drawing, the step or the
row — **from my words only**. An answer that raises a new `?` goes back to
GATE 1 and is not decided here.

Then `hcoord goal lint <goal.md>` must print `ok`.

## GATE 2 — Read it once

Print the goal's path, the lint line, the scenario ids with their titles and
which are `hand`, and one line per screen section saying whether a png exists
under `screens/`. An `(exists)` screen without one is named as a gap. A
`(target)` screen has no picture by construction, and its line says that instead
of rendering the sketch as a substitute.

Then: *"Read it once. Say `split` and I'll carve the scenarios into the
coordinator's task list, or tell me what to change."* **End the turn.** A change
is made to the file and this gate is repeated.

## Step 5 — The split

On `split`, run `/task-split` over the intent and the scenarios. It files
nothing.

**Every scenario id is owned by exactly one task, and every task owns at least
one** — that is what `hcoord goal lint --tasks` checks. A decision the split
wants made first therefore cannot be a task here. The decisions of this work were
made at GATE 1, which is what this session is for; a question still open is
answered at that gate and not carried into the list as a scenario-less task the
lint rejects.

Per task:

| field | value |
|---|---|
| `task` | `<blurb>-<slug>`, lowercase `[a-z0-9-]`, at most 40 characters — no issue is filed yet, so no `gh-<n>` |
| `topic` | the blurb |
| `title` | one line |
| `scenarios` | the owned ids |
| `done_when` | names the ids and quotes each observation as the goal states it |
| `depends_on` | task names, ordered so a widget that does not exist is provided before the scenario needing it, with `enabling -> <task>` in `note` |
| `status` | `"todo"` |

Create `~/fun_repos/hobby-mimisbrunnr/notes/<project>/coordinator/<blurb>/` when
absent, with the two files `hcdev coordinator` writes: `task.json` as
`{ "done": false, "blocked": null }`, and `tasks.json` with `project`, `blurb`,
`description` (the intent verbatim), `created`, `model: "sonnet"`,
`merge: "mine"`, `goal` and `tasks`. **`goal` is written relative to the
coordinator directory** — `../../planning/<blurb>/goal.md` — because that is
where `hcoord seed` resolves it from. An existing `tasks.json` is not replaced:
report it and stop.

```bash
hcoord goal lint <goal.md> --tasks <coordinator dir>/tasks.json
```

must print `ok`.

## Finish

Print the goal path, the lint line, the screens captured, the task list as a
table (number, task, title, scenarios, depends on) and `tasks.json`'s path. One
closing line: nothing is filed — `/issue-create` writes an entry up when I want
it, and `hcdev coordinator <blurb>` from the project starts the run.

Append one line to `<coordinator dir>/YYYY-MM-DD.md` naming the goal and the
split, in the format `/task:coordinate` describes under *Journal* (front matter
`title: "<blurb> — coordinator"`, `created`, tag `journal`, when the file is
new). Never commit the notes repo.

## Notes on scope

- **One goal per blurb.** A re-run resumes it.
- **No issue is filed and no code is written.**
- **`Current state` is the only section written from the code.** A `(target)`
  screen is drawn and never captured.
- **This skill stops at the list.** Running it is `/task:coordinate`, and
  judging the result is `/task:judge`.
