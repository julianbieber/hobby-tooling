# HMML Extract

| Field        | Value                                                                              |
| ------------ | ---------------------------------------------------------------------------------- |
| Type         | skill                                                                              |
| Install path | `~/.claude-hobby/commands/hmml-extract.md`                                                            |
| Owner        | @julianbieber                                                                      |
| Dependencies | HMML grammar (`lsps/hobby-mml-lsp/grammar.md`), `tree-extract` & `lsp-mcp` MCPs |
| Status       | experimental                                                                       |

## Description

Analyzes one unit of an existing system — an endpoint, a message/queue workflow,
or a scheduled job — and emits a valid
[`.hmml`](../../lsps/hobby-mml-lsp/grammar.md) (Mental Model Language)
spec: the four sections **modules / data / behavior / invariants** that capture
how the unit behaves at mental-model altitude. Plain extraction writes the
**baseline** — the *current* version of the unit, unannotated.

With `--against <ref>` it writes the **review delta** instead: the unit as the
worktree leaves it, with what `git diff --merge-base <ref>` changed marked `# -` / `# +`
(grammar §10), so a finished change can be read as a change to the mental model
before merge. `/task:build` runs it after the code is implemented. The file is
derived and regenerated whenever the diff moves, never maintained by hand.

## Why the delta is derived, not designed

The spec used to be annotated with the intended change *before* the code, then
translated into code. Reviewing that plan and then the real diff was a double
review, and the plan drifted from what was built — "reviewing the version that
works seems more productive than the double review from before". The HMML
exists so I understand what the code change is doing, so it represents the
implemented change.

You point at one unit ("the `/tiles/{z}/{x}/{y}` endpoint", "the shader reload batch
consumer"); the skill traces the call graph with `tree-extract`, reads the
behavior off the source, and writes the file into the current task's notes dir.

Produces:

- `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.hmml` —
  the overview: header + the `##` sections the `kind` takes,
  `source.repo`/`source.paths` pointing at the real code;
- one `<module>.hmml` beside it per collaborator with a public surface — a
  `kind: module` spec carrying that module's `## api`, function by function
  (grammar §11), with the overview's steps linked into it by member ref.

## Why both layers come out of one run

The overview names its collaborators and stops, so a change about a module's API
rather than a unit's flow had nothing to describe it in. The module specs are
that missing layer.

They cannot be a second run. A member ref into a spec that does not exist yet is
an **error**, not a warning — that is what makes the ref worth navigating by — so
an overview emitted alone hands back a spec that does not lint. One run writes
the set, then lints the set.

A collaborator that exposes nothing is **skipped and reported**: `## api` would
be empty, which raises `empty-api`, which the lint step then tells you to fix. It
keeps its `## modules` line, because it still does work in the behaviour, and it
gets no `@ spec` ref.

Emits the `.hmml` files and stops — reviewing, editing, and committing it are the
human's job (that is the point of the artifact).

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`, or manually:

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/hmml-extract/skill.md \
  ~/.claude-hobby/commands/hmml-extract.md
```

## Usage

```
/hmml-extract the shader reload batch worker
```

Point it at a unit (an endpoint, a queue/message workflow, or a job) or a
file/function. It writes
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.hmml`.

## Known Limitations

- The value is the **altitude** of the `## behavior` section — mental-model
  steps, not a transcription. That judgement is model-dependent; treat the
  output as a first draft to edit, never as ground truth.
- The `## modules` map is only as good as `tree-extract`'s graph for the repo;
  cross-file / external calls may be missed, so verify the module list.
- Emits and stops — it does not iterate with you, and it never runs `git add` /
  `git commit` in the notes or tooling repos.
- Output is checked with `hobby-mml-parser <path> --lint`; altitude and naming
  remain judgement calls the grammar cannot settle.
