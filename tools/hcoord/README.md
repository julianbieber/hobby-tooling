# hcoord

| Field        | Value                                                        |
| ------------ | ------------------------------------------------------------ |
| Type         | terminal app (ratatui) + CLI                                 |
| Binary       | `hcoord`                                                     |
| Owner        | @julianbieber                                                |
| Dependencies | `gh` on PATH for `merged` and `ci`; `hcdev coordinator` and `/task:coordinate` call it |
| Status       | experimental                                                 |

## Description

What a coordinator session needs from outside its Claude pane.

```
hcoord tui <coordinator notes dir>                         the left pane of `hcdev coordinator`
hcoord wait <task notes dir> [--timeout <s>] [--after <phase>]
                                                           block until a task needs a person or is done
hcoord merged <pr url> [--timeout <s>] [--ready]           block until its PR has landed
hcoord ci <pr url> [--spent <n>] [--bound <n>] [--timeout <s>]
                                                           classify a PR's failing checks
hcoord status <coordinator notes dir>                      counts, then each task's phase and age
hcoord goal lint <goal.md> [--tasks <tasks.json>]          check a goal file, and its split
hcoord goal show <goal.md> <id>...                         print scenarios verbatim
hcoord seed <coordinator notes dir> <task> [--force]       write a task's build-task.json and
                                                           business-context.md from its goal
hcoord capture <task notes dir> <scenario> --window <name> | --text <file>
                                                           write screens/<scenario>.png for a hand
                                                           scenario: an X11 window, or text rendered
```

## `tui` shows `tasks.json` and nothing else

The file lives in `~/fun_repos/hobby-mimisbrunnr/notes/<project>/coordinator/<blurb>/`
and is written by `/task:coordinate`. The pane re-reads it whenever its mtime
changes, so the list updates as the coordinator works without either side
talking to the other. It is designed for a 15% split: one line per task — status
glyph, number, title — with the coordinator's note, the task's own acceptance
result and its PR number indented beneath, and the counts at the top. The
acceptance line is read out of that task's `build-task.json`, not out of
`tasks.json`, and appears only for an unattended run that got as far as running
its checks. `d` toggles the description, `j`/`k`
scroll, `r` forces a reload, `q` quits.

```json
{
  "project": "wusel",
  "blurb": "food-economy",
  "description": "verbatim, what the coordinator was started with",
  "created": "2026-09-11",
  "tasks": [
    {
      "task": "food-economy-fields",
      "topic": "food-economy",
      "title": "Fields grow wheat over the seasons",
      "done_when": "start a game, place a field, watch it fill by autumn",
      "depends_on": [],
      "status": "todo | running | awaiting-human | awaiting-merge | done | blocked",
      "pr": "https://github.com/…/pull/12",
      "note": "one line, optional",
      "updated": "2026-09-11T10:00:00Z"
    }
  ]
}
```

Only `task` and `title` are required; `status` defaults to `todo`. A `merge` key
from an older list is ignored: every task merges its own PR, and `seed` writes
`merge: coordinator` into each task's state file.

## `wait` is how the coordinator spends its time

A coordinator is a Claude session, and a Claude session polling a file from a
prompt is a prompt per poll. `hcoord wait` turns that into one shell call: it
watches the task's `build-task.json` — the state file `/task:auto` writes at every
transition — and returns when the phase is one a person has to answer
(`await-*`, `context`, `assess-split`) or terminal (`done`, `handed-off`). The
default timeout is 570 seconds, under the ten minutes Claude Code allows a
shell call, and a timeout is exit 2 so the coordinator simply calls again.

A state file that cannot be read or parsed is retried on the same poll as one that
does not exist yet, because the ordinary cause of both is the task session part-way
through writing it — a zero-byte file is enough. Exit 1 therefore means the file stayed
unreadable for the whole timeout, which is the only reading under which a coordinator
blocking its list is right.

The `await-*` prefix is what keeps the two sides independent: `/task:auto` names
its gates and `wait` matches the shape, so `await-fix` — an unattended run whose
acceptance would not pass — needed nothing here to be seen.

`--after <phase>` is for the second call on the same gate: having reported
`await-play` once, the coordinator wants to block until the phase is something
*else*, not be told `await-play` again immediately.

`wait` also reports `human` on a `play` phase that has sat untouched for three
minutes, even without an `await-*` name. The reason is recorded in
`business-context.md` under `## play-gate-fix`: the write to `await-play` and
the printed gate report are two halves of one step in the skill, and the
`hcoord` half is the one that cannot be forgotten by a session that drops the
other. Three minutes is measured against the skill's own phase `play` — a
detached launch, a few seconds' wait and a `head` of the log — so it is well
past normal work and well inside one 570-second wait. A file recording neither
`written` nor `phase_since` counts as idle too, since it cannot be told apart
from one that has gone stale.

