# githooks

**Install:** `just install-git-hooks` — sets a **global** `core.hooksPath` to this
directory.

Enforces Bevy's AI policy ([bevy-website#2551]) in git itself.

| Hook | Refuses |
|---|---|
| `commit-msg` | machine attribution in the message; an empty message; the work git identity |
| `pre-push` | attribution or the work identity anywhere in the outgoing range; unfilled comment placeholders, **public remotes only** |

`policy-lib.sh` holds the attribution patterns, the placeholder token
(`TODO(jb-comment)` / `TODO(jb-doc)`) and the expected commit email. It is also
sourced by the Claude-side `hooks/human-authored-text` gate.

Bypass: `git commit --no-verify`, `git push --no-verify`.

## TODO(jb-doc): why this layer exists at all

<!-- What it should cover: why a git hook rather than a CLAUDE.md rule; where the
     Co-Authored-By trailer actually comes from; what "cannot be bypassed" buys
     that the Claude-side gate does not. -->

## TODO(jb-doc): global install, scoped behaviour

<!-- What it should cover: why core.hooksPath is set globally; why the hooks
     no-op outside ~/fun_repos and ~/fun_worktrees; why both hooks chain to a
     repo-local hook of the same name. -->

## TODO(jb-doc): why the placeholder check is conditional on the remote

<!-- What it should cover: why a placeholder is fine to push to my own repo and
     not to someone else's, and what that has to do with whether I keep using
     the hook. -->

[bevy-website#2551]: https://github.com/bevyengine/bevy-website/pull/2551
