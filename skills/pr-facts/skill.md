---
description: Gather the facts a PR description needs — commits, diffstat, issue, evidence, prior art, AI-use record — and open the authoring buffer so I write the body myself
allowed-tools: Bash(git branch:*), Bash(git -C:*), Bash(git log:*), Bash(git diff:*), Bash(git rev-parse:*), Bash(ls:*), Bash(pwd:*), Bash(cat:*), Bash(mkdir:*), Bash(gh:*), Read, Write
---

Assemble everything a PR description needs to be written **from**, write it to
the authoring gate's facts file, then run `gh pr create` so the gate opens the
buffer.

**You do not write the PR body.** Not a draft, not an outline, not a suggested
sentence. Bevy AI policy norm 8 bans AI-drafted PR descriptions; norm 6 asks for
text synthesized from scratch rather than revised from generated text. Your job
is the evidence; mine is the prose.

If asked to "write the PR description", say what this skill does instead and run
it. Do not comply.

## Facts standard

Everything you write into the facts file is a **fact with a citation**. No
characterization, no summary sentence, no "this PR improves…".

- A test is passing only if you have its name and its output.
- A claim names its source: commit SHA, file path, test name, issue number, or
  the exact command run.
- Anything unverified is listed under **Not verified**, with what would verify it.
- No adjectives about the change's quality, size, or importance.

## Instructions

1. **Identify the task.** `pwd` → `~/fun_worktrees/PROJECT/TOPIC/TASK`; the third
   component is the task, and a leading `gh-<number>` is the issue. Fall back to
   `git branch --show-current`. Many hobby tasks have no issue — fine, omit it.

2. **Base branch.**
   `gh repo view --json defaultBranchRef -q .defaultBranchRef.name`. If the branch
   was cut elsewhere, ask.

3. **Check for an existing PR** — `gh pr view --json number,url,state`. If one
   exists, stop and tell me; use `gh pr edit` rather than opening a duplicate.

4. **Gather. Do not interpret.**

   - `git log <base>..HEAD --oneline` — commits.
   - `git diff <base>...HEAD --stat` — files and line counts.
   - `gh issue view <n> --json title,body` when there is an issue — record the
     title and any stated acceptance criteria **verbatim**, quoted.
   - Test evidence from this conversation: the command run, its output, the names
     of the tests that passed. If none was run, that is a fact too.
   - **Prior art** (norm 5): `gh pr list --search "<keywords>" --state all` and
     `gh issue list --search "<keywords>"`. List anything overlapping.
   - **Size** (norm 4): files touched and total lines changed.
   - `.github/pull_request_template.md`, if present — record its path only. Do
     not tick any box; I do that.

5. **Run `/ai-use-facts`** and include its output verbatim under `## AI use`.

6. **Write the facts file** to
   `${XDG_RUNTIME_DIR:-/tmp}/hobby-authoring-gate/facts.md`, using the layout
   below. `mkdir -p` the directory first.

7. **Open the gate.** Run exactly:

   ```bash
   gh pr create --base <base>
   ```

   Add `--draft` only if I asked for a draft. **No `--title`, no `--body`.** The
   `human-authored-text` hook intercepts this, opens the buffer with the facts
   attached, and denies the call.

8. **Re-issue when the gate hands you the files**, exactly as instructed, with
   the title and `--body-file` it names and every other flag you had. Change
   nothing in the file. Then report the PR URL and stop.

9. If the gate reports that I wrote nothing, do not retry and do not offer to
   draft it. Say the PR was not opened, and stop.

## Facts file layout

```markdown
## Commits
<sha> <subject>            (verbatim from git log)

## Files
<diffstat, verbatim>
N files, +A / -B

## Issue #<n>: <title>
<acceptance criteria, quoted verbatim; omit the section if there is no issue>

## Evidence
- ran: `<command>` → <result>
- passing: <test names>

## Not verified
- <claim> — would need <what>

## Prior art
- #<n> <title> (<state>) — <how it overlaps>
- none found for: <the searches you ran>

## AI use
<verbatim output of /ai-use-facts>

## Repo PR template
.github/pull_request_template.md — <present | absent>
```

## TODO(jb-doc): why this skill stops at facts

<!-- What it should cover: which norm bans the drafted body, and why leaving the
     gate to enforce it would not be enough on its own. -->
