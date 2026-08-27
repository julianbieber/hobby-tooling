# Hobby Tooling

ai-policy: private

## What This Repo Is

Personal Claude Code tooling for hobby projects, running under a **separate Claude
account** from work. The private-account counterpart to
`team-constellation-tooling`. Adapted from that repo's `julian-*` tools, but
restructured for single-repo tasks.

## The Structural Difference That Explains Everything

At work, `~/worktrees/TOPIC/TASK/` is a **container** holding several repo
worktrees, because a work task often spans Patagona-API + request-worker + infra.

Here, `~/fun_worktrees/PROJECT/TOPIC/TASK/` **is** the worktree. Hobby tasks are
single-repo, so the project moves to the front of the path and the repo level
disappears.

Consequences, all of which make this tooling smaller than its work counterpart:

- No `wt add` — a task can't gain a second repo
- `task.json` is just `{ done, blocked }`; no `repos[]`, since the project is in the path
- `hwt go <task>` takes no project argument
- A task directory is itself a git repo, so Claude gets branch + git status for free
- No experts (one repo means one expert, and a third path level strains the
  108-byte Unix socket limit)

## Layout

```
dotfiles/
  nushell/{hwt.nu, hcdev.nu}          # symlinked into ~/.config/nushell/
  zellij/{config.kdl, layouts/hobby-dev.kdl}   # symlinked into ~/.config/zellij/
  claude-md/                          # symlinked into ~/.claude-hobby/ and ~/fun_worktrees/
bin/{chobby, hx-claude}               # symlinked into ~/.local/bin/
hooks/<name>/hook.sh                  # symlinked into ~/.claude-hobby/hooks/
skills/<name>/{skill.md, README.md}   # symlinked into ~/.claude-hobby/commands/
settings/hobby.settings.json          # symlinked to ~/.claude-hobby/settings.json
scripts/                              # install logic too involved for a justfile recipe
lsps/hobby-mml-lsp/                   # forked HMML language server + grammar.md
lsps/{claude,claude-skills,treesitter-class,path}-lsp/   # vendored from the work checkout
mcps/{just-mcp, lsp-mcp, tree-extract}/   # vendored from the work checkout
tools/hdash/                          # the work dashboard (Bevy)
```

## `topic:<name>` is how an issue names its worktree

`hdash` lists open GitHub issues and turns one into a dev session with a
click. The whole thing rests on a label. A worktree lives at
`~/fun_worktrees/PROJECT/TOPIC/TASK`, and an issue already implies two of those
three — the **project** is the repo it was filed in, the **task** is
`gh-<number>-<slug>` from its number and title. **Topic is the one component
nothing else supplies**, so the issue carries it as `topic:features`.

Prefixed, with no fallback to bare labels. Every hobby repo already has `bug`,
`enhancement`, `good first issue` from GitHub's defaults, and `wusel` labels its
issues `world_gen` / `world_sim` / `persistence` — all of which read like topics
and none of which are. Requiring the prefix keeps the tag an explicit statement
rather than a guess, and lets an issue name a topic whose directory does not exist
yet, which is the normal case: a topic dir only appears with the first worktree
under it. The value must be directory-shaped (`[A-Za-z0-9._-]+`, no leading dot);
anything else is treated as untagged rather than escaped, because it becomes both
a path component and a shell argument.

An untagged issue is still listed — it is the one that most needs attention — but
it has no launch button, only a line naming the missing label. `/gh-issue-create`
writes the label (creating it in the repo when absent), so issues opened through
the skill arrive launchable.

Existing worktrees are matched to issues **by number, not by task name**:
`gh-42-*`, `gh42-*`, `issue-42_*` and bare `gh-42` all belong to issue 42. Task
names predating the convention are spelled differently (`issue-9_dotfile_repo_retire`)
and a slug changes whenever the title is edited, so the derived name is what a
*new* task gets, never what a lookup relies on. When such a worktree exists, its
path — not the label — supplies the topic for `hcdev wt`, since the path is what
`hcdev` has to find.

## The notes repo is what makes a task exist

A task exists because `notes/<project>/<topic>/<task>/task.json` exists. Not because a
directory under `~/fun_worktrees` does — that tree answers one question only, *is it
checked out*, and is never enumerated to find out what work there is.

| | source | answers |
|---|---|---|
| a task exists | `notes/P/T/TASK/task.json` | which tasks there are |
| a task is checked out | `~/fun_worktrees/P/T/TASK` | Active vs Removed |
| done / blocked | `task.json` | state, and nothing on GitHub |
| what the work *is* | the GitHub issue | title, labels, author, `topic:` |

`hdash` and `hwt` apply the same rule from the same file — `scan_tasks` reads the notes
tree, `known-tasks` reads the notes tree — so `hwt list | length` and the dashboard's task
count agree. They did not before: `hwt` treated the *directory* as the marker and `hdash`
the `task.json`, which is why `hwt forget` (whose whole job is removing that file) dropped
a task from one tool and not the other.

