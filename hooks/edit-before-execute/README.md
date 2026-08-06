# edit-before-execute

**Event:** PermissionRequest (matcher `Write`)
**Install:** `~/.claude-hobby/hooks/edit-before-execute.sh`

Lets a large `Write` be edited at the permission prompt instead of being approved
or rejected as-is.

Was a verbatim copy of the work hook. The `Bash` trigger (`git commit`,
`gh pr create`) is gone — those commands go through
[`human-authored-text`](../human-authored-text/README.md) now.

## TODO(jb-doc): why the Bash half moved out

<!-- What it should cover: PermissionRequest fires only when a prompt would have
     appeared, so an allow-rule silently disabled it; and an unchanged buffer
     counting as approval is the opposite of what a policy gate needs. -->

