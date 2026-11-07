# Reflect

| Field        | Value                                                                  |
| ------------ | ---------------------------------------------------------------------- |
| Type         | skill                                                                  |
| Install path | `~/.claude-hobby/commands/reflect.md`                                                    |
| Owner        | @julianbieber                                                          |
| Dependencies | Notes repo; `~/.claude-hobby/context/known-issues.md`; this repo's `scripts/` and `tools/` |
| Status       | experimental                                                           |

## Description

Analyzes all journal entries in the notes repo to surface recurring
investigations, rediscovered procedures, lost action items, error-prone
patterns, and stale stubs. Then implements the agreed fixes: `known-issues.md`
updates, reusable tools (a script under `scripts/` or a crate under `tools/` in
this repo), action item tracking, stub cleanup, and journal condensing.

Uses Explore subagents (one per topic directory) to parallelize the initial read
phase.

## Installation

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/reflect/skill.md \
  ~/.claude-hobby/commands/reflect.md
```

Or: `just install-skills`.

## Usage

```
/reflect
```

## Known Limitations

- Phase-2 pattern detection is judgement-based; expect false positives when
  journals are very short.