A worktree with no `task.json` behind it is an **orphan**: on disk, recorded nowhere.
`hwt orphans` lists them; `hdash` gives each one a card at the top of the list with an
**Adopt** button that writes the file. Nothing adopts silently — a scan never writes.

Two things this rule deliberately does **not** buy:

- **Issue-less tasks stay first-class.** `hwt new` does not touch GitHub, and a notes dir
  alone is a complete task. GitHub is the source of truth for the tasks that have an issue.
- **`done` is local.** `task.json` is the only authority; closing an issue does not mark
  its task done and marking a task done does not close its issue. `hdash` asks `gh` for
  open issues only, so a closed issue's task shows unjoined.

The notes repo decides because it is the only one of the two that survives everything.
A worktree is deleted the moment the work pauses — `hwt rm` is routine — so a tree that
recorded existence would lose a task every time one was checked out somewhere else, or
on a second machine. The notes dir is committed and shared; the worktree is scratch.
Enumerating the worktree tree would also answer a different question than the one asked:
it lists what is on this disk, not what work there is.

`done` stayed local because the two flags do not mean the same thing. An issue closes
when the change ships; a task is done when I stop working on it, which happens on
abandonment and on splitting the work in two as well as on shipping. Mirroring would
either close issues that are still open questions or reopen tasks that are finished
here, and it would put a network call behind a keystroke that currently writes one
local file. `hdash` asks `gh` for open issues only, so a closed issue's task simply
shows unjoined — which is the honest rendering of the two being independent.

## Two machines: what is actually at risk, and `hwt sync`

Work moves between two machines, so anything a remote has never seen exists on one
disk. `hwt status` did not answer that question — it reads `hwt list`, which is
task-driven, so it sees only the worktrees.

**The worktrees were never the leak.** 34 of 37 are clean, with an upstream, zero
commits ahead: the PR flow pushes them as a side effect of how a task is worked.
Every unsynced byte was in the **main checkouts** under `~/fun_repos`, which no
command enumerated. `hwt sync` scans all three populations — main checkouts, task
worktrees, and orphans — because the last two are cheap to include once the walk
exists, and an orphan is by definition recorded nowhere else.

Four states, and only one of them is a sync problem:

| state | means | fix |
|---|---|---|
| `unsynced` | commits no remote has, or a dirty tree | `--push` / `--wip` |
| `no-remote` | commits, but nowhere to push them | `gh repo create` |
| `unborn` | zero commits; every file untracked | a first commit |
| `skipped` | `hobby.sync=false` in that repo | — |

`unborn` is its own state because it is not a degree of `unsynced`: five repos here
were `git init`'d and never committed, so `.gitignore` and `Cargo.toml` are
untracked alongside `src/`. Reporting that as "12 dirty files" would have buried it.

The opt-out is `git config hobby.sync false`, in the repo's own `.git/config`
rather than a marker file — a marker file in the working tree would itself show up
as untracked, i.e. as the thing it is meant to silence. It does not travel between
machines, which is correct: whether a scratch checkout exists at all is a fact
about one disk.

### A snapshot is a ref, not a branch, and not a stash

`--wip` writes the working tree — tracked edits and untracked files alike, minus
what `.gitignore` excludes — to a commit reachable only from
`refs/wip/<machine>/<branch>`, then pushes that ref.

**HEAD, the index and the working tree are untouched.** The snapshot is assembled
in a throwaway index (`GIT_INDEX_FILE` → `read-tree` → `add --all` → `write-tree`
→ `commit-tree`), so it can run over a checkout that is mid-edit without
disturbing it. That is what makes it safe to run without reading the diff first,
and it is why the target is a ref rather than a branch: committing on a branch
means checking that branch out.

The ref lives outside `refs/heads/`, so a snapshot never appears as a branch in the
GitHub UI and no PR can be opened from one by accident. GitHub accepts the push —
verified, not assumed.

`<machine>` is `$HOBBY_MACHINE`, falling back to the short hostname. It is in the
ref name so that snapshotting the same branch on both machines produces two refs
rather than one machine silently overwriting the other's in-flight work.

**Stashes are deliberately out of scope.** They cannot be pushed at all, so no
amount of tooling reaches them, and `CLAUDE.md` already treats the shared stash
stack as a hazard rather than a place to keep things.

### The message is a pointer, not a summary

A wip commit message is a file count, the branch, the machine, and — for a task
worktree — the path to that task's notes directory. It never describes the change.
The notes are synced too and are where the prose lives, so the message points at
them instead of paraphrasing them; and under the AI policy a factual list is
writable where a summary is not.

### The other direction: `--pull` fast-forwards, and nothing else

Arriving at a machine is the other half, and it was covered worse than leaving.
`hwt refresh` exists but pulls `main`/`master` on **main checkouts only** — not the
branch you were actually on, not any worktree, and it never sees a snapshot. So
`hwt sync` on a freshly-opened machine could report everything clean while the
other machine's work sat on the remote, unmentioned.

