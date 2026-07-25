# PR Create

| Field        | Value                                 |
| ------------ | ------------------------------------- |
| Type         | skill                                 |
| Install path | `~/.claude-hobby/commands/pr-create.md`                 |
| Owner        | @julianbieber                         |
| Dependencies | `gh` CLI authenticated |
| Status       | experimental                          |

## Description

Drafts a **reviewer-focused** PR description and creates the PR with
`gh pr create`. The body explains *why* the change exists and *what behaves
differently after merge* — deliberately **no implementation details** (the code
is the source of truth for *how*).

It gathers context from the branch diff/commits, the linked GitHub issue (when the
task has a `gh-<number>` prefix), and the
conversation, applies an evidence-gated writing standard (no speculation, cite
the source), and appends the repo's `.github/pull_request_template.md` checklist
verbatim — ticking only boxes it can back with evidence.

The drafted `gh pr create` command is handed to the `julian-edit-before-execute`
hook, which opens `$EDITOR` so you finalize and approve the body before it runs.

## Installation

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/pr-create/skill.md \
  ~/.claude-hobby/commands/pr-create.md
```

Or: `just install-skills`.

## Usage

```
/pr-create
```

Run it from inside the repo worktree once your commits are in place. Add a note
like "draft PR" if you want `--draft`.

## Known Limitations

- Assumes one PR per branch; if a PR already exists it suggests `gh pr edit`
  instead of creating a duplicate.
- The body is passed inline to `gh pr create` (so the edit hook can show it) —
  very large bodies make for a long command line.
- Relies on the issue and conversation for the *why*; if neither has the
  context, it will ask.
