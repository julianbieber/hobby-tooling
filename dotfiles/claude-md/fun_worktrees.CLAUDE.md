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
| Done/blocked state | same dir, `task.json` (gitignored — local state) |
| PRs opened | same dir, `prs.json` (written by the track-pr hook) |
| MML specs | same dir, `<unit>.mml` |

Derive topic and task from the path: they are the **second and third** components
under `~/fun_worktrees`, with the project first.

## Commands

```
hwt new <task> -p <project> -t <topic>   create a task worktree + notes dir
hwt go <task>                            cd to it (fuzzy, searches all projects)
hwt tree                                 project -> topic -> task overview
hwt done | block | unblock               task state
hwt rm <task>                            remove the worktree, keep the notes
hwt restore <task>                       re-create a removed worktree
hcdev wt <task>                          zellij session: Helix + Claude (hobby account)
hcdev notes                              cd to this task's notes dir
```

## Account

Sessions started via `hcdev` or `chobby` run on the **private** account
(`CLAUDE_CONFIG_DIR=~/.claude-hobby`). A bare `claude` here would use the **work**
account — check `echo $CLAUDE_CONFIG_DIR` if unsure.
