# AI Use Facts

| Field        | Value                                      |
| ------------ | ------------------------------------------ |
| Type         | skill                                      |
| Install path | `~/.claude-hobby/commands/ai-use-facts.md` |
| Owner        | @julianbieber                              |
| Dependencies | task journal in hobby-mimisbrunnr; `claude-conversation` hook |
| Status       | experimental                               |

## Description

Produces the factual record of what the AI did on a task — files it wrote,
decisions I made, what it only reviewed, what I verified myself — as bullets under
fixed headings. I write the PR's `## AI use` disclosure from it.

It emits no part of the disclosure paragraph.

Called by `/pr-facts`; also usable on its own.

## Categories

`Written by AI, reviewed by me` · `Designed by me, implemented by AI` ·
`Written by me` · `AI as critic only` · `Verified by me personally` ·
`Not verified by me` · `Unrecorded`

## Installation

```bash
just install-skills
```

## Usage

```
/ai-use-facts
```

## Known Limitations

- Conversation dumps live in `/tmp` and are wiped on reboot, so the record thins
  out for a task spanning a restart. `Unrecorded` grows instead of the skill
  guessing.
- Provenance for a file edited across many sessions is only as good as the
  journal entries.

## TODO(jb-doc): why the ledger is worth keeping at all

<!-- What it should cover: what norm 2 asks for that memory cannot supply a week
     later, and why an inaccurate disclosure is worse than a terse one. -->
