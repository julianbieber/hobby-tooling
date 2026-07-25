# MML Review

| Field        | Value                                                                                                  |
| ------------ | ------------------------------------------------------------------------------------------------------ |
| Type         | skill                                                                                                  |
| Install path | `~/.claude-hobby/commands/mml-review.md`                                                                                 |
| Owner        | @julianbieber                                                                                          |
| Dependencies | A `.mml` spec in the task notes (`mml-new` / `mml-extract`); the source repo it links to |
| Status       | experimental                                                                                           |

## Description

Reviews an **adjusted or new `.mml` spec** — one that expresses the *intended*
behaviour of a task — through parallel engineering lenses (security,
architecture, correctness/QA, performance, maintainability) and synthesizes a
**concrete implementation plan** for making the code match that intent.

It is the counterpart to, not a duplicate of, `mml-validate`:

- `mml-validate` asks "does the **existing code** still conform to the spec?"
  (code-vs-spec drift). Useless right after you edit a spec toward a new goal —
  it just reports the intended edit as drift.
- `mml-review` asks "the spec **is** the intent and the code does not match it
  yet — is the intent sound, and how do we implement it?" It produces the plan.

Parallel isolated subagents each review the spec + the current code from one
lens, then a synthesis pass filters and merges their concerns into a plan (goal
/ approach / ordered steps / risks / open questions), written to the task notes
as `plan-{YYYY-MM-DD}-{unit}.md` and echoed inline. Reviews and plans only —
never edits the spec or the code.

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`, or manually:

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/mml-review/skill.md \
  ~/.claude-hobby/commands/mml-review.md
```

## Usage

```
/mml-review                       # auto-detect the task's single .mml
/mml-review mesh-terrain-chunks  # a unit in the current task dir
/mml-review path/to/spec.mml      # explicit path
/mml-review --min-priority high   # only high/critical concerns in the plan
```

It is invoked automatically by `/julian:dev-flow` at step 3 (self-review of the
adjusted spec); the plan it writes is what the human reviews at GATE 1 alongside
the `.mml`.

## Known Limitations

- Reviews and plans only — never edits the spec or the code.
- Quality of the plan depends on the spec's `source` block pointing at real
  code; a spec with no `source` yet gets a thinner, feasibility-light plan.
- Multi-lens fan-out spends more tokens than a single pass; scope it with
  `--min-priority` for small changes.
