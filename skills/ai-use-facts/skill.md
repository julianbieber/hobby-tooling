---
description: Produce the factual record of what the AI actually did on this task — files written, decisions made, things reviewed but not authored — as bullets for me to write the PR's AI-use disclosure from
allowed-tools: Bash(git log:*), Bash(git diff:*), Bash(git -C:*), Bash(ls:*), Bash(pwd:*), Bash(cat:*), Bash(rg:*), Read, Glob, Grep
---

Bevy AI policy norm 2 requires every PR to disclose AI use, in the description,
with context about **how** it was used. That paragraph is mine to write. This
skill produces the record I write it from.

**Emit facts. Never a sentence of the disclosure.** No "AI was used to…", no
opening line, no suggested phrasing. Bullets only, in the categories below. If
asked to draft the disclosure, refuse and emit the facts.

## Why a record and not recall

A week into a task I cannot reliably remember which function I designed and which
I accepted after reading it. An inaccurate disclosure is worse than a terse one:
norm 2 exists so the reviewer can calibrate, and a wrong calibration is the
failure it is meant to prevent.

## Sources, in order

1. **The task journal** —
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/*.md`. Read every
   dated entry for this task.
2. **The conversation dumps** — `/tmp/claude-conversation-<session>.md`, where
   `<session>` is `$ZELLIJ_SESSION_NAME` with non-alphanumerics replaced. These
   are wiped on reboot, so treat absence as "no record", never as "no AI use".
3. **This conversation.**
4. **The diff** — `git log <base>..HEAD` and `git diff <base>...HEAD`, to catch
   files nothing in the record mentions.

## Categories

Every bullet goes in exactly one, and names the file, function or decision it is
about. Where the record does not say, put it in **Unrecorded** rather than
guessing.

```markdown
### Written by AI, reviewed by me
- `path/to/file.rs::fn_name` — <what it does>

### Designed by me, implemented by AI
- <the decision I made> → `path/to/file.rs`

### Written by me
- `path/to/file.rs::fn_name`

### AI as critic only
- reviewed <what>, found <what>; the change was mine

### Verified by me personally
- <claim> — by <how: ran the test, read the code, reproduced it>

### Not verified by me
- <claim> — <what would verify it>

### Unrecorded
- <file or area with no provenance in the journal or dumps>
```

## Instructions

1. Resolve project / topic / task from `pwd`.
2. Read the journal entries, then the conversation dumps, then this conversation.
3. Diff the file list in the record against the file list in
   `git diff <base>...HEAD --name-only`. Anything in the diff and not in the
   record goes under **Unrecorded**.
4. Emit the bullets. Nothing else — no preamble, no summary, no closing line.
5. State the counts: how many files are recorded, how many unrecorded.

A large **Unrecorded** section is a finding, not an inconvenience. Say so plainly
and name the files; do not fill it in by inference.

## TODO(jb-doc): why this stops short of the paragraph

<!-- What it should cover: where norm 2 and norm 6 meet, and what a skill that
     offered "just a first draft" would cost. -->
