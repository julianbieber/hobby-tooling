# task-context

**Event:** SessionStart (`startup|resume|clear`)
**Install:** `~/.claude-hobby/hooks/task-context.sh`

Injects the current hobby task identity into the session: project / topic / task,
the main checkout path, `task.json` done/blocked state, the notes directory, the
latest journal date, and an HMML spec count.

Silent (exit 0, no output) unless cwd is under
`~/fun_worktrees/<project>/<topic>/<task>`.

## Why it is short

The work counterpart (`julian-task-projects`) had to enumerate the repo worktrees
inside a task directory, because a work task dir is a container and has no branch
or git status of its own. A hobby task dir **is** the worktree, so Claude gets
branch and gitStatus natively and only the task metadata is worth injecting.
