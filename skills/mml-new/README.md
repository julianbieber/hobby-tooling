# MML New

| Field        | Value                                                                                                                                                           |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Type         | skill                                                                                                                                                           |
| Install path | `~/.claude-hobby/commands/mml-new.md`                                                                                                                                             |
| Owner        | @julianbieber                                                                                                                                                   |
| Dependencies | hobby MML grammar (`lsps/hobby-mml-lsp/grammar.md`), notes repo `~/fun_repos/hobby-mimisbrunnr`, `template{,-plugin,-system}.mml` (skill assets), `mml-translate` |
| Status       | experimental                                                                                                                                                    |

## Description

Scaffolds a skeleton [`.mml`](../../lsps/hobby-mml-lsp/grammar.md) (Mental
Model Language) spec for a **new** unit of behaviour — one that has no code yet
— so I can edit it into the shape I want before `mml-translate` turns it
into code. This is the "building a new feature" entry to the MML loop; its
sibling `mml-extract` is the "adjusting an existing feature" entry (code
→ `.mml`).

Two paths, chosen from what I give it:

- **Blank template** — I want to write the spec by hand; the skill drops a
  correctly-structured empty spec (from the skill asset matching the `kind`: `template.mml`, `template-plugin.mml`, or `template-system.mml`) at the
  right path so I start well-formed.
- **Draft from a rough idea** — I describe the feature in a sentence or two and
  the skill drafts a first-pass `.mml` (best-guess modules / data / behavior /
  invariants), clearly marked `# DRAFT`, that I then edit down.

Produces:

- `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.mml` —
  one spec, header + the four `##` sections, placed directly in the current
  task's notes dir. A new unit usually has no `source` block yet.

Writes the `.mml` and stops — reviewing, editing, and committing it are my job.
The spec becomes real when I edit it and commit it (in the notes repo) as the
baseline `mml-translate` diffs against.

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`, or manually:

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/mml-new/skill.md \
  ~/.claude-hobby/commands/mml-new.md
```

## Usage

```
/mml-new a nightly job that purges price-history older than the retention window, per contract
/mml-new blank endpoint spec for bevy-survivor called spawn-wave
```

Give it a rough idea (it drafts a first-pass spec) or ask for a blank template
to fill in yourself. Either way it writes
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.mml`; you
then edit, commit, and run `/mml-translate <unit>`.

## Known Limitations

- **A draft is a best guess, not a spec to trust.** The value is a starting
  point to react to and edit down; the modules, data model, and behavior are the
  model's guess from a one-line idea, never ground truth.
- Writes and stops — it does not iterate with you, and it never runs `git add` /
  `git commit` in the notes repo. The spec is only a translate baseline once
  **you** commit it.
- A new unit typically has **no `source` block** until you know the target repo
  and paths; without one, `mml-translate` falls back to full generation.
- No `.mml` parser exists yet (task 06), so output is validated against the
  grammar by eye until it does.