`--pull` fetches every checkout that has a remote (in parallel — it is the one
network-bound phase), then **fast-forwards, and only fast-forwards**. A merge or a
rebase can conflict; fast-forward is the single move that cannot lose work, so
everything else is printed rather than attempted:

```
  wusel                  main       ff  a1b2c3d..e4f5a6b (3)
  sdf-flood              sunset     skipped — behind 4, dirty (2 files)
  hackathon_game         main       skipped — diverged (2 local / 5 remote)
```

It runs over **every** checkout with a remote, not just the ones the report calls
`unsynced`: that column measures what this disk holds and no remote has, which
says nothing about the reverse direction.

Snapshots fetch into `refs/remote-wip/*`, deliberately not `refs/wip/*`. The second
namespace is *this* machine's own snapshots, and a forced fetch into it would rewind
one taken but not yet pushed — the same split git already makes between `refs/heads`
and `refs/remotes`. Snapshots whose `<machine>` component is this machine are then
filtered out of the "waiting" list, or every push would be reported back as incoming
work. What remains is listed newest first, with the diff command:

```
Wip snapshots waiting from another machine:
  zig_survivor  refs/remote-wip/desktop/laptop  2 hours ago
    git -C ~/fun_repos/zig_survivor diff HEAD refs/remote-wip/desktop/laptop
```

Applying one is left to you. It writes files you have not read yet, which is a
different kind of act from fast-forwarding a clean branch.

### Reports by default, acts only when asked

`--push` publishes commits, `--wip` publishes the working tree as it stands, and
`--pull` moves branches. None can be un-published, and one of these repos is public,
so the bare command prints and changes nothing. The report ends with the exact
command for each state it cannot act on itself.

### The identity leak this surfaced

Scanning the same tree turned up ~600 commits across 29 repos authored as the work
email, 11 of those repos public. The cause was structural: the **global** git
identity is the work one, and `just git-identity` set the hobby identity on exactly
two repos — this one and the notes repo. Every other clone inherited work identity
and nothing ever said so.

The fix is an `includeIf` on `gitdir:~/fun_repos/` and `gitdir:~/fun_worktrees/`
pointing at `~/.gitconfig-hobby`, so a repo is born with the right identity because
of **where it lands** rather than because a recipe was re-run after cloning.
Per-repo local config is still written on top, so a checkout moved out from under
those paths keeps the identity. `just doctor` now checks every repo under
`~/fun_repos` instead of two.

History was not rewritten: that means a force-push across public repos, and it is
a separate decision from stopping the bleeding.

## One dashboard, because there was only ever one list

`tools/hdash` replaces `hwt-overview` (the worktrees that exist) and `hgh-issues`
(the issues that have none yet). They were the same app over two halves of one
list: `hgh-issues` walked `~/fun_worktrees` to decide whether an issue had a
worktree, `hwt-overview` drew that worktree with no idea an issue existed, and
~370 lines of chrome, sidebar, channel plumbing and card recipe were duplicated
per app.

**The join is the whole point.** An issue's worktree is named `gh-<number>-<slug>`,
so matching on the *number* pairs the two halves — and `matches_issue` accepts the
pre-convention spellings (`issue-9_dotfile_repo_retire`) that a name-based match
would miss. One card then carries a piece of work from the issue through the
worktree and its PRs to the day it is marked done, with the primary button always
the next step: **Start task** → **Open hcdev** → (**Restore** if the worktree was
removed). The filesystem walk `find_worktree` did is gone: the task half already
holds that tree, so the match is pure and in-memory.

Four consequences worth knowing:

- **An orphan outranks everything.** A worktree the notes repo has never heard of sorts
  above even a lifted block, and no content toggle can hide it.
- **Topic is no longer a grouping level.** An unstarted issue has a topic *label*
  and no topic *directory*, so there is nothing to group it under. The list is
  flat and globally sorted by how much attention an item wants; project and topic
  are printed on the card.
- **Both content toggles are load-bearing defaults.** `show_done` off (there are
  ~25× more done tasks than open ones) and `show_untagged` on (an untagged issue
  is the one item the app cannot act on, so it is the one that most needs to be
  seen).
- **`~/fun_repos` holds upstream clones**, not just my projects — a `bevy` or `helix`
  checkout has 200 open issues that are nobody's work here. `hdash` therefore asks `gh`
  for the **selected** project and no other, which makes "All" a task list: an issue-only
  card there would report whichever projects happened to be visited this session. There is
  still no denylist or ownership filter; the selection is the gate. The sidebar leads with
  the projects that have a notes directory, and every project keeps a row, because a row is
  now the only way to reach a project's issues.

`feathers_kit` stays a deliberate fork of the work copy so the *work* dashboard
cannot regress, and this is the repo's only copy. With one workspace there is no
longer a cross-workspace path dependency, so `cargo fmt`/`clippy` no longer reach
sideways and editing `hwt_core` needs one workspace's tests, not two.

**Installing never needs tests** — the committed code is already tested, so
`install-dash` builds and replaces the binary and nothing more.