The output is one JSON line, `{"phase": "await-play", "outcome": "human"}`, so
the coordinator reads a field rather than parsing prose.

An unattended run records what its own acceptance checks did, and the line then
carries that too:

```json
{"acceptance": {"checks": 6, "failed": [{"command": "…", "exit": 1,
 "name": "switch documents"}], "passed": false},
 "outcome": "human", "phase": "await-fix"}
```

At `await-fix` the line also says what the session is blocked on. `decision` is one
line naming the first of three that applies — a CI block, then a merge block, then
the failing acceptance checks — because a later stage's block is the one still
standing; `ci` carries the recorded `{outcome, case, check, reason}` and `pr` the PR
when one was opened:

```json
{"ci": {"case": "not-mine", "check": "build", "outcome": "blocked"},
 "decision": "CI not-mine: build", "outcome": "human", "phase": "await-fix",
 "pr": "https://github.com/…/pull/7"}
```

Before `ci` existed a CI block reached the line with nothing but an empty
`acceptance`, so a coordinator reported it as a failed acceptance and lost the PR.

A state file seeded `auto: true` waits on `await-*` alone. Without it, `context` and
`assess-split` — the two waiting states `/task:build` writes — also count as `human`.

A scenario the goal marks `hand` is not in `failed` and is not counted as a pass:
the object then carries `hand`, the number of checks nobody ran, and the tui tally
reads `3 pass · 1 hand`. `hand` is absent when there were none, so a run without
one prints the line it always did.

That object is the point of the whole field. A coordinator cannot read the task's
own pane — the window is off-screen and about seventeen columns wide — so the
evidence that a change does what it promised has to arrive as data or not at
all.

A state file that records `phase_since` (when the phase last changed) also adds
it to the line, along with `unchanged_for` — the age in seconds, computed at
print time so the coordinator does not have to do its own date arithmetic — and
`written` (when the file was last written) when that was recorded too:

```json
{"outcome": "human", "phase": "implement", "phase_since": "2026-09-16T15:18:47Z",
 "unchanged_for": 4320, "written": "2026-09-16T16:26:47Z"}
```

`written` and `phase_since` differ here because the pipeline wrote the file again
without moving the phase — the one distinction an mtime alone cannot make. A
state file from before these fields existed carries none of the three, rather
than falling back to the file's own mtime.

## `status` names how long a phase has sat still

`hcoord status <coordinator notes dir>` prints the counts line `hcoord tui`
draws at the top, then one line per task that is not yet `done` (or
`handed-off`) and has a readable state file:

```
1 running · 1 waiting for you
  1. food-fields — implement, unchanged for 1h 12m, last write 4m ago
  2. food-market — await-play, unchanged for 3m
```

A task with no state file yet, or one already terminal, prints no line — there
is nothing to watch there. A file with neither timestamp reads `no time
recorded` rather than a guess.

## `merged` is the same trick, one layer on

A task is not done when its session ends — it is done when its PR is merged, so
the coordinator has a second thing to wait on and the same reason not to wait on
it from a prompt. `hcoord merged` polls `gh pr view` every fifteen seconds and
prints one JSON line, `{"pr": …, "outcome": "merged", "state": "MERGED",
"checks": "green"}`, with the same timeout and the same exit codes as `wait`.

Four outcomes, and the split between them is the whole design:

- **`merged`** — the PR landed. The task is done.
- **`blocked`** — a failing check, a conflict with the base, or a PR closed
  unmerged, with `reason` saying which. None of these resolve by waiting longer,
  so they are reported rather than waited out.
- **`mergeable`** — only with `--ready`: the checks pass and GitHub reports no
  conflict, so `gh pr merge` would go through now. A task session passes the flag
  before merging its own PR; the coordinator never does, because it never merges.
- **`timeout`** — nothing resolved. Call again.

**A repo with no checks counts as green.** `statusCheckRollup` comes back empty,
which is reported as `checks: none` and treated as passing. Most hobby repos run
no CI at all, and the alternative is a list that never moves.

Both shapes `statusCheckRollup` returns are read: a check run carries
`conclusion` once it has finished and `status` until then, a commit status
carries `state`. Reading only one of them would call a red build green.

## `ci` is the fix loop's eyes

A red check on a task's own PR is almost always that task's own diff, and the task's
own session has everything a coordinator does not: the branch, the plan, and the
failing log. `hcoord ci <pr url>` polls the same rollup `merged` does, and once the
checks resolve prints one JSON line and exits — `{pr, outcome, spent, bound}`, plus
`checks` when green and `case`, `reason`, `check`, `run`, `log` when the checks are
failing:

- **`green`** — every check passed, or the repo runs none. Same rule as `merged`: a
  repo with no CI must not deadlock the loop, this one included.
- **`fix`**, case `own-diff` — the failure is this diff's own. `/task:auto`'s
  `fix-ci` phase reads `log` and repairs it.
