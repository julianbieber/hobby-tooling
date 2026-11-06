# Subtask

| Field        | Value                                                    |
| ------------ | -------------------------------------------------------- |
| Type         | skill                                                    |
| Install path | `~/.claude-hobby/commands/subtask.md`                    |
| Owner        | @julianbieber                                            |
| Dependencies | nushell `hwt.nu` (`hwt subtask`), `hcdev.nu` (`hcdev tab`); `/issue-create` (optional) |
| Status       | experimental                                             |

## Description

The one way to split work out of a task. A subtask is a task of its own —
worktree, branch, `task.json`, PR — cut from the **default branch** and recorded
in the parent's `subtasks.json`, which is what lists it on the parent's hdash
card. It replaces `/spawn`, which recorded ownership but wrote no issue, while
this skill wrote an issue but recorded no ownership.

Two decisions per subtask:

- **Now or postponed.** Worked now, it opens as a tab of the parent's zellij
  session (`tab: true`); postponed, it waits on its own card — the shape for
  work put off for a longer period.
- **Which commits it takes.** `hwt subtask --take` moves whole commits of the
  parent's branch: cherry-picked onto the subtask, then the parent rebased onto
  the subtask's branch. The subtask's PR then holds exactly those commits, and
  once it merges the parent's rebase onto the default branch drops them. This
  is what makes a split possible after work has started.

Files the parent keeps editing may overlap the subtask's; conflicts are
resolved at PR time.

## Usage

```
/subtask extract the spatial hash into its own module
```