## Opening a session from a dashboard: two things that kill the terminal

Both failures looked identical from the outside — the click opened an alacritty
window that vanished before it drew a frame — and both are worth knowing, because
either one silently comes back the moment a session name is spelled somewhere new.

**A session name has to fit the socket path.** A session's IPC socket is
`<runtime-dir>/zellij/contract_version_1/<session>`, and that path has to fit
`sockaddr_un.sun_path` — 108 bytes on Linux. With the socket dir at 40 characters
zellij rejects any name over 66, and `hcdev-<project>-<topic>-<task>` reaches 67 for
an issue-derived task like `gh-6-simulate-food-production-and-city-growth`. So
`hcdev-session-name` is the **only** place a session name is spelled: it measures the
budget from the real socket dir, and when the three components overflow it, truncates
and appends six hex characters of the full name's sha256. The hash is not decoration —
plain truncation would land two long task names sharing a prefix on one session, and
`hcdev wt A` would drop you into B's editor and Claude pane. `hwt sessions` imports the
function rather than deriving a second copy of the rule. `hdash` still spells the
*window* title the long way, which is fine: a title has no length limit, and it is not
what zellij is asked for.

**A launched terminal has to leave zellij behind.** Run from inside a zellij pane, a
dashboard inherits `ZELLIJ`, `ZELLIJ_SESSION_NAME` and `ZELLIJ_PANE_ID`, and `setsid`
does not undo that — the terminal it opens believes it is already inside that session.
For the task you are *currently* in, `hcdev` then asks zellij to attach a session to
itself, which zellij answers with a **panic**. `hwt_core::spawn_detached` therefore
removes the three variables from every process it starts, and `hwt sessions` does the
same with `with-env`. `hcdev-launch` also refuses the self-attach in so many words, for
the case nothing spawned: a `hcdev wt <this task>` typed by hand.

## HMML is a Bevy dialect here

`lsps/hobby-mml-lsp/grammar.md` is the **source of truth** for HMML (v2) and
what the `hmml-*` skills read. The work `01-language-spec/grammar.md` describes v1
and governs nothing in this repo.

Hobby specs are **`.hmml`**, never `.mml`. The extension is what keeps the two
dialects apart: Helix routes `.hmml` to `hobby-mml-lsp` by file type, and work
`.mml` to `mml-lsp`, with no path globs or block-ordering tricks involved.
`mml_parser::SPEC_EXT` is the single definition — every place that builds or
filters a spec filename goes through it, so the extension stays a one-line
change.

The **binaries keep their `hobby-mml-*` names** (`hobby-mml-lsp`,
`hobby-mml-parser`), as do the crate directories. That is
deliberate, not an oversight: they never collided with the work `mml-lsp` /
`mml-parser` on `PATH` in the first place, so renaming them would
buy nothing and cost a `cargo uninstall` dance plus stale `~/.cargo/bin`
entries. The language is HMML; the programs that read it are `hobby-mml-*`.

The fork exists because hobby projects are Bevy games and MML v1 assumes a unit
is a function (`in: T` / `out: T`). An ECS has no such contract — data flows
through the World — so v2 adds `kind: plugin` / `kind: system` and the sections
`## components`, `## resources`, `## events`, `## systems`, `## access`, with the
legal section set a function of `kind:`. A system's interface is its **access
block** (`schedule:`, `query:`, `reads:`, `writes:`, `spawns:`, `emits:`, …), and
every name in it is role-checked against the declared components / resources /
events. That check is what replaces the `in:`/`out:` type check and what gives
the LSP goto-definition, "who writes this component?", and the index's schedule
map.

`## data` is the one section **both** dialects take, and v2.1 is what made that
true. A Bevy feature is full of types that are not components, resources, or
events — a field type, an enum a component is built from — and there was nowhere
to put them: a world section would have claimed a role the type does not have,
and omitting them made every field naming one an `undefined-type` error. So a
Bevy kind may declare `## data`, with `in:`/`out:` and `marker` still illegal
there, and a `## data` name in an access list an `access-role-mismatch`. The
section is a type namespace in this dialect, not an interface — role is what
makes an access list checkable, and that is exactly what `## data` withholds.

`endpoint` / `workflow` / `job` keep the v1 grammar unchanged, so the handful of
non-Bevy hobby repos (`mosaik_map`, `securitd`, `glsllsp`) still have a usable
dialect. The two cannot be mixed in one file.

## The vendored language servers

`lsps/` holds five servers. `hobby-mml-lsp` is a fork with its own dialect (above);
the other four — `claude-lsp`, `claude-skills-lsp`, `treesitter-class-lsp`,
`path-lsp` — are **copies** of crates that also exist in the work tooling checkout,
vendored so a hobby-only machine can build them at all.

Three consequences worth knowing before touching them:

- **The binary names are unchanged** (`claude-lsp`, `claude-skills-lsp`,
  `treesitter-class-lsp`, `path_lsp`), unlike the deliberately-renamed `hobby-mml-*`.
  There is one Helix and one `languages.toml` shared by both accounts, so a
  `hobby-`-prefixed second name would not isolate anything — it would just register a
  second server that runs on every buffer alongside the first. One binary is correct.
- **Building from here replaces the work build** in `~/.cargo/bin`, and vice versa.
  The two copies will drift; this one is the copy that has the hobby fix.
- **The hobby fix is `CLAUDE_CONFIG_DIR`.** `claude-skills-lsp` used to anchor its
  user tier on `$HOME/.claude`, which in a hobby session offered the work skills and
  none of the 18 hobby ones. It now resolves `$CLAUDE_CONFIG_DIR` first, falling back
  to `$HOME/.claude`, so one binary serves both accounts — `hcdev` exports the
  variable before launching zellij, so the Helix pane inherits it. Keep that property
  if you re-sync from the work copy.

`treesitter-class-lsp` needs no hobby change: its `~/worktrees/TOPIC/TASK` widening
does not match `fun_worktrees`, and falling back to the client root is already the
right scope when the task directory *is* the repo.

Nothing in `lsps/` may name a work repo, worktree, or ticket — the copies were
scrubbed, and a re-sync has to be scrubbed again.

## The vendored MCP servers

`mcps/` holds copies of the work tooling repo's general-purpose MCP servers —
`just-mcp`, `lsp-mcp`, `tree-extract` — vendored so `just install-mcps` builds on a
hobby-only machine instead of skipping all three.

**They are `hobby-`-prefixed, and this is where they diverge from `lsps/`.** The two
accounts run separate builds that coexist in `~/.cargo/bin`:

| | chobby (`~/.claude-hobby`) | claude (`~/.claude`) |
|---|---|---|
| just | `hobby-just-mcp` | `just_mcp` |
| lsp | `hobby-lsp-mcp` | `lsp-mcp` |
| tree-extract | `hobby-tree-extract-mcp` | `julian-tree-extract-mcp` |

The argument for keeping upstream names in `lsps/` does not carry over: there is one
Helix and one `languages.toml`, so a second LSP name would run a second server on
every buffer — but each account has its **own MCP registry**, so a second name
isolates instead of duplicating. Nothing is shared to collide over.

The prefix goes all the way down — package, lib and binary — and the **package** name
is the load-bearing part. Cargo tracks installs by package name in
`~/.cargo/.crates2.json`, so a hobby build sharing the work package name would evict
the work binaries on every `cargo install`, which is exactly what this is meant to
prevent. Renaming only the binary would not be enough.

Consequences: the two copies drift independently (neither overwrites the other), a
re-sync has to be re-scrubbed *and* re-prefixed, and `hobby-tree-extract` writes its
own name into the `generator:` field of `.project-description.md` and the
`.git/info/exclude` comment. That last one is safe because the exclude dedup keys on
the path, not the comment.

Only the *general-purpose* three are here, and the omissions are deliberate rather
than pending work:

- `db-mcp` and `jira-mcp` speak to work systems. Hobby projects have no databases,
  and issue tracking is GitHub via `gh`.
- `experts-mcp` assumes a repo level *below* the task, which this layout does not
  have — the same reason `CLAUDE.md` gives for there being no experts at all.
- `constellation-mcp` is not mine and is work-specific besides.

`tree-extract` is the one crate with a path dependency: it reuses the Helix language
registry and grammar cache from `treesitter-class-lsp`, repointed at `lsps/` here.
Vendoring one without the other does not build.

The *server* names stay unprefixed (`just-mcp`, `lsp-mcp`, `tree-extract`) even
though the binaries do not, so a skill or prompt calling `mcp__tree-extract__*` works
on either account. Only the command behind the name differs.

Registration is `-s user` scope, not the default `local` — `local` binds a server to
the single directory it was added from, so every hobby project outside that one
directory would see no MCPs. `install-mcps` removes before adding, so re-running it
repoints an entry that still names a work binary instead of skipping it as already
registered.

## The AI policy is enforced in four layers — two of them currently switched off

Bevy's revised AI policy ([bevy-website#2551]) applies to **all** work on this
account, not only Bevy contributions. Four layers, ordered by how hard they are to
bypass.

| Layer | Where | Enforces | State |
|---|---|---|---|
| 1 | `githooks/` — global `core.hooksPath` | no machine attribution, no work identity, no placeholders on an upstream-tier push | **off** |
| 2 | `hooks/human-authored-text/` — `PreToolUse` | I write every message, body and comment; a PR carries a filled `## AI use` section | **off** |
| 3 | `skills/pr-facts`, `ai-use-facts`, `explain-back`, `comment-review`, `gh-issue-*`, `build:task` gates | facts instead of drafts; prior-art and size checks; understanding checked before the PR; comments checked and cut | on |
| 4 | `dotfiles/claude-md/hobby.CLAUDE.md`, `just check-placeholders`, `just ai-policy` | what may be commented, and by whom, per tier | on |

