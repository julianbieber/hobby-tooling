# Aspect

| Field        | Value                                                    |
| ------------ | -------------------------------------------------------- |
| Type         | skill                                                    |
| Install path | `~/.claude-hobby/commands/aspect.md` → `/aspect`         |
| Owner        | @julianbieber                                            |
| Dependencies | nushell `hwt.nu` (`hwt aspect new`), `hcdev.nu` (`hcdev tab`), `haspect`, `/task:aspect` |
| Status       | experimental                                             |

## Description

Run from a task's own Claude session. It turns a research question into a
**research aspect** of that task: a worktree under
`~/fun_aspects/<project>/<topic>/<task>/<name>` on branch `<task>@<name>`, a
notes dir `notes/<project>/<topic>/<task>/aspects/<name>/` with `aspect.json`
and a self-contained `brief.md`, and a tab in the task's zellij session whose
Claude (`haspect` → `/task:aspect`) writes the answer to `findings.md`.

The aspect appears on the task's hdash card under **Aspects** with its state.

## Why an aspect and not a subtask

A subtask is a deliverable with its own card, session, branch and PR, and it
can wait for weeks. A research question ends in a file the task reads, and it
belongs to the task: it has no card, closes with it, and runs in its session
as a tab. The research context — sources, spikes, dead ends — stays in the
tab's conversation instead of this one.

## Refused under `auto: true`

An unattended `/task:auto` run is one session that a coordinator watches
through one `build-task.json`. A tab it cannot see would hold a gate nobody
answers.
