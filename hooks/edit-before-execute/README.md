# edit-before-execute

**Event:** PermissionRequest (matcher `Write`)
**Install:** `~/.claude-hobby/hooks/edit-before-execute.sh`

Lets a large `Write` be edited at the permission prompt instead of being approved
or rejected as-is.

Was a verbatim copy of the work hook. The `Bash` trigger (`git commit`,
`gh pr create`) was moved out to the `human-authored-text` gate, which has since
been deleted — so the `Bash` branch now exits 0 and nothing edits those commands.
Whether it comes back is part of the commenting/commit/PR rework.

## TODO(jb-doc): why the Bash half moved out

<!-- What it should cover: PermissionRequest fires only when a prompt would have
     appeared, so an allow-rule silently disabled it; and an unchanged buffer
     counting as approval is the opposite of what a policy gate needs. -->

