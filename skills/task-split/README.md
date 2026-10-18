# task-split

| Field        | Value                                          |
| ------------ | ---------------------------------------------- |
| Type         | skill                                          |
| Install path | `~/.claude-hobby/commands/task-split.md`       |
| Owner        | @julianbieber                                  |
| Dependencies | `gh` CLI authenticated (prior-art search only) |
| Status       | experimental                                   |

## Description

Carves a project, or an issue that turned out to be too big, into an ordered list
of tasks that each satisfy the **Task shaping** rules in `CLAUDE.md`. It writes
nothing — the output is a list to edit, and an accepted entry becomes an issue
through `/issue-create`.

## Where the rules live, and why not here

`dotfiles/claude-md/hobby.CLAUDE.md`, section **Task shaping**. That file is
symlinked to `~/.claude-hobby/CLAUDE.md` and is loaded in every hobby session in
every repo, so the rules apply to work that never invokes this skill — a task
proposed mid-conversation, a `/subtask` split, an issue written by hand. A skill
is opt-in and would have covered only the deliberate case.

The division is: `CLAUDE.md` holds the rules, this skill holds the *procedure* for
applying them, and the README holds the argument for why each rule is shaped the
way it is. Duplicating the rules into the skill would create two copies that drift.

## Why these six rules

**Rule 1 (verifiable user-visible impact)** is the anchor, and its stated
justification — traceability to value — is the weak one. What it actually does is
forbid the layer task. "Add the table", "add the endpoint", "wire the UI" each
fail it in isolation, so the rule forces vertical slicing, and vertical slicing is
what makes any prefix of the list shippable and gives review something concrete to
check.

Vertical slicing is also the rule's cost, not a free bonus: the tickets are harder
to write, and enabling work has to go somewhere. The `enabling` escape exists
because the two alternatives are worse. Bundling puts the migration inside the
first feature that needs it, tripling that task and destroying the review's ability
to separate mechanical risk from semantic risk. Fiction — "user can now log in
faster" on a migration — puts an unchecked claim in the one field the whole rule
exists to make checkable, which is worse than having no field.

**Rule 2 (the observation, written first)** carries more weight than it looks like.
It is what makes rule 1 enforceable at all: without a stated observation, "impact
on user behaviour" is a genre of sentence, and every task passes.

**Rule 3 (independent revert)** is the cheapest structural test available, and the
only one checkable at merge time rather than at planning time. It catches splits
that read as vertical and are entangled in practice.

**Rule 4 (review-bounded size)** replaces estimation. "Can one person hold this
diff in their head" produces agreement where effort estimates produce argument,
and it handles mechanical churn correctly — a 40-file rename is large and
reviewable, and any effort-based measure gets that case wrong.

**Rule 5 (no unanswered questions)** separates discovery from execution. A task
carrying an open decision has no meaningful done-condition, which is why those are
the ones that sit open for weeks.

**Rule 6 (no remembered state)** is rule 1's counterweight, and the one most
likely to be argued with. A vertical slice frequently ships behind a flag, so
rule 1 actively generates the debt this rule forbids leaving implicit. The rule
does not ban the flag — it bans the flag existing without a task to remove it.

## What was deliberately left out

**Estimates, points, any size field.** They constrain nothing about shape, rules
3-5 bound size structurally by checkable properties, and a number is the field
people optimise against rather than satisfy.

## Related

- `/issue-create` — turns one accepted entry into an issue, and runs the shape
  check before writing it
- `/subtask`, `/spawn` — splitting work already in progress; bound by the same
  rules