`githooks/policy-lib.sh` is the single definition of the banned patterns, the
placeholder token and the expected commit email. Layer 2 sources it.
`scripts/ai-policy-tier.sh` is the single definition of the **tier**; layer 1
reads it through `policy_tier()`, and layers 3 and 4 call it directly.

The placeholder tokens are `TODO(jb-comment)` (inline) and `TODO(jb-doc)` (doc
comments and README sections). They name **what needs explaining, never the
explanation**.

### Layers 1 and 2 are wired but disarmed

**The false positives are why.** Layer 2 classifies by substring against the whole
Bash command text, so any command whose *argument* happens to contain `git commit`,
`gh pr create` or `gh issue comment` is intercepted as if it were that command — writing the
phrase into a journal entry, a heredoc, a `python3 -c`, a grep pattern. Issue #24
reports it, and disarming the layer tripped it four more times in the session that
wrote this section: twice far enough to open an editor and demand a message, once
for a `cat > facts.md` and once classifying a file edit as a pull request.

That is worse than an ordinary bug because of what the layer does when it fires: it
blocks and waits on a human. A check that stops work on text merely mentioning the
thing it guards costs more attention than the thing it prevents.

The classifier at `hooks/human-authored-text/hook.sh:75-110` is untouched here —
this turns the layer off rather than narrowing the matcher, so #24 stays open and
re-arming brings the false positives back with it.

Layer 1 went with it because the two share `policy-lib.sh`, and because the git
hooks were the enforcement half of the same rule.

`policy_guards_enabled` in `githooks/policy-lib.sh` is the single switch. All three
mechanical checks — `commit-msg`, `pre-push`, and the `human-authored-text`
`PreToolUse` hook — call it first and `exit 0` when it says no. It reads
`HOBBY_AI_POLICY_GUARDS`, defaulting to `POLICY_GUARDS_DEFAULT`, which is `0`.

```
HOBBY_AI_POLICY_GUARDS=1 git push        # re-arm for one command
POLICY_GUARDS_DEFAULT=1 in policy-lib.sh # re-arm everywhere, permanently
```

**Nothing was uninstalled.** `core.hooksPath` still points at `githooks/`, the hook
is still symlinked into `~/.claude-hobby/hooks/` and still registered in
`settings.json`, and `just doctor` still checks all of that — it reports the wiring
and the switch on separate lines, because installed and armed are separate
questions. The switch is one word away from being on, which unsetting
`core.hooksPath` would not have been.

The default lives in `policy-lib.sh` rather than `settings.json`'s `env` block
because a git hook is not a Claude Code process: `commit-msg` and `pre-push` run
under whoever invokes git, including by hand, and never see that file.

Two consequences:

- **The work-identity check falls with them.** `commit-msg` and `pre-push` also
  rejected commits authored under the work email, which is a leak, not an
  authorship question — it shared a file with the AI checks and so shares the
  switch. `just doctor` and `just git-identity` still cover the two repos that set
  the identity explicitly; nothing now catches a hobby repo that never did.
- **Layers 3 and 4 are instructions, and stay in force.** The rules in this file
  and in `hobby.CLAUDE.md` — I write every commit message, PR body, issue and
  comment; no machine attribution; the comment tier table — are unchanged. What is
  gone is the mechanism that made them non-optional, which is exactly what "layer 1
  is the only guarantee" below was about.

### The two tiers

`scripts/ai-policy-tier.sh` takes the first answer of:

1. `AI_POLICY_TIER` in the environment
2. an `ai-policy: upstream` / `ai-policy: private` line in the repo's `CLAUDE.md`
3. an origin owner that is not `julianbieber` → `upstream`
4. `gh repo view --json isFork` → `upstream` when true
5. otherwise `private`

Step 4 is the only one that touches the network; `HOBBY_AI_POLICY_NO_GH=1` skips
it, and a fork then resolves `private`. Step 2 is the only marker that works
offline.

`just ai-policy` reports the tier and the reason per repo. Under `~/fun_repos` it
finds 11 `upstream`, of which 7 are forks owned by me: `bevy_ecs_ldtk`,
`bevy_image_export`, `bevy_new_minimal`, `bevy_sky_gradient`, `glasgow`,
`presenterm`, `wgsl-analyzer`. The predicate this replaced —
`github.com[:/]julianbieber/` on the push URL — matched all 7 as mine.

### Ownership, not the URL

The URL predicate tested where a push lands, and a fork's whole purpose is that
the code does not stop there. An unresolvable tier fails closed —
`push_is_upstream` returns true for anything that is not literally `private`, so a
broken resolver over-enforces instead of silently letting a public push through.

### Why the private tier stops at public doc comments

The ban produced markers rather than comments: 56 of them in this repo alone,
none of which describe anything. A doc comment on a public item states the
contract a caller needs before reading the body, and is checkable against the
signature. An inline comment on a private item is where restating-the-code lives,
so it stays a placeholder on both tiers.

### Layer 1 is the only guarantee — which is what turning it off costs

