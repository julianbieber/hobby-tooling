# Journal: Resume

| Field        | Value                                               |
| ------------ | --------------------------------------------------- |
| Type         | skill                                               |
| Install path | `~/.claude-hobby/commands/journal/resume.md`                          |
| Owner        | @julianbieber                                       |
| Dependencies | Notes repo; `gh` CLI (optional, fallback only) |
| Status       | experimental                                        |

## Description

Resume a task by loading context from team notes — without dumping every journal
file into the session. The task is auto-detected from the worktree path or git
branch, then a subagent surveys the notes (`implementation.md`, recent daily
files, the `insights.md` heading index, `task.json`, `prs.json`) and returns a
short state briefing plus a shortlist of candidate next steps. You pick one (or
type your own), and the same subagent returns a **focused** package: the goal
narrowed to that step plus only the prior work, insights, files, and PRs that
bear on it. The big files stay in the subagent's context, keeping the main
session lean.

Fresh / empty tasks (no journal `.md` yet) skip the subagent and fall back to a
plain issue summary and a "how do you want to proceed?" prompt.

## Installation

```bash
mkdir -p ~/.claude-hobby/commands/journal/
ln -s ~/fun_repos/hobby-tooling/skills/journal/resume/skill.md \
  ~/.claude-hobby/commands/journal/resume.md
```

Or: `just install-skills`.

## Usage

```
/journal:resume
/journal:resume <what you want to work on next>
```

The optional argument biases the shortlist toward a specific next step and can
shortcut the pick prompt when it resolves to a single candidate.

## Known Limitations

- Requires the team notes repo at
  `~/fun_repos/hobby-mimisbrunnr/notes/`.
- The issue fallback only fires when the notes dir holds an `issue.md`; tasks
  without one degrade to "summarize what exists and ask".
