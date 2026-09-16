# ~/fun_worktrees — Hobby Task Worktrees

## Structure

```
~/fun_worktrees/<project>/<topic>/<task>/
```

Three levels. The **task directory is the git worktree itself** — hobby tasks are
single-repo, so there is no repo level underneath. The branch is named after the
task, and the main checkout it derives from is `~/fun_repos/<project>`.

```
~/fun_worktrees/
  bevy_survivor/
    features/
      enemy-waves/        <- a git worktree, branch `enemy-waves`
    bugs/
      spawn-crash/
  wusel/
    perf/
      chunk-loading/
```

## Where Things Live

| | |
|---|---|
| Main checkout | `~/fun_repos/<project>` |
| Notes for this task | `~/fun_repos/hobby-mimisbrunnr/notes/<project>/<topic>/<task>/` |
| Journal entries | same dir, `YYYY-MM-DD.md` |
| Done/blocked state | same dir, `task.json` (tracked — `{ done, blocked }`, nothing else) |
| PRs opened | same dir, `prs.json` (written by the track-pr hook) |
| HMML specs | same dir, `<unit>.hmml` |
| Business context | same dir, `business-context.md` (my own words, recorded verbatim) |

Derive topic and task from the path: they are the **second and third** components
under `~/fun_worktrees`, with the project first.

**`task.json` is what makes a task exist**, not the worktree directory. `hwt` and `hdash`
both enumerate the notes tree; `~/fun_worktrees` only answers whether a task is currently
checked out. A worktree with no `task.json` behind it is an *orphan* — `hwt orphans` lists
them, and `hdash` offers **Adopt**.

The notes dir outlives the worktree: `hwt rm` is routine, and the notes are committed
and shared across machines while the worktree is scratch on one disk. A record that
vanished with the checkout would lose the task every time.

## Commands

```
hwt new <task> -p <project> -t <topic>   create a task worktree + notes dir
hwt go <task>                            cd to it (fuzzy, searches all projects)
hwt tree                                 project -> topic -> task overview
hwt done | block | unblock               task state
hwt orphans                              worktrees with no task.json
hwt rm <task>                            remove the worktree, keep the notes
hwt restore <task>                       re-create a removed worktree
hcdev wt <task>                          zellij session: Helix + Claude (hobby account)
hcdev auto <task> [-w]                   the same, with Claude running /task:auto (plan, then build)
hcdev coordinator <blurb>                from ~/fun_repos/<project>: task list + Claude
hcdev close <task> [-p] [-t]             kill its session, stop its processes, close its window
hcdev notes                              cd to this task's notes dir
```

A task under the topic `coordinator` is a **coordinator**: notes, `task.json`
and a `tasks.json`, no worktree. `hcdev coordinator <blurb>` opens it;
`/task:coordinate` in that session works the tasks it lists, one `hcdev auto`
window at a time.

## Account

Sessions started via `hcdev` or `chobby` run on the **private** account
(`CLAUDE_CONFIG_DIR=~/.claude-hobby`). A bare `claude` here would use the **work**
account — check `echo $CLAUDE_CONFIG_DIR` if unsure.
