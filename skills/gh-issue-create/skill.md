---
description: Draft or refresh a GitHub issue body from the current conversation, then create or edit it with gh (with confirmation), tagging it with its topic
allowed-tools: Bash(git branch:*), Bash(git remote:*), Bash(pwd:*), Bash(ls:*), Bash(gh:*), Read
---

Write a proper GitHub issue **body**, derived from the **current conversation**,
and tag the issue with the **topic** its work belongs under. Issues are usually
opened before any notes exist, so this skill does NOT read the notes directory —
the conversation is the source. It confirms with you before writing and preserves
any meaningful existing body rather than overwriting blindly.

## Writing Standard (applies to everything this skill writes)

- **Evidence-gated:** Only call something done/working when there is evidence —
  a passing test (name it), command output, an open/merged PR (#), a commit SHA.
  Without evidence, label it "in progress" / "attempted", never "done".
- **No speculation:** Drop hedges and predictions ("likely", "should",
  "probably", "this will fix"). State only what is known. If something is
  unverified, say so plainly ("not yet verified").
- **Cite the source:** Every non-obvious claim names its evidence inline — PR #,
  commit SHA, file path, test name, or the exact command used.

## The topic tag (why every issue gets a label)

A hobby worktree lives at `~/fun_worktrees/PROJECT/TOPIC/TASK`. An issue already
implies two of those three: the **project** is the repo it is filed in, and the
**task** is `gh-<number>-<slug>`. **Topic is the missing component**, so the issue
carries it as a label:

```
topic:features        ->  ~/fun_worktrees/<project>/features/gh-42-<slug>
topic:perf, topic:ui  ->  first one wins; don't add two
```

`hgh-issues` (the issue dashboard) reads that label and nothing else: with it, an
issue has a one-click **Start task** button that creates the worktree and opens an
`hcdev` session; without it the issue is listed as *untagged* and cannot be
launched at all. So the tag is not decoration — it is the thing that makes an issue
actionable.

Rules:

- **Prefixed, always**: `topic:features`, never a bare `features` label. A repo's
  ordinary labels (`bug`, `enhancement`, `good first issue`) must never be read as
  topics.
- **Directory-shaped values only**: `[A-Za-z0-9._-]+`, no leading dot, no spaces.
  `topic: features` (stray space) and `topic:a/b` are rejected by the dashboard as
  untagged. When in doubt, lowercase with `-` or `_` separators.
- **Reuse an existing topic** where one fits. Existing topics are the second path
  level: `ls ~/fun_worktrees/<project>/`. A new topic is fine when the work
  genuinely doesn't belong to any of them — the label may name a topic whose
  directory doesn't exist yet.
- **One topic per issue.**

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

3. **For a refresh, fetch the current issue** and capture its title, body, and labels:

   ```bash
   gh issue view <n> --json title,body,state,labels
   ```

   A refresh normally leaves labels alone — the one exception is a **missing topic
   tag**, which step 6b adds, since an untagged issue cannot be launched.

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

6. **Resolve the topic and make sure its label exists** (see *The topic tag* above):

   a. **Pick the topic:**
      - Working inside a worktree? `pwd` gives it — the **second** component under
        `~/fun_worktrees/PROJECT/`.
      - Otherwise list what the project already uses and choose the one that fits:
        ```bash
        ls ~/fun_worktrees/{project}/
        ```
      - Nothing fits, or the project has no worktrees yet? Propose a new topic name
        and say it is new. If the conversation gives no basis at all, **ask** — do
        not invent one, and do not silently skip the tag.
      - For a **refresh**, keep the existing `topic:` label if the issue has one and
        it still fits; only propose a change when the work has genuinely moved.

   b. **Ensure the label exists in the repo** — `gh issue create --label` fails on
      an unknown label, so create it first if needed:
      ```bash
      gh label list --search "topic:" --json name --jq '.[].name'
      gh label create "topic:{topic}" --color BFD4F2 \
          --description "Task topic: worktrees land under ~/fun_worktrees/<project>/{topic}/"
      ```
      `gh label create` on an existing label is an error, not a no-op — check first.
      One shared colour for every topic label keeps them visually one family.

7. **Show the proposed body AND the topic label to the user, and CONFIRM before
   writing.** Present the full draft plus the `topic:<topic>` you resolved (say so
   when the label or the topic itself would be new). For a refresh, state explicitly
   that you are about to replace the existing body. Do not run `gh` until the user
   approves. Incorporate any edits they give.

8. **Write it:**

   a. **New issue:**
      ```bash
      gh issue create --title "<title>" --label "topic:{topic}" --body "$(cat <<'BODY'
      ...
      BODY
      )"
      ```
      Ask for a title first if the conversation doesn't imply an obvious one. Add
      further labels only if the repo already uses them (`gh label list`).

   b. **Existing issue:** body only — leave title, assignees, and state alone unless
      asked:
      ```bash
      gh issue edit <n> --body "$(cat <<'BODY'
      ...
      BODY
      )"
      ```
      The single label exception: if the issue carries no `topic:` label, add one in
      the same command (`--add-label "topic:{topic}"`), because an untagged issue
      cannot be started from the dashboard. Never remove or replace other labels.

9. **Confirm to the user** what was written and report the issue URL and the topic
   label it carries.

10. **Suggest the follow-up**, if a worktree doesn't exist yet — either of:

    - `hgh-issues`, and click **Start task** on the issue: with the `topic:` label in
      place it runs `hwt new` + `hcdev wt` in one go.
    - by hand: `hwt new gh-{number}-{slug} -p {project} -t {topic}`.

    Both keep the task, the branch, and the issue traceable to each other, and
    `gh-{number}-{slug}` is exactly the task name the dashboard derives.

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
