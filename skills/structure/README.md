# structure

| Field        | Value                                              |
| ------------ | -------------------------------------------------- |
| Type         | skill                                              |
| Install path | `~/.claude-hobby/commands/structure.md`            |
| Owner        | @julianbieber                                      |
| Dependencies | `hobby-struct-parser` (`just install-struct`)      |
| Status       | experimental                                       |

## Description

Checks a repo against the code quality standards its `structure.hstruct` states,
explains what a count is made of, attempts the fix when the code has moved away
from a standard, and ratchets a budget the code has moved below.

Also the entry point for a repo that has none: `--draft` works out the standards
that project actually holds, rather than generating them from the metric list.

## Why the constraints are in the skill and not just the checker

Three things the checker cannot enforce on its own, and each is a way the format
decays:

- **Never write an instance into the file.** `--detail` exists so offenders can be
  read; storing them is what made the discarded first design churn on every edit.
- **Never raise a budget.** `--ratchet` cannot, but a skill with an `Edit` tool
  can, and a machine widening a standard so its own change fits is the failure
  this format exists to prevent. Loosening is a hand edit, visible in the diff.
- **Never add a standard during a task.** A task is short; a standard outlives the
  project's current shape. Deciding one is a conversation.

## Relationship to `/task:build`

`task:build` runs the check as its `standards` phase, between `comments` and
`review`. This skill is the same work invoked by hand — on a repo you are not
running a task against, or when a number is surprising and you want it explained.
