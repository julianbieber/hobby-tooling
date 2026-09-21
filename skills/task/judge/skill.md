---
description: Judge a finished coordinator run scenario by scenario against its goal — the app running on the merged default branch, each scenario's sketch and captured screen beside it — and record my verdict verbatim in judgement.md, a wrong as a follow-up line and a tune as a commit opened as a PR
argument-hint: '[--blurb <name>]'
allowed-tools: Skill, Read, Write, Edit, Bash
---

The end of a coordinator run, in one sitting, in the coordinator session: I look
at what was built against the goal it was built from and say, scenario by
scenario, whether it is what I pictured. This skill owns no task and reads no
task's code. What it reads is the goal, the screens the run captured, and the
running application.

The order, the screen lookup and the file's format are `hcoord judge`. What is
left for this skill is asking me and turning my words into a follow-up line or a
changed default.

## Rules that override everything else

1. **My words are recorded verbatim and never paraphrased.** They go to
   `hcoord judge record` through `--words -` on standard input, not through a
   quoted argument, and nothing is summarised on the way.
2. **Nothing is invented.** A follow-up line and a tuning commit come from my
   words and from a fact verified in the code, never from what this pass supposes
   I meant. A justification I did not give is asked for, not supplied.
3. **One scenario per turn.** Print it, ask, end the turn.
4. **The notes repo is never committed.**
5. **No machine attribution**, anywhere: no `Co-Authored-By`, no
   `Generated with Claude Code`, no 🤖.

## Step 0 — Which coordinator

`pwd` must be `~/fun_repos/<project>`; anywhere else, say so and stop.

The coordinator's notes dir is `$HCOORD_NOTES_DIR` when `hcdev coordinator` set
it; else `--blurb <name>` names it; else the single open coordinator under
`~/fun_repos/hobby-mimisbrunnr/notes/<project>/coordinator/`. Several and no
flag: ask which.

```bash
hcoord judge list <notes dir>
```

Print the output verbatim. A `tasks.json` that names no goal is a stop: there is
nothing to judge. When the `unfinished:` line names tasks, say so in one line,
ask whether to judge anyway, and end the turn — judging half of a run is my call,
not this pass's.

## Step 1 — The app, on the merged default branch

```bash
gh repo view --json defaultBranchRef -q .defaultBranchRef.name
git status --short && git rev-parse --abbrev-ref HEAD
git pull --ff-only
```

A dirty tree, a branch other than the default, or a refused pull is printed and
asked about before going on: judging a tree that is not what merged is judging the
wrong thing.

Then launch the application the way `/task:auto`'s play gate does, detached, with
its output in the coordinator's notes dir:

```bash
setsid -f sh -c '<run command> > <notes dir>/judge.log 2>&1'
```

The run command is found in the project's `justfile` or `README.md` and **printed
before it is run**. Wait a few seconds and `head` the log; a build error is
reported to me, not repaired here. When the goal's screens are all files or
printed lines, launch nothing and say so in one line.

## Step 2 — The walk

For each `S` line of `list`, in the order printed, skipping any whose `judged=` is
already a verdict unless I asked for it again:

```bash
hcoord judge show <notes dir> <id>
```

Print it **verbatim**. Then give the capture as a path I can open, with the one
line `show` ended on when the file is missing. Then ask:

> `ok`, `wrong: <what you saw instead>`, or `tune: <the default you want>`.
> `skip` leaves it unjudged.

End the turn.

## Step 3 — The answer

Record it the moment it arrives, before anything else, so an interrupted pass
loses nothing:

```bash
printf '%s' '<my words, verbatim>' | hcoord judge record <notes dir> <id> wrong --words -
```

`ok` carries no words. `wrong` and `tune` carry everything after the colon.
A refusal from `hcoord` — for words that are missing, say — is reported to me and
the same scenario is asked again.

A `wrong` also gets **one** proposed follow-up line, written from my words and
subject to the task-shaping rules in `~/.claude-hobby/CLAUDE.md`: it names an
observation someone can make, not a layer. It goes in with `--follow-up '<line>'`
on the same `record` call. `skip` records nothing. Then the next scenario.

## Step 4 — The tunes

Once every scenario is judged, stop the application (`pkill -f` on the command
launched). Nothing to tune means straight to step 5.

For each `tune`, change **that default in the project's code**. Never a file the
running application owns: the goal's decision `tune` settles that. A default that
cannot be found by reading the code is reported to me, not guessed at.

All of them go on one branch, `judgement-<blurb>-tunes`, cut from the default
branch, one commit per tune. The subject names the default and the change; the
body quotes my words as the justification. Build and test as `/task:auto` rule 4
requires, truncated. Then one PR for the branch, in the shape of
`~/.claude-hobby/CLAUDE.md` — the three sections and nothing else, no `Closes`
line, because a tune closes no issue.

## Step 5 — Finish

Print `judgement.md`'s path, its verdict tally, the follow-up lines and the tune
PR. Append one line to `<notes dir>/YYYY-MM-DD.md`, in the format
`/task:coordinate` uses, naming what was judged and what came of it.

## Notes on scope

- **Re-running resumes.** `judged=` in `list` says what is already recorded, and a
  scenario already judged is skipped unless I ask for it again.
- **No issue is filed.** The follow-up lines are the output, and
  `/gh-issue-create` files them when I want them filed.
- **The goal is never edited.** A goal that was wrong is itself a `wrong`.
