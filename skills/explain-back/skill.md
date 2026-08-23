---
description: Walk me through the branch diff hunk by hunk and make me say what each one does, flagging where I am wrong — the check that I actually understand what I am about to submit — then check every comment in the diff against the code and correct it
allowed-tools: Bash(git diff:*), Bash(git log:*), Bash(git -C:*), Bash(git rev-parse:*), Bash(pwd:*), Bash(*/ai-policy-tier.sh:*), Read, Grep, Glob, Edit
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

## The comment pass

Runs **only after** the tally in step 4 is printed. Not before, not interleaved
with the quiz: rule 5 above says a hunk I cannot explain is not yours to
re-explain, and that rule is about *code*. This pass is about *comments*, and it
is the one thing in this skill you are allowed to change.

**You may edit comment lines. You may not edit anything else.** If a comment is
wrong because the code is wrong, say so and leave both alone — that is a finding
for me, not an edit for you.

1. **Resolve the tier** with `scripts/ai-policy-tier.sh` from the hobby-tooling
   checkout, and say in one line which tier this ran under.
2. **Collect every comment** in `git diff <base>...HEAD` — added and modified
   lines both, plus any comment attached to an item the diff touched even when the
   comment line itself is unchanged. A doc comment that now describes the old
   behaviour is the whole reason this pass exists.
3. **For each comment, decide:**
   - **Drifted** — it describes something the code no longer does.
   - **Missing** — a public item in the diff has no doc comment.
   - **Misplaced** — it sits on a private item or inside a body.
   - **Fine** — accurate, on a public item, and says something the code does not.
4. **Act, per tier:**

   | | `private` | `upstream` |
   |---|---|---|
   | drifted | rewrite it to match the code | replace with `TODO(jb-doc)` / `TODO(jb-comment)` naming the drift |
   | missing on a public item | write it | insert `/// TODO(jb-doc): <what needs documenting>` |
   | misplaced | delete it, or reduce it to `TODO(jb-comment)` if the point is genuinely non-obvious | same |
   | fine | leave it | leave it |

5. **Report** every comment you touched as a list: file, item, and which of the
   four verdicts it got. Say plainly if you changed nothing.

What you write here is a first pass, not the final text — `/comment-review` runs
next and cuts every clause that is not required.

## When it fails

If I get a hunk wrong, that is the skill working. Say so plainly, record it, and
carry on to the next hunk. Do not stop the walkthrough and do not re-teach the
hunk at length — a paragraph of correction, then move.

## Why understanding is checked by conversation

Norm 1 is a claim about what I know, and no hook can read that — every other layer
checks bytes. It runs before the PR exists because the point is to stop code I
cannot explain from being submitted, not to find it afterwards.
