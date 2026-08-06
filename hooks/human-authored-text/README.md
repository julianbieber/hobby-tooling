# human-authored-text

**Event:** PreToolUse (matcher `Bash`)
**Install:** `~/.claude-hobby/hooks/human-authored-text.sh`

No commit message, PR body, issue or comment reaches git or GitHub unless I typed
it. Enforces norms 1, 2 and 6 of Bevy's AI policy ([bevy-website#2551]).

Only fires inside `~/fun_repos` and `~/fun_worktrees` (`policy_in_scope`).

## Gated commands

| Command | Re-issue with |
|---|---|
| `git commit` (not `--amend --no-edit`, `-C`, `--reuse-message`) | `-F` |
| `git merge -m`, `git tag -a/-m`, `git notes -m` | `-F` |
| `gh pr create`, `gh pr edit --body` | `--title` + `--body-file` |
| `gh pr comment`, `gh pr review` | `--body-file` |
| `gh issue create`, `gh issue edit --body`, `gh issue comment` | `--body-file` |
| `gh release create` | `--notes-file` |
| `gh issue close --comment`, `gh gist create`, `gh api -f body=` | *no file form — denied outright, run it yourself* |

## The exchange

1. Claude runs a gated command with text it wrote.
2. The hook opens `$EDITOR` in a floating zellij pane on a buffer of headings and
   facts. **Claude's proposed text is deliberately not shown.**
3. I write it. The pane closes.
4. The hook **denies** the call and tells Claude the file to re-issue against.
5. Claude re-runs; the hook sees a registry file whose sha256 still matches and
   allows it.

Rejections: empty buffer; machine attribution; text that is Claude's own, pasted;
for a PR, an `## AI use` section that is missing, empty, or still the stub.

## TODO(jb-doc): why deny-and-re-issue rather than rewriting the command

<!-- What it should cover: updatedInput is available on PreToolUse and was
     verified before this was written, so this is a choice, not a limitation.
     Cover the shell-quoting problem with heredoc bodies, and what the .sha
     registry guarantees that a rewritten command would not. -->

## TODO(jb-doc): why Claude's draft is hidden from the buffer

<!-- What it should cover: norm 6 asks for synthesis from scratch rather than
     revision of generated text, and what showing the draft would do to that. -->

## TODO(jb-doc): what this hook does not and cannot enforce

<!-- What it should cover: norm 1's "understand every line" is not checkable
     here; where that is handled instead. Also the bypass surface — this hook
     only sees Claude's Bash calls, and githooks/ is what covers the rest. -->

## Relationship to `edit-before-execute`

That hook opened an editor on the same two commands from `PermissionRequest`, and
treated an unchanged buffer as approval. It no longer matches `git commit` or
`gh pr create`; it keeps the large-`Write` case only.

[bevy-website#2551]: https://github.com/bevyengine/bevy-website/pull/2551
