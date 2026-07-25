# Journal

| Field        | Value                                                      |
| ------------ | ---------------------------------------------------------- |
| Type         | skill                                                      |
| Install path | `~/.claude-hobby/commands/journal.md`                                        |
| Owner        | @julianbieber                                              |
| Dependencies | Notes repo at `~/fun_repos/hobby-mimisbrunnr/` |
| Status       | experimental                                               |

## Description

Documents what was accomplished and what remains open for the current task by
writing per-day journal files plus `implementation.md` and `insights.md` into
the team notes repo. Extracts task ID and topic from worktree path, git branch,
or recently modified note files.

Produces:

- `YYYY-MM-DD.md` — daily Done / Open / Notes / Action Items entries; open items
  from the previous daily are carried forward with a
  `*(carried from YYYY-MM-DD)*` marker so they survive day boundaries
- `implementation.md` — goal, requirements, plan, current state snapshot
- `insights.md` — technical learnings worth keeping across sessions, plus a
  `## Caveats` section for anything a fresh session must not violate
- `notes/{project}/{topic}/shared.md` (multi-arm topics only) — caveats, ruled-out
  approaches, and insights that hold for the whole task family

Filters out trivia (compile errors, throwaway debugging); keeps design
decisions, schema/infra changes, blockers, deployed-code bugs. Caveats are
exempt from the trivia filter and are routed to `insights.md`/`shared.md`, never
only into a daily Notes section. Ends with a forcing question: "what would bite
a fresh session that isn't written down yet?"

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`, or manually:

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/journal/skill.md \
  ~/.claude-hobby/commands/journal.md
```

## Usage

```
/journal
```

Runs against the current worktree / branch. Will ask for a task ID if it can't
infer one.

## Known Limitations

- Hard-coded to the `hobby-mimisbrunnr` notes repo. Fork this skill
  if you move the notes repo.
- Assumes worktrees follow the `~/fun_worktrees/PROJECT/TOPIC/TASK` layout (the task dir is the worktree).
- Never runs `git add` / `git commit` on the notes repo — you stage changes
  yourself.
