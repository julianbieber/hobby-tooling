---
description: Gather the facts a GitHub issue needs, resolve its topic label, and open the authoring buffer so I write the issue myself
allowed-tools: Bash(git branch:*), Bash(git remote:*), Bash(pwd:*), Bash(ls:*), Bash(mkdir:*), Bash(gh:*), Read, Write
---

Resolve which repo and topic an issue belongs to, gather what has been established
about the problem, and open the authoring gate so **I** write the title and body.

**You do not write the issue body or title.** An issue is a public problem
statement, and Bevy AI policy norm 9 requires those to be personally authored. Not
a draft, not an outline, not a suggested title. If asked to write it, say what
this skill does instead and run it.

Issues are usually opened before any notes exist, so this skill does not read the
notes directory — the conversation is the source.

## Facts standard (applies to everything this skill writes)

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

`hdash` (the work dashboard) reads that label and nothing else: with it, an
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

4. **Capture the existing body verbatim** on a refresh. Quote it into the facts
   file whole — links, issue references and notes-to-self included. I decide what
   to keep. Do not fold, merge or condense it.

5. **Gather the facts from the conversation** — what was established this session,
   with citations, under the layout below. Facts only: the command that failed and
   its output, the file and line, the issue numbers mentioned. No problem
   statement, no goal, no acceptance criteria — those are the issue, and the issue
   is mine to write.

   Write them to `${XDG_RUNTIME_DIR:-/tmp}/hobby-authoring-gate/facts.md`
   (`mkdir -p` first).

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

7. **Report the topic label you resolved** — say when the label or the topic
   itself would be new, and when a refresh would replace an existing body.

8. **Open the gate.** Run exactly:

   a. **New issue:**
      ```bash
      gh issue create --label "topic:{topic}"
      ```
      No `--title`, no `--body`. Add further labels only if the repo already uses
      them (`gh label list`).

   b. **Existing issue:**
      ```bash
      gh issue edit <n>
      ```
      Add `--add-label "topic:{topic}"` when the issue carries no topic label —
      an untagged issue cannot be started from the dashboard. Never remove or
      replace other labels, and leave title, assignees and state alone unless
      asked.

   The `human-authored-text` hook intercepts, opens the buffer with the facts
   attached, and denies the call. **Re-issue exactly as it instructs**, with the
   `--title` and `--body-file` it names and every other flag you had. Change
   nothing in the file.

9. **Report** the issue URL and the topic label it carries. If the gate says I
   wrote nothing, say the issue was not filed and stop — do not retry, do not
   offer to draft it.

10. **Suggest the follow-up**, if a worktree doesn't exist yet — either of:

    - `hdash`, and click **Start task** on the issue: with the `topic:` label in
      place it runs `hwt new` + `hcdev wt` in one go.
    - by hand: `hwt new gh-{number}-{slug} -p {project} -t {topic}`.

    Both keep the task, the branch, and the issue traceable to each other, and
    `gh-{number}-{slug}` is exactly the task name the dashboard derives.

## Facts file layout

Facts, with citations. The buffer the gate opens carries the issue's own headings
— Problem, Goal, Scope, Out of scope, Context — and those are mine to fill.

```markdown
## Observed
- `<command run>` → <output, quoted>
- <file>:<line> — <what is there>

## Referenced this session
- #<n> <title>
- <url>

## Existing body (refresh only)
<the current body, verbatim, in full>

## Topic
topic:<topic> — <existing | new>, label <exists | to be created>

## Unknown
- <what was discussed but not established>
```
