# MML Extract

| Field        | Value                                                                              |
| ------------ | ---------------------------------------------------------------------------------- |
| Type         | skill                                                                              |
| Install path | `~/.claude-hobby/commands/mml-extract.md`                                                            |
| Owner        | @julianbieber                                                                      |
| Dependencies | MML grammar (`llm_mental_model/01-language-spec`), `tree-extract` & `lsp-mcp` MCPs |
| Status       | experimental                                                                       |

## Description

Analyzes one unit of an existing system — an endpoint, a message/queue workflow,
or a scheduled job — and emits a valid
[`.mml`](~/projects/team-constellation-mimisbrunnr/notes/llm_mental_model/DESIGN.md) (Mental Model Language)
spec: the four sections **modules / data / behavior / invariants** that capture
how the unit behaves at mental-model altitude. The human then edits the `.mml`
to drive a change, which `mml-translate` turns back into code — spec →
code, not prose → code.

You point at one unit ("the `/tiles/{z}/{x}/{y}` endpoint", "the shader reload batch
consumer"); the skill traces the call graph with `tree-extract`, reads the
behavior off the source, and writes the file into the current task's notes dir.

Produces:

- `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.mml` —
  one spec, header + the four `##` sections, `source.repo`/`source.paths`
  pointing at the real code.

Emits the `.mml` and stops — reviewing, editing, and committing it are the
human's job (that is the point of the artifact).

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`, or manually:

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/mml-extract/skill.md \
  ~/.claude-hobby/commands/mml-extract.md
```

## Usage

```
/mml-extract the shader reload batch worker
```

Point it at a unit (an endpoint, a queue/message workflow, or a job) or a
file/function. It writes
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.mml`.

## Known Limitations

- The value is the **altitude** of the `## behavior` section — mental-model
  steps, not a transcription. That judgement is model-dependent; treat the
  output as a first draft to edit, never as ground truth.
- The `## modules` map is only as good as `tree-extract`'s graph for the repo;
  cross-file / external calls may be missed, so verify the module list.
- Emits and stops — it does not iterate with you, and it never runs `git add` /
  `git commit` in the notes or tooling repos.
- No `.mml` parser exists yet (task 06), so output is validated against the
  grammar by eye until it does.