This is the reasoning the layers were built on, unchanged; with layer 1 disarmed it
now reads as the list of what nothing checks.

The `Co-Authored-By: Claude` trailer is not the model's idea — it is instructed by
the harness system prompt, so a CLAUDE.md rule against it is one instruction
against another. Layers 2–4 are all inside Claude Code: layer 2's matcher is
`Bash`, so a commit made any other way never reaches it, and layers 3–4 are
instructions a model can misread. Layer 1 runs whoever invokes git, including me
by hand.

### Deny, rather than rewriting the command

`updatedInput` is available on `PreToolUse`, so this is a choice, not a
limitation. Rewriting means splicing my prose back into a shell command string,
which is a quoting hazard on exactly the text most likely to contain quotes. The
deny path instead hands over a file and records its sha, and pass 1 re-checks that
sha on the re-issued command — so nothing can edit the authored text between my
writing it and git using it.

### Stripping the trailer is half of it

Norm 1 bans the attribution; norm 2 requires a disclosure. Doing only the first
produces a PR that hides AI involvement completely — the opposite failure.
`/ai-use-facts` assembles the record, so the paragraph I write has something to be
accurate about.

### This repo is `private`, and that is recent

The blanket style rule used to apply here on the argument that habit beats scope.
It no longer does — `ai-policy: private` at the top of this file, and the tier
table in `hobby.CLAUDE.md`, are what govern. Every `TODO(jb-comment)` still in
this tree predates that and is inline, which is the one category the private tier
does not relax.

