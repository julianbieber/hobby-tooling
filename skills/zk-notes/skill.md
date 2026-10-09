---
description: Reference for searching notes with zk CLI in ~/fun_repos/hobby-mimisbrunnr/notes
allowed-tools: Bash(zk:*)
---

# zk Notes Search Guide

Search and browse notes in `~/fun_repos/hobby-mimisbrunnr/notes`
using the `zk` CLI. Always run from within
`~/fun_repos/hobby-mimisbrunnr/notes` using
`-W ~/fun_repos/hobby-mimisbrunnr/notes`.

## Full-Text Search

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -m "search terms"
```

Match strategy can be changed with `-M`:

- `fts` (default) — full-text search
- `re` — regex
- `exact` — exact string match

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -m "pattern.*match" -M re
```

## Filter by Tag

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -t tagname
```

Multiple tags (AND):

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -t tag1 -t tag2
```

### List All Tags

```bash
zk tag list -W ~/fun_repos/hobby-mimisbrunnr/notes
```

## Filter by Path

Restrict search to a subdirectory — notes are laid out as
`<project>/<topic>/<task>/`:

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes wusel
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes wusel/world_gen
```

Combine path with search:

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -m "chunk" wusel/world_gen
```

## Filter by Date

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes --created-after "last week"
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes --modified-after "2 days ago"
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes --created-after "2026-01-01" --created-before "2026-02-01"
```

## Output Formats

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -m "query" -f short    # title + path
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -m "query" -f medium   # title + metadata
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -m "query" -f long     # title + body excerpt
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -m "query" -f full     # full note content
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -m "query" -f json     # JSON output
```

## Limit Results

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -m "query" -n 10
```

## Sort Results

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -m "query" -s modified-    # newest modified first
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -m "query" -s created-     # newest created first
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -m "query" -s title+       # alphabetical by title
```

## Link Graph Queries

Find notes linked by a specific note:

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes --linked-by project/topic/task/YYYY-MM-DD.md
```

Find notes linking to a specific note:

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes --link-to project/topic/task/implementation.md
```

Find related notes:

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes --related project/topic/task/implementation.md
```

Find orphan notes (not linked by anything):

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes --orphan
```

## Combining Filters

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -m "chunk" -t plan --created-after "last month" -s modified- -n 20
```

## Reading a Note

After finding a note with `zk list`, read its content:

```bash
zk list -W ~/fun_repos/hobby-mimisbrunnr/notes -m "query" -f full -n 1
```

Or use the Read tool with the file path from list results.
