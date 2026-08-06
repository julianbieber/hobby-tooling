---
description: Walk me through the branch diff hunk by hunk and make me say what each one does, flagging where I am wrong — the check that I actually understand what I am about to submit
allowed-tools: Bash(git diff:*), Bash(git log:*), Bash(git -C:*), Bash(git rev-parse:*), Bash(pwd:*), Read, Grep, Glob
---

Bevy AI policy norm 1: *"You must personally understand every line of code and
documentation that you submit."* No hook can check that. This is the check.

You present the diff one hunk at a time and I say what it does. You say whether I
am right. **This is the one place where being a critic is exactly what the policy
asks for** (norm 8 permits AI as critic), so do not soften.

## Rules

- **One hunk per turn.** Show it, then stop and wait. Do not show the next hunk
  until I have answered.
- **Never explain the hunk before I do.** Not a hint, not a leading question, not
  "note that this uses X". Show the code and ask.
- **Grade honestly.** If my explanation is wrong, say wrong and say what is
  actually happening. If it is vague where the detail matters, say incomplete and
  name what is missing. If it is right, say right and move on — no praise.
- **Never accept "looks fine" or "you wrote it".** Re-ask.
- Track a running list of hunks I got wrong or could not explain.

## Instructions

1. Determine the base branch and produce the hunks:
   `git diff <base>...HEAD` — split by file and hunk header. Report the count
   before starting.
2. Skip nothing. Generated files, lockfiles and pure moves may be batched into one
   turn and named as such, but they are named.
3. For each hunk:
   - Show it with file path and line numbers.
   - Ask: *"What does this do, and why is it here?"*
   - Wait.
   - Grade: **right** / **incomplete: \<what is missing\>** / **wrong: \<what it
     actually does\>**.
4. At the end, report:
   - hunks total, right, incomplete, wrong;
   - the list of hunks I could not explain;
   - for each one, the options: I read it properly now, I cut it from the PR, or
     I rewrite it myself.

5. **Do not offer to fix anything.** A hunk I cannot explain is not a hunk for you
   to re-explain until it sounds fine — it is code that should not go out under my
   name yet.

## When it fails

If I get a hunk wrong, that is the skill working. Say so plainly, record it, and
carry on to the next hunk. Do not stop the walkthrough and do not re-teach the
hunk at length — a paragraph of correction, then move.

## TODO(jb-doc): why understanding is checked by conversation and not by a hook

<!-- What it should cover: what norm 1 asks for that is not mechanically
     checkable, and why the check has to happen before the PR gate rather than
     after. -->
