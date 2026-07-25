---
description: Draft a reviewer-focused PR description (why + expected behavior, not implementation) and create the PR with gh
allowed-tools: Bash(git branch:*), Bash(git -C:*), Bash(git log:*), Bash(git diff:*), Bash(git rev-parse:*), Bash(ls:*), Bash(pwd:*), Bash(cat:*), Bash(gh:*), Read
---

Draft a pull-request description that guides the reviewer, then create the PR
with `gh pr create`. The body explains **why** the change exists and **what the
behavior is after merge** — not how it's implemented (the code shows that).

Hand-off note: the `edit-before-execute` hook opens `$EDITOR` on the
`gh pr create` command so you can finalize and approve the body there. This
skill produces ONE clean, complete `gh pr create` command and stops. Do not try
to re-implement editing or prompt for body text yourself.

## Writing Standard (applies to everything this skill writes)

- **Evidence-gated:** Only call something done/working when there is evidence —
  a passing test (name it), command output, an open/merged PR (#), a commit SHA,
  a DB/log query result. Without evidence, label it "in progress" / "attempted",
  never "done".
- **No speculation:** Drop hedges and predictions ("likely", "should",
  "probably", "this will fix"). State only what is known. If something is
  unverified, say so plainly ("not yet verified").
- **Cite the source:** Every non-obvious claim names its evidence inline — PR #,
  commit SHA, file path, test name, or the exact DB/log query used. A query or a
  log line is a valid citation.

## Instructions

1. **Extract the task ID** (and, if the task carries one, the GitHub issue number
   for the title and body):

   a. **From worktree path** (`~/fun_worktrees/PROJECT/TOPIC/TASK`): run `pwd`;
   the **third** subdirectory is the task ID, the first is the project and the
   second the topic. Example:
   `~/fun_worktrees/bevy_survivor/features/gh-42-enemy-waves` -> project
   `bevy_survivor`, topic `features`, task `gh-42-enemy-waves`.

   b. **From git branch**: `git branch --show-current` — extract a leading
   `gh-<number>` if present. Skip `main`, `master`, `develop`.

   c. **From recent conversation**: if a task ID was mentioned.

   A `gh-<number>` prefix gives the issue number. Many hobby tasks have no issue —
   that is fine, just omit the issue references from the title and body.

2. **Determine the base branch and gather change context**:

   - Base: default to the repo's default branch
     (`gh repo view --json defaultBranchRef -q .defaultBranchRef.name`); if the
     branch was cut from a different base, ask the user.
   - `git log <base>..HEAD --oneline` — the commits (SHAs + subjects).
   - `git diff <base>...HEAD --stat` — the files touched, for YOUR understanding
     of scope only. **Do NOT paste this into the body.**
   - Read the conversation for the intent behind the change.

3. **Fetch the GitHub issue**, when the task has a `gh-<number>` prefix:
   `gh issue view <number> --json title,body,comments` — to ground the *why* and
   the expected behavior. Capture the title, body, and any acceptance criteria.
   With no issue, the conversation and the commits are the only source for the
   *why*, so lean on them harder.

4. **Check for an existing PR**: `gh pr view --json number,url,state` — if one
   already exists, tell the user and switch to suggesting
   `gh pr edit --body ...` rather than creating a duplicate.

5. **Read the repo's PR template** if present at
   `.github/pull_request_template.md` (relative to the repo root — find it with
   `git rev-parse --show-toplevel`). Append it verbatim beneath the drafted body
   so reviewers still complete it. **Auto-check only the boxes you can back with
   evidence** (e.g. tick "tests pass" only if you have the passing output, "no
   breaking changes" only if that's verifiably true); leave every other box
   unchecked.

6. **Draft the body** with the PR-Body Template below. Hard rules:

   - **Guide the reviewer**: lead with *why* this PR exists and *what behaves
     differently* after merge.
   - **No implementation details.** No diffs, no file walkthroughs, no "changed
     function X to do Y". The reviewer reads the code for *how*.
   - Apply the Writing Standard: evidence-gated verification, no speculation,
     cite sources (PR/commit/test/query).
   - Include a `Closes #<n>` line only when the task has an issue number.

7. **Compose the title**: `gh-{number}: {concise summary}` (e.g.
   `gh-42: Spawn enemy waves on a timer`). With no issue number, use a plain
   concise summary — hobby PRs often have no issue, so do not block on it.

8. **Hand off to `gh pr create`** using the invocation below — emit it as a
   single Bash command with the body inline. Stop after issuing it: the
   `edit-before-execute` hook opens `$EDITOR` so you finalize and approve the
   body. Do not post, edit, or re-run on your own afterward; just report the
   resulting PR URL.

9. If no task ID can be determined, draft with a plain title and carry on — do
   not block. Drop the `Closes #` line when there is no issue.

## PR-Body Template

Reviewer-facing. *Why* + *expected behavior* + *how to verify*. NOT
implementation.

```markdown
## Why

{The problem or need this PR addresses, in plain terms — what was wrong, missing,
or requested. Cite evidence where the problem is observable: a failing test, a
log/DB query, an incident, the GitHub issue.}

## Expected behavior after merge

{What a user / caller / operator observes once this is merged. Describe outcomes
and contracts, not code paths. Call out anything that changes externally: API
responses, config, deploys, data.}

## Verification

{What has been verified, with citations — test names, command output, query
results, PR checks. Mark anything not yet verified as "not yet verified". No
speculation.}

## Scope / out of scope

{Anything intentionally left out, follow-ups, or boundaries that help the
reviewer know what NOT to expect here.}

Closes #{GH_ISSUE}

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

If the repo has `.github/pull_request_template.md`, append its contents verbatim
below the block above (checkboxes ticked only where evidence-backed, per step
5).

## gh pr create invocation

Emit exactly one command. The body is inline (via a quoted heredoc) so the
`edit-before-execute` hook can present it in `$EDITOR` as editable text — a
`--body-file` would hide the body from the editor.

```bash
gh pr create \
  --base "<base-branch>" \
  --title "gh-<number>: <summary>" \
  --body "$(cat <<'PRBODY'
## Why

...

## Expected behavior after merge

...

## Verification

...

## Scope / out of scope

...

Closes #<GH_ISSUE>

🤖 Generated with [Claude Code](https://claude.com/claude-code)

<!-- repo PR-template checklist appended verbatim below, if one exists -->
PRBODY
)"
```

Notes:

- Add `--draft` only if the user asked for a draft PR.
- Do **not** pass `--fill` — it would overwrite the authored body with commit
  text.
- The hook treats an edited-and-saved command as approval; an emptied command as
  rejection. After issuing the command, stop and report the resulting PR URL.
