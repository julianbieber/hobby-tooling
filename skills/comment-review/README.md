# Comment Review

| Field        | Value                                        |
| ------------ | -------------------------------------------- |
| Type         | skill                                        |
| Install path | `~/.claude-hobby/commands/comment-review.md`  |
| Owner        | @julianbieber                                |
| Dependencies | `scripts/ai-policy-tier.sh`                  |
| Status       | experimental                                 |

## Description

Takes every comment in the branch diff apart clause by clause, judges which parts
are required, prints the verdict table, then cuts what did not survive.

It is the **trimming** half of the comment pipeline. `/explain-back` makes
comments true; this makes them short. It never adds one.

Runs at the `comment-review` phase of `/build:task`, after `/explain-back`.

On an `upstream` repo it reports and changes nothing — those comments are
hand-written.

## Installation

```bash
just install-skills
```

## Usage

```
/comment-review [<base>]
```

## Known Limitations

- Clause-level judgement on a comment that is one long sentence degrades to a
  whole-comment verdict.
- Cannot tell a genuinely non-obvious reason from a familiar one — what is obvious
  to the author is what this skill is worst at.

## TODO(jb-doc): why trimming is a separate pass from writing

<!-- What it should cover: why the same pass cannot both write and cut, and what
     goes wrong when the writing pass is asked to be terse instead. -->
