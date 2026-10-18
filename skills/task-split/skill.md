---
description: Carve a project or an oversized issue into a sequence of tasks that each satisfy the task-shaping rules, and hand back the list for me to edit before any of it is written up
allowed-tools: Bash(git branch:*), Bash(git log:*), Bash(git remote:*), Bash(pwd:*), Bash(ls:*), Bash(rg:*), Bash(gh pr:*), Read, Grep, Glob
---

Take a body of work and produce the ordered list of tasks it should be split
into, with each one tested against the shaping rules in `CLAUDE.md`.

**Arguments:** $ARGUMENTS — the project, goal, or task with an `issue.md` to carve up. If
empty, ask what is being split.

**This skill writes nothing.** Its output is a proposed list for me to edit;
turning an accepted entry into an issue is `/issue-create`, one at a time,
because the body, the topic and the task name are per-issue decisions.

## The rules this pass applies

The six rules live in `CLAUDE.md` under **Task shaping** and are the standard, not
a summary of one. Read them there. What this skill adds is the procedure for
applying them and the re-cuts to reach for when one fails.

## Instructions

1. **Establish the end state.** What is true for a user when the whole thing is
   done that is not true now. One or two sentences. If that cannot be stated, the
   split cannot be checked against rule 1 and the right output of this skill is
   that sentence being missing — say so and stop.

2. **Search prior art** before proposing anything, as `CLAUDE.md` requires:

   ```bash
   rg -il "<terms>" ~/fun_repos/hobby-mimisbrunnr/notes/<project>/*/*/issue.md
   gh pr list --search "<terms>" --state all
   ```

   Report overlaps. Do not propose a task that duplicates open work or races
   someone else's.

3. **Draft the slices as behaviours, not as components.** Write the list as the
   order in which a user gains capability, before looking at the code at all.
   Starting from the architecture is what produces layer tasks, and rule 1 exists
   to reject exactly those.

4. **Test every candidate against all six rules** and record which ones it fails.
   State the failure, do not silently fix it — a candidate that had to be re-cut
   is information about the shape of the work.

5. **Re-cut what failed.** The four moves that fix nearly everything:

   - **Layer task -> vertical slice.** "Add the table / endpoint / UI" becomes one
     narrow thing a user can do end to end.
   - **Too large -> narrow the user, not the stack.** One entity kind, one input
     format, one code path, happy path only. Slicing by *case* keeps the task
     end-to-end; slicing by *layer* does not.
   - **Question inside the work -> decision task in front of it** (rule 5), whose
     output is the decision and the tasks that follow from it.
   - **Enabler with no consumer -> either fold it forward or drop it.** An
     enabling task is legal only with a named consumer that exists (rule 1's
     escape). One with no consumer is speculative work, and this is where it gets
     caught.

6. **Check the sequence, not just the items.** Four properties the list as a whole
   must have:

   - **Every prefix is shippable.** Stopping after task *n*, for any *n*, leaves a
     coherent system. This is what rule 1 buys and the only place it is verified.
   - **Revert independence** (rule 3) holds pairwise for anything that touches
     shared code, not just for adjacent entries.
   - **Every `enabling` task's named consumer is in the list**, and later in it.
   - **The last task leaves nothing to remember** (rule 6) — no flag, no dual code
     path, no migration half-done. If it does, the cleanup is a task and it goes
     on the end.

7. **Present the list for me to edit.** The layout below. Include the failed
   candidates and the re-cuts — I want to see what was rejected and why, not only
   what survived.

8. **Suggest the follow-up**, once I accept the list: `/issue-create` per
   entry, in order. Mention that each issue needs its own topic and that the
   dependency lines below should become task references (`<topic>/<task>`) in the
   bodies.

## Output layout

```markdown
## End state
<one or two sentences: what is true for a user afterwards>

## Tasks

### 1. <imperative title, <=72 chars>
- **Behaviour:** <what a user can do that they could not before>
- **Done when:** <the concrete observation — do X, see Y>
- **Depends on:** <task numbers, or none>
- **Revert:** <what backing this out alone would leave; "clean" if nothing>

### 2. <title>   [enabling -> 4]
- **Enables:** task 4 — <the behaviour it unlocks>
- **Done when:** <observation; for an enabler this is a test or a command, not a
  user action>
- **Depends on:** ...
- **Revert:** ...

## Decisions to make first
- <question> -> its own item, blocking tasks <n, m>

## Rejected cuts
- <candidate> — fails rule <n>: <why> -> re-cut as tasks <n, m>

## Sequence checks
- Prefix shippable: <pass, or the first n that is not and why>
- Revert independence: <pass, or the entangled pair>
- Enablers consumed: <pass, or the orphan>
- Nothing left to remember: <pass, or the cleanup task added>

## Prior art
- #<n> <title> — <overlap, or "adjacent, no conflict">
```
