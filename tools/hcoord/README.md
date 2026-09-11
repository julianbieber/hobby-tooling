# hcoord

| Field        | Value                                                        |
| ------------ | ------------------------------------------------------------ |
| Type         | terminal app (ratatui) + CLI                                 |
| Binary       | `hcoord`                                                     |
| Owner        | @julianbieber                                                |
| Dependencies | none at runtime; `hcdev coordinator` and `/task:coordinate` call it |
| Status       | experimental                                                 |

## Description

What a coordinator session needs from outside its Claude pane.

```
hcoord tui <coordinator notes dir>                         the left pane of `hcdev coordinator`
hcoord wait <task notes dir> [--timeout <s>] [--after <phase>]
                                                           block until a task needs a person or is done
hcoord status <coordinator notes dir>                      one line of counts
```

## `tui` shows `tasks.json` and nothing else

The file lives in `~/fun_repos/hobby-mimisbrunnr/notes/<project>/coordinator/<blurb>/`
and is written by `/task:coordinate`. The pane re-reads it whenever its mtime
changes, so the list updates as the coordinator works without either side
talking to the other. It is designed for a 15% split: one line per task — status
glyph, number, title — with the coordinator's note and PR number indented
beneath, and the counts at the top. `d` toggles the description, `j`/`k`
scroll, `r` forces a reload, `q` quits.

```json
{
  "project": "wusel",
  "blurb": "food-economy",
  "description": "verbatim, what the coordinator was started with",
  "created": "2026-09-11",
  "tasks": [
    {
      "task": "food-economy-fields",
      "topic": "food-economy",
      "title": "Fields grow wheat over the seasons",
      "done_when": "start a game, place a field, watch it fill by autumn",
      "depends_on": [],
      "status": "todo | running | awaiting-human | done | blocked",
      "pr": "https://github.com/…/pull/12",
      "note": "one line, optional",
      "updated": "2026-09-11T10:00:00Z"
    }
  ]
}
```

Only `task` and `title` are required; `status` defaults to `todo`.

## `wait` is how the coordinator spends its time

A coordinator is a Claude session, and a Claude session polling a file from a
prompt is a prompt per poll. `hcoord wait` turns that into one shell call: it
watches the task's `build-task.json` — the state file `/task:auto` writes at every
transition — and returns when the phase is one a person has to answer
(`await-*`, `context`, `assess-split`) or terminal (`done`, `handed-off`). The
default timeout is 570 seconds, under the ten minutes Claude Code allows a
shell call, and a timeout is exit 2 so the coordinator simply calls again.

`--after <phase>` is for the second call on the same gate: having reported
`await-play` once, the coordinator wants to block until the phase is something
*else*, not be told `await-play` again immediately.

The output is one JSON line, `{"phase": "await-play", "outcome": "human"}`, so
the coordinator reads a field rather than parsing prose.

## Not part of the hdash workspace

`hdash` is a Bevy build; this is a few hundred lines over ratatui. Sharing a
workspace would put the Bevy dependency graph and its optimised-dependency
profile in front of a tool that builds in ten seconds on its own. `hdash` reads
the `status` field of `tasks.json` with `serde_json::Value` rather than
depending on this crate, for the same reason.
