# Task Aspect

| Field        | Value                                                          |
| ------------ | -------------------------------------------------------------- |
| Type         | skill                                                          |
| Install path | `~/.claude-hobby/commands/task/aspect.md` → `/task:aspect`     |
| Owner        | @julianbieber                                                  |
| Dependencies | `haspect` (starts it in the tab), `/research`                  |
| Status       | experimental                                                   |

## Description

What a research aspect's tab runs. `haspect` starts it while the aspect's
`aspect.json` says `open`, and resumes the tab's conversation after that. It
reads the brief `/aspect` wrote, answers the question with `/research` and
spikes in the aspect's own worktree, and writes `findings.md` in a fixed
shape: answer, evidence, consequence for the task, open questions, spikes.

`aspect.json` `state` moves `open` → `active` → `reported`; `hwt aspect close`
sets `closed`. hdash shows the state on the task's card.

## Why the findings have a fixed shape

The reader is the task's own session, or me deciding from the card. The answer
comes first so it can be read without the rest, and evidence is separated from
the consequence so a verified fact is never mixed with a recommendation.
