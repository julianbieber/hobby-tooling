# Aspect

| Field        | Value                                                    |
| ------------ | -------------------------------------------------------- |
| Type         | skill                                                    |
| Install path | `~/.claude-hobby/commands/aspect.md` → `/aspect`         |
| Owner        | @julianbieber                                            |
| Dependencies | nushell `hwt.nu` (`hwt aspect new`, `hwt aspect start`), `hcdev.nu` (`hcdev tab`), `haspect`, `/task:aspect` |
| Status       | experimental                                             |

## Description

Run from a task's own Claude session, or from one of its aspects' tabs. It
adds an **aspect** to that task: a notes dir
`notes/<project>/<topic>/<task>/aspects/<name>/` with `aspect.json` and a
self-contained `brief.md`. Started — now, or later in the task — it gets a
worktree under `~/fun_aspects/<project>/<topic>/<task>/<name>` on branch
`<task>@<name>`, cut from the current tip of its base, and a tab in the task's
zellij session whose Claude (`haspect` → `/task:aspect`) works it.

An aspect is `research` (done when `findings.md` answers it) or `code` (done
when its PR to its base has merged and its done-when holds). Both kinds start
the same way.

The aspect appears on the task's hdash card under **Aspects** with its kind
and state; a click on one not yet started starts it.

## Why an aspect and not a task

The split is about when, not what. A task is work planned for some later
point, with a card of its own. An aspect is part of this task — run now, or
added so it is not forgotten and run before the task is finished, which is
why `hwt done` refuses a task with an unfinished aspect.

## Started from the current base

An aspect may be added long before it is started, so its worktree is not cut
when it is added: starting it fetches and cuts from the base's current tip, or
rebases the branch onto it when it already exists. The PR goes to the same
base.
