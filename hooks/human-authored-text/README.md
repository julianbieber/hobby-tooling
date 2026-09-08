# human-authored-text

**Event:** PreToolUse (matcher `Bash`)
**Install:** `~/.claude-hobby/hooks/human-authored-text.sh`

No commit message, PR body, issue or comment reaches git or GitHub unless I typed
it. Enforces norms 1, 2 and 6 of Bevy's AI policy ([bevy-website#2551]).

Only fires inside `~/fun_repos` and `~/fun_worktrees` (`policy_in_scope`).

## Gated commands

| Command | Re-issue with |
|---|---|
| `git commit -m` (not `--amend --no-edit`, `-C`, `--reuse-message`, `--fixup`, `--squash`) | `-F` |
| `git merge -m`, `git tag -a/-m`, `git notes -m` | `-F` |
| `gh pr create`, `gh pr edit --body` | `--title` + `--body-file` |
| `gh pr comment`, `gh pr review` | `--body-file` |
| `gh issue create`, `gh issue edit --body`, `gh issue comment` | `--body-file` |
| `gh release create` | `--notes-file` |
| `gh issue close --comment`, `gh gist create`, `gh api -f body=` | *no file form — denied outright, run it yourself* |
| `git commit` with no message flag | *denied — re-issue with `-m`* |
| any of the above alongside another command | *denied — issue it as its own Bash call* |

## What counts as one of them

Classification reads **argv**, not the command string: `policy_classify_command`
in `githooks/policy-lib.sh` tokenizes the command into simple commands and looks
at `argv[0]` and the subcommand. Consequences:

- A command that only *mentions* a gated one is not gated — `rg 'git commit'`,
  `git log --grep=…`, a heredoc writing notes, an `echo` into a file.
- Spelling does not matter: `git -C <dir> commit`, extra whitespace, and a literal
  `bash -c '…'` all resolve.
- `--no-edit`, `-C` and `--reuse-message` exempt only as flags of that command,
  never as text inside its message.
- A word built by `$(…)`, a variable or `eval` makes only *that* segment
  unresolvable, and unresolvable means allowed.

Not covered: prefix wrappers (`env FOO=1 git commit`, `timeout 30 …`, `xargs`),
git aliases, and non-literal wrappers (`bash -c "$SCRIPT"`).

`just test-hooks` runs `tests/cases.txt` — every row above, in both directions —
against the classifier without opening an editor.

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

Denied before any editor opens: a gated command chained to another; a command with
no `--*-file` form; `git commit` with no message flag.

## TODO(jb-doc): why a chain is refused rather than gated

<!-- What it should cover: the editor could open on the one segment that needs
     it, and the re-issue instruction would then have to reproduce a chain the
     hook did not author. Also what `cd x && git commit` costs now. -->

## TODO(jb-doc): why --fixup and --squash are allowed through

<!-- What it should cover: git builds those messages from an existing commit, so
     no generated text is involved, and both are incompatible with the -F the
     gate would demand — the substring gate opened a buffer that could not be
     used. This is the one exemption that changed. -->

## Adding a gated command

1. A case in `_policy_classify_gh` or `_policy_classify_git` (`githooks/policy-lib.sh`).
2. A row in the table above.
3. Fixtures in `tests/cases.txt`: the gated form, a near-miss that must stay
   allowed, and the `--*-file` re-issue.

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
