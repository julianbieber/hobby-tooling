# zk Notes

| Field        | Value                                              |
| ------------ | -------------------------------------------------- |
| Type         | skill (reference)                                  |
| Install path | `~/.claude-hobby/commands/zk-notes.md`                               |
| Owner        | @julianbieber                                      |
| Dependencies | [zk](https://github.com/zk-org/zk) CLI; notes repo |
| Status       | stable                                             |

## Description

Reference for searching the team notes repo with the `zk` CLI: full-text search,
tag filtering, path filtering, date ranges, link-graph queries, orphan
detection, and output formats. Always uses
`-W ~/fun_repos/hobby-mimisbrunnr/notes`.

Load this skill whenever Claude needs to find notes in the team notebook.

## Installation

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/zk-notes/skill.md \
  ~/.claude-hobby/commands/zk-notes.md
```

Or: `just install-skills`.

## Usage

```
/zk-notes
```

## Known Limitations

- Notes directory path is hard-coded. Fork and edit if your notes live
  elsewhere.