[bevy-website#2551]: https://github.com/bevyengine/bevy-website/pull/2551

## Installing

`just bootstrap` is the fresh-machine entry point; `just doctor` diagnoses an
existing install and names the recipe that fixes each gap. Every recipe is
idempotent.

Anything more involved than a symlink lives in `scripts/` rather than inline in the
justfile: `preflight.sh` (dependency check), `wire-nushell.sh` (append the `use`
lines once), `helix-paths.sh` (find this machine's Helix bits), `wire-helix.sh`
(generate the Helix language config), `helix-servers.sh` (declared-vs-installed
language servers), `local-paths.sh` (the machine-local overrides file), `doctor.sh`.

## No machine-specific paths in this repo

This repo is checked out on more than one machine and they do not agree on where
anything outside `~/fun_repos` lives — the Helix checkout is `~/fun_repos/helix` on
one and `~/outside_repos/helix` on another. A hardcoded default is therefore a
guaranteed breakage somewhere, so **paths are discovered, never assumed**, and
`scripts/helix-paths.sh` is the single resolver every script and recipe uses.

The escape hatch is `~/.config/hobby-tooling/paths.env` (`just local-paths`),
outside the repo and never committed. It is also the *only* correct home for a path
under the work tree: `TREE_SITTER_MML` belongs there, not in any file here.

The rule bites hardest in formats with no variable expansion. `dotfiles/zellij/config.kdl`
binds `Alt a` to the tool-audit log, and KDL cannot expand anything — so the binding
runs `sh -c` and lets the shell do it:
`tail -f ${CLAUDE_CONFIG_DIR:-$HOME/.claude}/tool-audit.log`. Since `hcdev` exports
`CLAUDE_CONFIG_DIR` before launching zellij, a hobby pane follows the hobby log and
every other session follows the work one — one committed line, correct on both
accounts. Reach for a shell before reaching for a hardcoded path.

## Dotfiles are installed, never merely present

Every symlink in `install-*` goes through `scripts/link-dotfile.sh`, not `ln -sf`.
The difference is the one case `ln -sf` cannot handle: it replaces a stale symlink
but **refuses a regular file**, and a regular file is what the retired home dotfiles
repo (`github.com:julianbieber/config.git`, see #9) left at these paths. The helper
moves it to `<target>.pre-hobby-tooling.<timestamp>` and prints where it went —
nothing is overwritten in place, and no backup is silent.

`doctor` tests these with `-h` before `-e` for the same reason. `-e` is true for a
file some *other* repo installed, which is exactly how `~/.config/zellij/config.kdl`
passed as this repo's for as long as it did: the layout was linked, the config was
merely there, and `default_shell "nu"` — what makes `hwt`/`hcdev` reachable in a pane
— came from somewhere `just bootstrap` had never heard of. A real file at a path this
repo owns is a gap, not a pass.

`link-dotfile.sh` is also where per-machine variants will land (#9): prefer
`<source>.$HOBBY_MACHINE` over `<source>` there, and every recipe inherits it.

## The conversation under ctrl-g

**ctrl-g** in Claude Code opens `$EDITOR` on the prompt buffer. Here that shows
the conversation so far beside it, so the context is readable while writing the
next prompt. Two pieces, and they only work as a pair:

- `hooks/claude-conversation/hook.sh` renders the transcript to
  `/tmp/claude-conversation-<key>.md` on every `PostToolUse` and `Stop`
- `bin/hx-claude` is `$EDITOR`, and vsplits that file next to the prompt buffer
  ctrl-g hands it

`<key>` is the sanitized `$ZELLIJ_SESSION_NAME`, so ctrl-g in one pane opens that
pane's conversation and not a neighbour's. **The key derivation is duplicated in
both scripts and must stay identical** — diverge and each silently uses a
different file.

This is not about the zellij layout. The `editor` pane stays a plain shell; the
conversation view is a ctrl-g thing, transient, and gone when you close it.

`settings.json` calls the hook **by absolute path**
(`$HOME/.claude-hobby/hooks/claude-conversation.sh`), like every other hook, not
by a bare `claude-conversation-dump` on `PATH`. That bare name was the bug: it
resolved only on a machine that also had the work checkout, and there only
through a symlink into a work *worktree* that any `wt rm` deletes. On a
hobby-only machine it failed after every single tool call, silently, because
`PostToolUse` registers it with an empty matcher. Only `hx-claude` needs
`~/.local/bin`, because `$EDITOR` names it.

`hx-claude` **keeps its upstream name**, for the reason `lsps/` keeps theirs and
`mcps/` does not: `$EDITOR` is one variable shared by both accounts, so a
`hobby-`-prefixed second name would isolate nothing — it would just be a name
nothing points at. Installing from here replaces any link from the work checkout,
which is harmless because the script is account-agnostic. The dump path is safe
to share for the same reason: `hcdev` names sessions
`hcdev-<project>-<topic>-<task>` and the work `cdev` names its own `cdev-…`, so
the two accounts already have disjoint keys inside
`/tmp/claude-conversation-*.md`.

## Helix: one dev flow, three files

Helix merges `~/.config/helix/languages.toml` over the `languages.toml` compiled
into its binary, so:

```
<checkout>/languages.toml               machine-NEUTRAL, committed, builds the binary
~/.config/helix/languages.local.toml    machine-SPECIFIC, generated, never committed
~/.config/helix/languages.toml          = the two concatenated, an OUTPUT
```

Edit the first for anything shared; let `just wire-helix` regenerate the third.
Editing the installed file directly is the one thing that does not work.

What forced the split is `[[grammar]] name = "mml"`. Its `source.path` must be
absolute, differs per machine, and does not exist at all on a hobby-only one — and
`helix-term/build.rs` builds grammars with `STRICT = true`, so a single unresolvable
path fails the whole `hx` build on every other machine. Committing that line was
what previously required commenting it out per machine. Now it is generated.

That reasoning applied to `.hmml` too, until HMML got its own grammar —
`lsps/tree-sitter-hmml`, forked from the work `tree-sitter-mml`. Because it ships
in this repo, `wire-helix` derives its path from the repo root and declares it
unconditionally, so **every machine highlights `.hmml`**. `TREE_SITTER_MML` now
only affects the work `.mml` dialect.

Two invariants for that fork: the grammar name, the exported symbol in
`src/parser.c`, and `name:` in `grammar.js` must all say `hmml` (Helix `dlsym`s
`tree_sitter_<name>`, and a mismatch silently yields no parser); and
`src/parser.c` stays committed, so a machine needs only a C compiler — requiring
the tree-sitter CLI everywhere would reintroduce the portability problem. The
queries are a real copy, not `; inherits: mml`, which was a second hidden
dependency on the work grammar.

`just helix-build` rebuilds `hx` with `HELIX_DISABLE_AUTO_GRAMMAR_BUILD=1` (the
automatic path runs `--grammar fetch` first, which dies on any grammar whose remote
has gone away and takes the binary build with it), then builds grammars separately
where a failure is survivable.

## Paths

```
~/fun_repos/<project>                          main checkouts
~/fun_worktrees/<project>/<topic>/<task>       the worktree itself
~/fun_repos/hobby-mimisbrunnr/notes/
    <project>/<topic>/<task>                   task notes
~/.claude-hobby                                CLAUDE_CONFIG_DIR
~/.config/hobby-tooling/paths.env              machine-local overrides, uncommitted
```

The Helix checkout is deliberately absent from that list — it is discovered, and it
is on branch **`custom`** wherever it is.

## Account Isolation

`CLAUDE_CONFIG_DIR=~/.claude-hobby` gives a fully separate login, MCP set,
settings, skills, and history. Two ways in:

- `chobby` — wrapper on `PATH` that sets the var and execs `claude`
- `hcdev` — exports the var before launching zellij, so every pane inherits it

A bare `claude` always means the **work** account. If you're in a hobby worktree
and unsure, check `echo $CLAUDE_CONFIG_DIR`.

## Conventions

- Every tool is a folder with a `README.md`, never a loose file.
- `skill.md` is the canonical skill filename.
- No `julian-` prefix — this repo has one author, so provenance prefixes are noise.
- Hobby projects use **GitHub issues**, not Jira. Anything reaching for a tracker
  goes through `gh`.
- No secrets, no work identifiers, no company hostnames. This repo is private but
  it is not a work repo.

## Installing

`just` lists every recipe. `just install` runs them all. Each `install-*` recipe is
independently re-runnable.
