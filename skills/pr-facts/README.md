# PR Facts

| Field        | Value                                          |
| ------------ | ---------------------------------------------- |
| Type         | skill                                          |
| Install path | `~/.claude-hobby/commands/pr-facts.md`         |
| Owner        | @julianbieber                                  |
| Dependencies | `gh` CLI authenticated, `human-authored-text` hook, `/ai-use-facts` |
| Status       | experimental                                   |

## Description

Gathers the evidence a PR description is written **from** — commits, diffstat,
issue text, test output, prior art, AI-use record — writes it to the authoring
gate's facts file, and runs `gh pr create` with no title or body so the gate opens
the buffer.

It writes no part of the PR description.

Replaces `/pr-create`, which drafted the body and appended
`🤖 Generated with Claude Code`.

## Installation

```bash
just install-skills
```

`install-skills` prunes the dangling `pr-create.md` symlink on the next run.

## Usage

```
/pr-facts
```

From inside the worktree, once the commits are in place. Say "draft PR" for
`--draft`.

## Known Limitations

- One PR per branch; suggests `gh pr edit` when one already exists.
- The facts file is a single path, so two gates opening at once collide.
- Prior-art search is keyword-based and will miss a differently-worded PR.

## TODO(jb-doc): why this replaced /pr-create rather than being added beside it

<!-- What it should cover: what a skill that offers to draft the body does to the
     norm 6 habit even when the gate would reject the draft anyway. -->