- **`rerun`**, case `flake` — the log or the verdict looks like a dead runner or a
  network blip, not a code problem. `fix-ci` reruns the failed jobs and asks again.
- **`blocked`**, case `not-mine` — the same check is already failing on the base
  branch's tip. Repairing it here would be widening this diff to fix another task's
  work.
- **`blocked`**, case `decision` — the log matches a pinned-revision failure (a
  `cargo` dependency resolved against a rev that no longer exists). Re-pinning is a
  project decision, not a mechanical repair.
- **`timeout`** — the checks never resolved. Call again.

**The bound lives here, not in the prompt.** `--spent` is the number of `fix-ci`
rounds already spent (`ci_result.rounds`), `--bound` defaults to 3
(`business-context.md` `## fix-loop-bound`, matching the acceptance-repair cap
`/task:auto`'s `verify` phase already uses). `fix` and `rerun` turn into `blocked`
once `spent` reaches `bound`; `not-mine` and `decision` ignore the count entirely,
because neither resolves by trying again — counting attempts against them would only
delay the report.

**Classification order matters, and a fact goes first.** A check already red on the
base branch is not this diff's doing whatever its log says, so that comparison runs
before any log is even read. Infrastructure comes next and outranks a pinned-revision
match, because a cancelled run's log describes the runner dying, not the code.

**The log evidence is read from the failing check's own job, not pasted from the
caller.** `detailsUrl`/`targetUrl` on the check gives a GitHub Actions run and job id;
`gh run view --job <job> --log`, kept to its last 20 signature-bearing lines (or its
last 20 lines outright, when nothing looks like a failure) and capped at 2000
characters. A check with no job behind its URL contributes an empty log rather than
failing the call.

## Not part of the hdash workspace

`hcoord` is its own workspace, and `hdash` reads the `status` field of
`tasks.json` with `serde_json::Value` rather than depending on this crate.

## `goal` reads the UX spec a run is built and accepted against

A goal file (`skills/task/goal/README.md` has the format) is what a coordinator's
tasks are split from: each task's `scenarios` names the ids it owns, and the
coordinator's `goal` names the file. `lint` checks the file on its own — every
scenario on a drawn screen, one observation each, every widget it names in the
table, no `?` left — and with `--tasks` that every scenario has exactly one
owner and every task at least one scenario. The one task allowed none is an
`enabling` one: its `note` says `enabling -> <task>`, and that task is in the list
with it in `depends_on`, so it is done before the task it enables starts. A
`### <name> (exists|target)` under `## Project on disk` is a screen like one under
`## Screens`, so a scenario can name a directory tree. `show` prints scenarios verbatim so a
brief quotes them rather than paraphrasing. Both are plain text with `file:line`
findings and exit 1 when there is one; nothing here is JSON because a person
reads it.

## `seed` writes a task's brief from the goal

`hcoord seed <coordinator notes dir> <task>` is what `/task:coordinate` W3 calls
when `tasks.json` names a `goal`, instead of assembling the state file in a
prompt. It writes the task's `build-task.json` — `acceptance` being the task's
scenarios exactly as `goal show` prints them, `goal` and `scenarios` recording
where they came from, and a `brief` that quotes the coordinator's description, the
goal's intent, the screens the scenarios name (with the capture's path when
`screens/<name>.png` exists beside the goal), and what depends on the task. It
also writes `business-context.md` with the goal's decision blocks that name the
task or one of its scenarios as a whole word; when none does, no such file is
written.

An `enabling` task is seeded with no scenarios and an `acceptance` saying it has no
checks — its PR's CI going green is the acceptance — and its brief quotes the
scenarios of the task it enables under *Scenarios it unlocks*, with their screens
and the decisions naming them, as what it is for.

It refuses, writing nothing, a goal that fails `lint` (an open `?` included, and a
scenario-less task that is not a valid `enabling` one), a list with no `goal`, and a task whose notes already
hold a `build-task.json` — `--force` overwrites both files. It makes no network
call.

## A hand scenario is captured, not checked

`hcoord capture <task notes dir> <scenario> --window <name> | --text <file>` writes
`screens/<scenario>.png` into the task's notes directory and prints that relative
path. `verify` calls it for each scenario a goal marks `hand`, and records the
path on the check as `screen`.

The artefact is always a png. The two sources are there because a goal's screens are of two kinds: a window of
the running application, or a file or a command's output. A png of rendered text is
still the picture. `--text -` reads standard input, so `<cmd> | hcoord capture …
--text -` captures a command's output without a temporary file.

The window source captures only a window the X server has mapped. One parked on an
off-screen tag fails rather than writing a blank image, and a name matching more than
one visible window is refused rather than guessed at, because a capture of the wrong
window is wrong silently. A failed capture writes nothing and exits 1; `verify`
records the failure as the check's `note` and does not fail the run, since the
scenario was one nobody could run in the first place.
