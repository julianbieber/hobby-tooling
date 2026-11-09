# Task Aspect

| Field        | Value                                                          |
| ------------ | -------------------------------------------------------------- |
| Type         | skill                                                          |
| Install path | `~/.claude-hobby/commands/task/aspect.md` → `/task:aspect`     |
| Owner        | @julianbieber                                                  |
| Dependencies | `haspect` (starts it in the tab), `/research`, `/task:build`   |
| Status       | experimental                                                   |

## Description

What every aspect's tab runs, whatever its kind. `haspect` starts it while the
aspect's `aspect.json` says `open`, and resumes the tab's conversation after
that. It reads the brief `/aspect` wrote and the task's recorded context, then
works the aspect by its kind:

- **research** — answers the question with `/research` and spikes in the
  aspect's own worktree, and writes `findings.md` in a fixed shape: answer,
  evidence, consequence for the task, open questions, spikes. State moves
  `open` → `active` → `reported`.
- **code** — runs the full `/task:build` pipeline in the aspect's worktree,
  with every gate. State moves `open` → `active` → `pr` → `done`, the last
  once the PR to the aspect's base has merged.

`hwt aspect close` sets `closed`. hdash shows the state on the task's card.

## How a finished aspect reaches its task

`/task:aspect finish` records `outcome` and `open_items` in `aspect.json` and
messages the task's session to read them. It never writes the task's notes, so
there is one record of an aspect's state and nothing to fall behind it. A respawned
tab runs this skill again while the aspect is unfinished, which is how a PR that
merged while no session watched it gets noticed.

## Why the findings have a fixed shape

The reader is the task's own session, or me deciding from the card. The answer
comes first so it can be read without the rest, and evidence is separated from
the consequence so a verified fact is never mixed with a recommendation.
