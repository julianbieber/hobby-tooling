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
  "merge": "mine | coordinator",
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

Only `task` and `title` are required; `status` defaults to `todo` and `merge` to
`mine`.

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
  conflict, so `gh pr merge` would go through now. A coordinator that was handed
  the merges passes the flag; without it a green PR is waited on, because merging
  is mine by default.
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

`hdash` is a Bevy build; this is a few hundred lines over ratatui. Sharing a
workspace would put the Bevy dependency graph and its optimised-dependency
profile in front of a tool that builds in ten seconds on its own. `hdash` reads
the `status` field of `tasks.json` with `serde_json::Value` rather than
depending on this crate, for the same reason — it reads `merge` the same way.
