---
description: Draft or refresh a GitHub issue body from the current conversation, then create or edit it with gh (with confirmation)
allowed-tools: Bash(git branch:*), Bash(git remote:*), Bash(pwd:*), Bash(gh:*), Read
---

Write a proper GitHub issue **body**, derived from the **current conversation**.
Issues are usually opened before any notes exist, so this skill does NOT read the
notes directory — the conversation is the source. It confirms with you before
writing and preserves any meaningful existing body rather than overwriting
blindly.

## Writing Standard (applies to everything this skill writes)

- **Evidence-gated:** Only call something done/working when there is evidence —
  a passing test (name it), command output, an open/merged PR (#), a commit SHA.
  Without evidence, label it "in progress" / "attempted", never "done".
- **No speculation:** Drop hedges and predictions ("likely", "should",
  "probably", "this will fix"). State only what is known. If something is
  unverified, say so plainly ("not yet verified").
- **Cite the source:** Every non-obvious claim names its evidence inline — PR #,
  commit SHA, file path, test name, or the exact command used.

## Instructions

1. **Determine the project (repo)** — `gh` needs to resolve one:

   a. **From worktree path** (`~/fun_worktrees/PROJECT/TOPIC/TASK`): run `pwd`;
   the **first** component is the project. Example:
   `~/fun_worktrees/bevy_survivor/features/enemy-waves` -> `bevy_survivor`.

   b. **From the current repo**: `git remote get-url origin`.

   c. Otherwise ask which project the issue belongs to.

   Run `gh` from the worktree or from `~/fun_repos/{project}` so it targets the
   right repo. If the project has no GitHub remote, say so and stop — there is
   nowhere to file an issue.

2. **Decide: new issue, or refresh an existing one?**

   - If the task name or branch carries a `gh-<number>` prefix, or the user named
     an issue, treat this as a **refresh** of that issue.
   - Otherwise this is a **new** issue.

3. **For a refresh, fetch the current issue** and capture its title and body:

   ```bash
   gh issue view <n> --json title,body,state,labels
   ```

4. **Assess the existing body**:

   - If empty or boilerplate, draft fresh.
   - If it has meaningful content (a real problem statement, acceptance criteria,
     links), **preserve and incorporate it** — fold it into the template rather
     than discarding. Do not drop links, issue references, or notes-to-self.

5. **Synthesize the body FROM THE CONVERSATION** (not from notes — they typically
   don't exist yet). Pull the problem, goal, scope, acceptance criteria, and
   out-of-scope from what has been discussed this session. Apply the Writing
   Standard. Where something is genuinely unknown, write a clearly-marked `TODO`
   rather than guessing.

6. **Show the proposed body to the user and CONFIRM before writing.** Present the
   full draft. For a refresh, state explicitly that you are about to replace the
   existing body. Do not run `gh` until the user approves. Incorporate any edits
   they give.

7. **Write it:**

   a. **New issue:**
      ```bash
      gh issue create --title "<title>" --body "$(cat <<'BODY'
      ...
      BODY
      )"
      ```
      Ask for a title first if the conversation doesn't imply an obvious one.
      Suggest labels only if the repo already uses them (`gh label list`).

   b. **Existing issue:**
      ```bash
      gh issue edit <n> --body "$(cat <<'BODY'
      ...
      BODY
      )"
      ```
      Body only — leave title, labels, assignees, and state alone unless asked.

8. **Confirm to the user** what was written and report the issue URL.

9. **Suggest the follow-up**, if a worktree doesn't exist yet: the issue number
   makes a good task name, so
   `hwt new gh-{number}-{slug} -p {project} -t {topic}` keeps the task, the
   branch, and the issue traceable to each other.

## Issue-Body Template

```markdown
## Problem

{What's wrong, missing, or wanted — grounded in the conversation. Cite evidence
where the problem is observable (command output, a log line, a failing test). No
speculation.}

## Goal

{The intended outcome — what "solved" looks like. One or two sentences.}

## Scope / acceptance criteria

- [ ] {concrete, checkable criterion}
- [ ] {concrete, checkable criterion}

## Out of scope

- {explicitly excluded work or follow-ups}

## Context / links

- {related issues (#n), PRs, docs, commands surfaced in the conversation. TODO
  any unknowns rather than guessing.}
```
