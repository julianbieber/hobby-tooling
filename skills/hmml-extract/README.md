# HMML Extract

| Field        | Value                                                                                         |
| ------------ | --------------------------------------------------------------------------------------------- |
| Type         | skill                                                                                         |
| Install path | `~/.claude-hobby/commands/hmml-extract.md`                                                    |
| Owner        | @julianbieber                                                                                 |
| Dependencies | HMML grammar (`lsps/hobby-mml-lsp/grammar.md`), `hobby-mml-parser`, `tree-extract` & `lsp-mcp` MCPs |
| Status       | experimental                                                                                  |

## Description

Reads a branch's diff back as one `.hmml` **review delta**: the touched units'
behaviour walks as the branch leaves them — a system with the schedule that runs
it, an observer with its event — with each changed step marked `+`, `-` or `~`
and linked to its line in the diff, plus the data shapes (with their ECS roles)
and public signatures the change touches and a grouped summary of its tests.
`/task:build` runs it in its `delta` phase and shows it at GATE 2.

Produces one file per PR, in the notes dir of the checkout it reads:

- `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/delta-<branch>.hmml`
- `…/{task}/aspects/{aspect}/delta-<branch>.hmml` for an aspect's worktree

The file is derived from the diff and rewritten on every run.

## Why one file per PR

This follows the work MML v2 change. The per-unit spec model wrote every touched
unit, collaborator and module as a full spec whose `# +` / `# -` comment markers
had to reconstruct the base, which made a delta many files and mostly markers.
Nothing plans in HMML any more, so that property had no consumer; a gutter on
each line, context kept around the changed steps, and a diff link on every
changed line is what reading a change needs.

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`; the linter
by `just install-mml`.

## Usage

```
/hmml-extract                       # current worktree, against the merge base
/hmml-extract --against <sha>       # only what changed since <sha>
```

## Known Limitations

- The value is the altitude of the walks. That is a judgement call, so read the
  delta next to the diff, not instead of it.
- Units are found by walking callers up to an `add_systems` / `add_observer`
  registration; a system registered through a helper or a macro can be missed.
  The report lists the units it found and the changed files it left uncovered.
