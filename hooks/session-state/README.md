# session-state

**Events:** SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, Stop, Notification,
SessionEnd (empty matcher on each)
**Install:** `~/.claude-hobby/hooks/session-state.sh` (`just install-hooks`), registered in
`settings/hobby.settings.json`

Records whether each hobby-account Claude session is **busy** (mid-turn) or **idle**
(waiting on you), and since when, in one small JSON file per session. `hdash` reads them to
put an `idle 12m` / `busy 3m` badge on each card, count the sessions waiting on you, and sort
an idle task first.

| Event | State |
| --- | --- |
| `SessionStart` (source `startup`, `resume`, `clear`) | idle |
| `SessionStart` (source `compact`) | unchanged |
| `UserPromptSubmit`, `PreToolUse`, `PostToolUse` | busy |
| `Stop` | idle |
| `Notification` (`permission_prompt`, `idle_prompt`, `elicitation_dialog`) | idle |
| `Notification` (any other type) | ignored |
| `SessionEnd` | file deleted |

Inside zellij it also marks the tab the session runs in: the tab name gets a leading `● ` while
the session is idle and `○ ` while it is busy, and loses it at `SessionEnd`. The rename runs in
the background after the state file is written, under a per-pane `flock` in `tabs/`, and reads
the state back from the file, so the last event to finish sets the marker whatever order they
ran in. Anything that looks a tab up by name strips the marker first (`hcdev-find-tab`,
`zj-notes`), and `htab` keeps it when it renames its tab.

An unchanged state only touches the file (its mtime is the time of the last event). A state
change is written under a per-session `flock` to a temp file and renamed into place. The hook
always exits 0 and never blocks a turn.

## File format

`${XDG_RUNTIME_DIR:-$HOME/.cache}/hobby-session-state/<session_id>.json`, the dir created
`0700`. `session_id` must match `^[A-Za-z0-9-]{1,64}$`, else the hook does nothing.

```json
{
  "version": 1,
  "state": "busy",
  "since": 1791374400,
  "project_dir": "/home/me/fun_worktrees/project/topic/task",
  "zellij_session": "hcdev-project-topic-task",
  "zellij_pane": "1",
  "pid": 181478,
  "pid_start": 52811937
}
```

| Field | Meaning |
| --- | --- |
| `state` | `busy` or `idle` |
| `since` | epoch seconds of the last busy/idle transition |
| `project_dir` | `$CLAUDE_PROJECT_DIR`, else the event's `cwd` |
| `zellij_session` | `$ZELLIJ_SESSION_NAME` (empty outside zellij) — how `hdash` finds a pane whose directory is a main checkout |
| `zellij_pane` | `$ZELLIJ_PANE_ID` (empty outside zellij) |
| `pid` | the `claude` process (the first ancestor of the hook whose comm is `claude`; 0 if none) |
| `pid_start` | that process's start time (field 22 of `/proc/<pid>/stat`), so a reused pid is detectable |

Ported from the work `julian-session-state` hook. Two differences: the directory is
`hobby-session-state`, not the work `claude-session-state`, so neither account's dashboard
reads or deletes the other's files; and there is no `main` field — every Claude process
counts, whatever pane started it.

## Test

`./test.sh` runs the hook against fixture payloads in a throwaway dir (needs `jaq`).

## Known limitations

- An interrupted turn (Esc) may fire no `Stop`, leaving the file at `busy`. `hdash` corrects
  for it with a CPU-time heartbeat on `pid`.
- `/clear` and resume start a new session id without a `SessionEnd` for the old one, and a
  killed session never sends one. `hdash` deletes a file once its `pid` is gone, and keeps
  the newest live file per task.
- Linux only: the `pid`/`pid_start` lookup reads `/proc`.
