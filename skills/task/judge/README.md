# Judge

| Field        | Value                                                              |
| ------------ | ------------------------------------------------------------------ |
| Type         | skill                                                              |
| Install path | `~/.claude-hobby/commands/task/judge.md` → `/task:judge`           |
| Owner        | @julianbieber                                                      |
| Dependencies | `hcoord`, `gh` and `git` on PATH; a coordinator whose `tasks.json` names a goal |
| Status       | experimental                                                       |

## Description

Run in the coordinator session after every task is done. It walks the goal's
scenarios — the `hand` ones first — with the application running on the merged
default branch, shows each one's text, its screen's sketch and the screen `verify`
captured, and takes `ok`, `wrong: <words>` or `tune: <words>`. Every answer lands
verbatim in `judgement.md` in the coordinator's notes. A `wrong` also gets one
proposed follow-up line; a `tune` becomes a commit changing that default, on one
branch, opened as a PR.

## Why it exists

The goal's `## Current state` records what the runs before it left: 48 merged
PRs, no review comments and no follow-up issues. "Review the UX now that the run
is finished" was a good intention with nothing at the end of a run that made it
happen. A pass that is a command, with a file at the end of it, is the thing that
makes it happen — and it is the one place I judge a run, at the end, in one
sitting, with the words to say what is wrong.

## Why the file's shape is `hcoord` and the words are the skill

The same cut `hcoord seed` makes. The order scenarios are judged in, the lookup of
a screen and the format of the file are re-derived wrongly when they live in a
prompt, so they are `hcoord judge`, with tests. My words, and the judgement of
what they mean for a follow-up or a default, cannot live anywhere but a
conversation.

## Why hand scenarios first

They are the scenarios that exist because nobody could script them, so no check
has looked at them; and they are the reason the application is running while I
judge.

## Why a `wrong` stops at a line and a `tune` goes all the way to a PR

A tune is a number with an obvious home in the code. A wrong is a disagreement
about behaviour, and its next step is a task — a decision I make from the line,
not one this pass makes for me.

## What it deliberately does not do

- **Write issues.** The follow-up lines are the output; `/issue-create` writes
  them up when I choose.
- **Edit the goal.** A goal that was wrong is itself a `wrong`.
- **Write into the running application's own files.** The goal's decision `tune`
  says a tune is a change to the project's code.
- **Judge a task's diff.** It judges the scenarios, which is what the goal promised.
