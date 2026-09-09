# Hobby Tooling

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

Four states, and they now say what a checkout needs rather than what flag reaches
it:

| state | means | what happens |
|---|---|---|
| `unsynced` | commits no remote has, or a dirty tree | a worktree is committed and pushed; a main checkout is reported |
| `no-remote` | commits, but nowhere to push them | reported, with the `gh repo create` |
| `unborn` | zero commits; every file untracked | reported; a first commit is a decision |
| `skipped` | `hobby.sync=false` in that repo | — |

`unborn` is its own state because it is not a degree of `unsynced`: five repos here
were `git init`'d and never committed, so `.gitignore` and `Cargo.toml` are
untracked alongside `src/`. Reporting that as "12 dirty files" would have buried it.

The opt-out is `git config hobby.sync false`, in the repo's own `.git/config`
rather than a marker file — a marker file in the working tree would itself show up
as untracked, i.e. as the thing it is meant to silence. It does not travel between
machines, which is correct: whether a scratch checkout exists at all is a fact
about one disk.

### Three populations, three treatments, and no flags

The first version had `--pull`, `--push` and `--wip`, and a bare `hwt sync` that
only reported. That put the decision in the wrong place. **Which of those a
checkout needs is a fact about the checkout, not a thing to be chosen at the
prompt** — a main checkout wants exactly a fast-forward, a task worktree wants
exactly commit-rebase-push, every time, in both directions of a trip. Choosing
per run meant choosing wrong: `--push --wip` on leaving would push a main
checkout's scratch commits, and `--pull` on arriving never rebased a worktree at
all.

So the treatment is keyed on the population and the only flag left is
`-p <project>`:

| | treatment | because |
|---|---|---|
| `~/fun_repos/<project>` | fetch, fast-forward, nothing else | a clone here is read, tried and scribbled in; its local commits and dirty files are scratch, not work in flight |
| a task worktree or orphan | wip-commit the tree, rebase onto the upstream, push | a task branch exists to become a PR, so it is meant to travel |
| the notes repo | a `chobby` session running `/sync-notes` | its commit messages and its conflicts are prose |

Anything a main checkout cannot fast-forward — dirty, diverged, detached, holding
local commits — is named in the summary with the command that would fix it. That
is all that survives of the old report-only mode, and it is the part that was
load-bearing: it is where every unsynced byte turned out to be.

### The wip commit is on the branch, and the refs are gone

`--wip` used to write the working tree to `refs/wip/<machine>/<branch>`, outside
`refs/heads/` so a snapshot could never become a branch or a PR by accident, and
`--pull` fetched the other machine's into `refs/remote-wip/` and listed them with
a diff command.

**That is deleted, and a dirty worktree is now committed onto its own branch**,
with `wip` as the first word of the subject. The reason is that the ref could not
participate in the rest of the treatment: a rebase refuses to run over a dirty
tree and a push carries branches and nothing else, so a snapshot beside the branch
left the branch exactly as unsynced as before and required a person to notice a
list, read a diff, and apply it by hand. Every step of that was optional, and the
optional steps are what does not happen on the day you switch machines.

The price is real and is stated where it will be read: a `wip` commit is in the
branch's history and has to be squashed or amended before review. It is
`--no-verify`, because a repo's commit hooks lint what a person wrote and this is
a machine picking up a tree mid-edit; a failing hook must not block the sync.

Nothing was lost in the deletion — a sweep of every checkout found zero refs under
`refs/wip` or `refs/remote-wip`. The mechanism was built, documented, and never
once used, which is itself the argument: the path that needs a person to run a
second command after reading a report does not get run.

A rebase that conflicts is **aborted**, not left in progress. What touches a
worktree next is an editor or a Claude session, and neither expects to arrive
mid-rebase.

**Stashes remain out of scope.** They cannot be pushed at all, so no amount of
tooling reaches them, and `CLAUDE.md` already treats the shared stash stack as a
hazard rather than a place to keep things. Uncommitted work in a worktree is now
covered by the wip commit; in a main checkout it is reported and left alone.

### The wip message is a pointer, not a summary

A wip commit's subject is a file count and the machine; its body says the commit
is a snapshot and must be squashed, and — for a task worktree — names that task's
notes directory. It never describes the change. The notes are synced too and are
where the prose lives, so the message points at them instead of paraphrasing them.

`<machine>` is `$HOBBY_MACHINE`, falling back to the short hostname. In the ref
scheme it was a collision key; here it is the only thing in the commit that says
which disk the uncommitted work was sitting on when the sync picked it up.

### The notes repo is a session, not a git command

Every other checkout is synced by rules that need no judgement — fast-forward or
don't, rebase or abort. The notes repo breaks that in both directions, so
`hwt sync` hands it to `chobby` running `/sync-notes` and touches it no further.

**Its commit messages read the files.** The convention is
`<project>/<topic>/<task>: <what these notes are>`, one commit per task
directory, which makes `git log --format=%s` the list of what has been worked on.
A tree of `wip @ laptop: 3 modified` would destroy exactly that, and a file count
is all a shell function can honestly write.

**Its conflicts are prose, and the skill refuses them.** Two machines writing a
journal entry for the same day produce a both-modified file that no merge driver
can resolve, so `/sync-notes` stops and reports rather than auto-resolving. That
is the right call for the same reason the wip commit is not: a wrong resolution
here loses work with no test failing and no compiler complaining.

**The skill is not in this repo.** It is `.claude/commands/sync-notes.md` in
`hobby-mimisbrunnr`, project-scoped to the repo it operates on, which is the one
place it is ever invoked from — `hwt sync` starts the session in that directory.
Nothing here installs it, and `install-skills` does not see it.

Run from inside a Claude session, `hwt sync` prints the command rather than
nesting a second agent in the first.

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

**And a command that fails kills it the same way.** `nu -c` exits on the first error and
takes the window with it, so a failing `hwt restore` was indistinguishable from both
bugs above — nothing on screen either way. Every nu launch now goes through
`hold_on_error`, which wraps the pipeline in `try`/`catch` and waits on `input`, so the
message that says what went wrong is still there to read. The cost is a keypress after a
session that exits non-zero; that is worth paying to never lose an error again.

`Restore` also runs `hcdev wt` after `hwt restore`, as `Start task` does after `hwt new`.
The card is only rescanned by the **Refresh** button, so a restore that stopped at the
worktree left the card reading `Removed` and a second click could report nothing but
"already present".

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

### A spec holds both versions of the unit (v2.3)

A spec used to be edited in place to describe the change a task intended, which
threw away the thing the change was a change *to*: read afterwards, the file said
what the code should do and nothing said what it did. §10 makes the file keep the
**baseline** — the unit as it is before the change — and write the change on top
of it as `# +` / `# -` comment markers. Both versions in one file, the delta
surviving a commit, and `/hmml-translate` told exactly what to add and remove
instead of inferring it from `git diff` in the notes repo.

The markers are ordinary comments, so this is a convention, not grammar — which
is precisely why `mml-parser/src/annotations.rs` exists. **No other pass sees
them**, including the one rule that makes the convention work: dropping every
`# +` block and un-commenting every `# -` line must reproduce the baseline. That
is checked by reconstructing it, re-parsing and re-validating, and reporting any
error the baseline has that the annotated spec does not — `unrecoverable-baseline`,
typically a `# +` on something the baseline itself already needs.

**The annotations stay after the code lands.** Stripping them would leave a file
indistinguishable from a plain baseline, and the spec is the record of what the
task changed. The next task gets its baseline from a fresh extract, not from this
file.

**§10 is the one change spelling, which cost v2.2 its member delta.** A `@ <path>`
type body's inline `+ streak: u32` / `- remaining` said the same thing a second
way, and the two could not both be right about what an unmarked line means: §10
needs unannotated text to be the baseline, and `+ streak` is intent. The inline
form still parses so v2.2 specs keep working, but the grammar now calls it legacy
and nothing emits it. Nothing was lost in the trade — a `# +` member is a comment,
the parser skips comments, so it never reaches `rust-member-unknown`, which is
exactly the "intent is never an assertion" suppression the inline markers were
written to provide.

One divergence from the work copy: `redundant-change-bullet` is a **warning**
here, not an info hint. Work's `--lint` does not gate on info severity; this one
gates on any diagnostic at all, so an info hint would gate anyway while claiming
not to. A doubled bullet is a one-character fix, so gating on it costs nothing.

### The detail layer says what a collaborator can do (v2.4)

A behavioural spec names its collaborators and stops — `WaveRng — draws the
spawn offsets` and no more. So the moment a change is about a module's API
rather than a unit's flow, the spec has nothing to offer and translation is back
to prose → code. `kind: module` is the layer underneath: **one module's public
surface**, `## api` in the slot every other kind writes `## behavior` in, one
`fn` signature per public function with the bullets describing it nested beneath.

**The two failure modes of a ref get different severities, and that is the whole
design.** A `@ spec unit` whose file is missing stays a warning — the spec may
be planned and not yet written. A `@ spec unit::function` whose file exists but
defines no such entry is an **error**: the file is right there to check the name
against. That is what makes a member ref worth navigating by, and it is why
`/hmml-extract` writes both layers in one run — emitting the overview alone
would hand back a spec that does not lint.

**A module with no public surface gets no spec.** `## api` would be empty, which
raises `empty-api`, which the lint step then tells you to fix — so "one spec per
`## modules` line" would contradict the linter. The module keeps its line,
because it still does work in the behaviour; it just gets no `@ spec` ref. The
normal case is a helper that is private to the unit, which is the same boundary
the detail layer already draws around private functions, one level up.

**A Bevy system is not an api entry.** A system is called by the schedule rather
than by a caller, and its interface is its access block — it belongs in a
`kind: plugin` or `kind: system` spec. A module spec is for the plain-Rust
helpers a system calls into: a table, a generator, a mesher, a solver. That is
the one place this dialect and the work v1.5 it came from genuinely differ.

Two smaller consequences. `Future`, `Result`, `Either` and `Tuple` join the
containers so a signature can stay faithful to the code it is read next to
instead of normalising an effect away; a tuple stays `Tuple<A, B>` so the type
grammar remains `Ident<args…>`. And a `use` line's ref may not carry a member —
a `use` imports types out of a spec file, and a function is not a type — which
is why the use-line parser now splits on the first colon that is not part of a
`::`, so the whole ref reaches validation instead of being cut down the middle
of the separator.

Where work needed a new `kind-mismatched-section` error, this dialect needed
nothing: sections here are already a named set per kind, so `## behavior` on a
module and `## api` on anything else are ordinary `section-not-allowed-for-kind`
errors.

## HSTRUCT records the standards a repo holds itself to

`lsps/hobby-struct-lsp/grammar.md` is the source of truth. A repo's file is
`structure.hstruct` at its root — or at each crate root in a workspace — and
`just structure` checks every repo that has one.

**It exists because the `CLAUDE.md` files are descriptive and nothing was
prescriptive.** `wusel`'s is 1383 lines narrating how the world generator and
the market work; the section saying how to write new code is ten lines at the
bottom. The standards that keep a repo maintainable and performant were written
nowhere, so nothing could hold the code to them or say how far away it was.

### The standard is durable; the instances are not

This is the whole design, and the obvious approach gets it backwards.

A standard — *"state has one owner"*, *"per-frame work is proportional to what
changed"* — is as true on a project's last day as its first. The **files that
violate it** are renamed, split, and edited constantly. So the file stores the
standard, its argument, and a **budget**: how many offenders exist today.

```
## A file fits in one head

A file that cannot be read in one sitting gets edited by guessing, and the
guesses accumulate.

check: files over 900 lines
budget: 10
```

Eleven is an error. Nine is a nudge to tighten. Renaming a file, splitting one,
or adding two hundred lines to one already over changes nothing in here.

**Nothing in an `.hstruct` file names a module, a file, or a type.** A rule
needing an enumeration to be expressed is the wrong rule for this format. The
offenders are still available — `--detail` prints them — they are simply not
*stored*, because storage is where the churn was.

**What the first draft stored, and why it failed.** A record per module with its
`@ path`, a layer-membership list per layer, and a waiver per violation —
including
`? gameplay/growth.rs 2758 lines`. Editing `growth.rs` at all, even to improve
it, made that waiver stale *and* raised a new violation. The file punished
touching the code, and 236 of its lines were largely a staler copy of the
directory listing. It encoded **measurements**, which churn, in place of
**standards**, which do not.

### The ratchet is the improvement loop

`--ratchet` rewrites every budget the code has moved below, in place, and **only
downward** — a budget that rose would record a regression as the new normal.

That is what lets a repo adopt this on its worst day, which was the one idea the
first draft got right and is kept: write the standards with `budget: 999`, ratchet once,
and the file records exactly how far the code is from each rule. Every later
refactor that improves a number is one command from being recorded.

### A budget is what makes a rough metric usable

`unfiltered queries` and `allocating systems` are heuristics. Some unfiltered
queries are correct; the allocation check matches names rather than types, so it
over-counts a `Vec` built at startup and under-counts one behind a helper.

Under per-instance waivers each would have to be adjudicated and re-adjudicated
on every edit — unusable, which is exactly why the first draft had **no
performance metric at all** despite performance being half of what the standards
are for. Against a budget the count only has to be monotone: getting worse is
caught, and the noise is constant.

### Not everything is countable, and the file says so

A standard declares its verification: `check:` + `budget:`, or `review:` +
`audited:`. A reviewed standard is honest about not being mechanical, which is
the right home for the rules that matter most and that no parser can settle —
*"every spawn in an `OnEnter` carries a `DespawnOnExit`, or it leaks across
screens"* is the most load-bearing rule in `wusel` and is not statically
decidable.

The first draft exiled those to prose, on the reasoning that the file may only
contain what a checker can verify. That was the wrong cut and it is why it
drifted toward whatever was easy to count — line numbers and import edges. The honest
requirement is only that a rule **declares how it is verified**; a dated
`review:` says plainly that a person last looked on that day, which prose cannot
say and which can be seen to go stale.

### Two things the discarded draft got right

**Rust is parsed, and an import resolves by path, not by its last segment.** Text
matching read `use crate::gameplay::{city, growth};` as one import of a module
called `gameplay`. Leaf-segment matching then blamed
`use crate::terrain::brush::Brush` on an unrelated top-level `brush.rs`, and read
`use bevy::app::…` as a local module named `app` — which had put five fabricated
cycles in `wusel`'s first file.

**Workspaces get one file per crate, at the crate root.** `root:` names one
source tree and two files at a repo root collide on the filename. The manifest is
then a sibling, which is what lets `crate:` be verified rather than decorative.

## The vendored language servers

`lsps/` holds six servers. `hobby-mml-lsp` and `hobby-struct-lsp` implement this
repo's own two languages (above); the other four — `claude-lsp`, `claude-skills-lsp`, `treesitter-class-lsp`,
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

## The AI policy stack was removed, and rebuilt on content instead of authorship

Everything that constrained who writes commit messages, PR bodies, issue text and
code comments is **gone from this repo**. This section records what was there, so
the rebuild is not designed blind.

### What was deleted

| Deleted | Was |
|---|---|
| `githooks/` (`commit-msg`, `pre-push`, `policy-lib.sh`) | a global `core.hooksPath` refusing machine attribution, the work git identity, and unfilled placeholders on an upstream-tier push |
| `hooks/human-authored-text/` | a `PreToolUse` gate that intercepted every message-bearing `git`/`gh` command, opened an editor, and denied until a human had typed the text |
| `scripts/ai-policy-tier.sh`, `just ai-policy`, `just check-placeholders` | the `upstream` / `private` tier resolver and the `TODO(jb-comment)` / `TODO(jb-doc)` placeholder convention it gated |
| `skills/pr-facts`, `skills/ai-use-facts` | facts-not-drafts assembly for a PR body and its mandatory `## AI use` disclosure |
| `skills/explain-back`, `skills/comment-review` | a hunk-by-hunk comprehension quiz, then a clause-by-clause trimming pass over every comment in the diff |

`just install-git-hooks` is gone with them, `bootstrap` no longer calls it, the
global `core.hooksPath` is unset, and `doctor` no longer has an "AI policy gates"
section. `install-hooks` gained the same stale-symlink prune `install-skills`
already had, so the retired hook's link in `~/.claude-hobby/hooks/` does not
linger.

### The one rule that survived

**No machine attribution.** No `Co-Authored-By: Claude`, no `Claude-Session:`
trailer, no `Generated with Claude Code` footer, no 🤖. It is now an instruction
in `dotfiles/claude-md/hobby.CLAUDE.md` and nothing else — the mechanism that made
it non-optional was `githooks/`, which is deleted. Kept because Bevy CI rejects the
trailer outright, so it is a correctness fact about where the code goes, not a
preference about authorship.

The work-identity check went with the hooks. `~/.gitconfig`'s `includeIf` on
`gitdir:~/fun_repos/` and `gitdir:~/fun_worktrees/` is now the only thing giving a
hobby clone the hobby identity, and `just doctor` / `just git-identity` the only
things that notice when one escapes it. Nothing blocks a push any more.

### Why it came down

Two reasons, and they are different in kind.

**The gate was wrong in practice.** It classified by substring against the whole
Bash command text, so any command whose *argument* contained `git commit`,
`gh pr create` or `gh issue comment` was intercepted as if it were that command —
a journal entry mentioning the phrase, a heredoc, a `python3 -c`, a grep pattern.
Issue #24 reports it. That is worse than an ordinary bug because of what the layer
did when it fired: block, and wait on a human. It had already been disarmed at
`policy_guards_enabled` for exactly this.

**The policy was wrong in aim.** Its whole content was *who is allowed to write*.
It had nothing to say about whether a comment explained anything, whether a commit
message was readable, or whether a PR description was the right size — so what it
produced was placeholder markers that described nothing (19 still in this tree; the
policy's own note claimed 56 at its peak) and PR bodies whose only guaranteed
section was a disclosure. Forbidding bad prose from one author does not produce
good prose from the other.

The replacement is a quality question, not an authorship one, and it is the open
work of this task.

### Comments: what replaced it, and why the old shape was wrong

The rule now lives in `dotfiles/claude-md/hobby.CLAUDE.md`. **Three places take a
comment — the module, a public element, a test — and nothing else does.**
`/comment-pass` enforces it, over a branch diff or a path, and runs as the
`comments` phase of `/task:build`.

**The enforcement goes both ways, and that is the whole difference from what it
replaces.** `comment-review` could only cut; `explain-back` did the writing; when
the policy stack took both, nothing was left. One reading of the code decides
whether a comment should go and whether one is missing, so it is one pass.
`scripts/comment-candidates.py` finds the five states the rule can settle without
reading the code — `BARE-COMMENT`, `PRIV-DOC`, `PUB-UNDOC`, `MOD-UNDOC`,
`TEST-UNJUSTIFIED` — across Rust, nushell, shell and the justfile; judging whether
a *legally placed* comment says the right thing needs the code, and stays the
skill's job.

The skill's governing constraint is not "cut what is not required" — that is what
produced placeholders last time — but **never assert a reason you have not
verified**. A confident, plausible, wrong comment is the worst output available
here: it is load-bearing for the next reader and invisible, where a missing
comment is merely missing.

The old tier table keyed on two things, and a survey of the 1098 inline and ~1800
doc-comment lines in `tools/`, `lsps/` and `mcps/` falsified both.

**Position does not predict value.** The tier table let a `///` through on a
public item and banned a `//` in a body, so it kept

```rust
/// Check if two LSP ranges overlap
fn ranges_overlap(a: &lsp_types::Range, b: &lsp_types::Range) -> bool {
```

and banned `// Err only if initialize is called twice; first workspace root
wins.` The first restates its own signature; the second is the only statement of
a contract that exists nowhere else.

**Authorship does not predict it either.** 26 of the 28 pure-restatement comments
in the tree (`// Parse URI` ×7, `// Convert position` ×4) are in `mcps/lsp-mcp`,
vendored from the work checkout and never subject to the policy. Hobby-authored
code contributed two. The policy was not holding back a flood of bad comments;
there was no flood. What it produced was the doc/inline skew — 606 doc lines to
250 inline in `hdash` — because doc comments on public items were the only tier
that permitted words at all.

**The replacement keys on responsibility instead, and mostly relocates rather
than deletes.** Of the four most load-bearing comments in the tree, three survive
the new rule by moving: two sit in test functions and become the test's
justification, one sits in `async fn initialize` and is a caller-facing contract,
so it moves into the `///`. Only the fourth dies — `// cheaper than repeated
removes` in `pub fn replace_file`, which is about the body's shape and not about
calling it.

That class is the deliberate casualty, and it is not lost so much as moved out of
the file: **a decision about why a body is shaped one way rather than the obvious
one belongs in the commit message**, dated and attached to the diff that made it,
where `git log -L` finds it. That couples the comment rule to the commit-message
rule below, which is where those notes land.

The module clause carries the one constraint that is easy to violate: a module
header sits far from what it describes and is read long after, so it may hold
only what stays true as the contents change. Naming the responsibility survives a
refactor; listing the contents is wrong the first time something moves.

### The unverifiable half of a comment now has a source

`/comment-pass`'s governing rule is "never assert a reason you have not
verified". That is right, and on its own it caps what the pass can write: most
of a caller's contract is in the code, but the part that is not — which rule
sets the bound, what an upstream system guarantees, why the obvious shape was
rejected — is not in the code by definition, so the rule silently forbids the
most valuable sentence in the comment.

The obvious fix, and the one the old policy stack tried, is to quiz the human
marker by marker after the code is written. That fails for a reason worth
recording: it asks for a judgment about code they did not author, at the moment
where the only honest answer is often *you tell me*.

So the questions moved **before** the code. `/task:build` gains a
business-context stage between the HMML baseline and the delta annotation
(GATE C): the questions are derived from the goal read against what the code
does today, before anything is designed, and the answers are recorded **verbatim**
in `business-context.md` in the task notes. They then reach the delta, the plan,
the code, and only last a comment.

`/comment-pass` becomes a **matcher**. Two sources for a sentence and no third:
a fact verified in the code, or a passage in the recorded context, compressed to
fit and never extended. A site neither covers is reported as a **context gap**
with its `file:line` — closed by extending the context (`/task:build context`),
never by writing the sentence.

This is the same three-source rule the commit-message policy below already
states — my prompts, what we settled, or a verified fact. What changed is that
the first two are now written down somewhere a later session, and a sub-skill,
can read them. A gap is a place the code carries no explanation and I gave none,
which is a decision for me at GATE 2 rather than something to discover in
review.

### Commit messages: the policy made them measurably worse

The AI policy landed on 2026-08-06 and required that every commit message be
hand-written. Split this repo's history there:

| | commits | with a body | mean body | mean subject |
|---|---|---|---|---|
| before | 35 | **35 / 35** | 18 lines | 55 chars |
| after | 9 | **0 / 9** | — | 42 chars |

The obvious confound — that the later commits are smaller — does not hold. Four of
the nine touch 23, 17, 15 and 5 files and carry no body at all, and two shipped a
typo in the subject (`Rewoork`, `mchine`). The other half of the picture: all 20 of
the pre-policy commits worth imitating carry a `Co-Authored-By: Claude` trailer.
The best commit messages in this repo were machine-written, and the rule requiring
them to be hand-written took the body rate from 100% to 0%.

That is a statement about cost, not about writing. An 18-line body per commit is
affordable when producing it is near-free and the work is editing it; hand-written
it is a tax, and the tax was paid in bodies not existing.

**So the rule now runs on content, and the model writes them.** It is in
`dotfiles/claude-md/hobby.CLAUDE.md`. Two things in it are load-bearing.

**Terse, and a record of decisions.** Not the 18-line narratives the pre-policy
commits carried — those are more than is wanted. The body says what was decided and
what justifies it, and nothing else. The diff is already in the commit; a body that
narrates it is spending the reader's attention on what they can already see.

**A justification has exactly three permitted sources**: something said in the
task's prompts, something settled in discussion before the change, or a fact
verified in the code, diff or test output. The third is an *observation* and is
free to state. The first two are *justifications* and cannot be derived — they live
in the conversation, not in the tree, so a model that was not told the reason does
not have it and must ask. Never supply a plausible one. This is the same constraint
`/comment-pass` runs on, sharpened: a comment's reason can at least be checked
against the code, and a commit message's cannot be checked against anything, which
is exactly why an invented one survives.

The attribution ban stays, and its actual reason is narrower than the old policy's:
**a `Co-Authored-By: Claude` trailer is advertising for Anthropic**, and these repos
do not carry it. Bevy CI rejecting the trailer is a second, practical reason that
applies only to Bevy contributions.

History is not rewritten. 20 commits here carry the trailer; the rule is forward-only,
the same call already made about the ~600 work-identity commits, and for the same
reason — the alternative is a force-push across public repos.

Conventional Commits was considered and rejected: `feat:` / `fix:` prefixes carry
less than subject lines like `Drop hobby-mml-index: installed, tested, and never
once run` already do, and nothing here consumes them.

### PR descriptions: the template is what failed, and the artifact survived

Same split, sharper. Median PR body before the policy (#1-#21): 4316 characters,
81 lines. After (#22-#28): 257 characters, 16 lines. #28 has no body at all on a
five-file change.

The proof is #27, which is the authoring gate's template with the blanks left in:

```
## Why
too many false positives, therefore not worth the hassle

## Expected behavior after merge

## Verification

## Scope / out of scope

## AI use
LLM written
```

Three sections empty and the mandatory disclosure reduced to two words. Mandating
headings produced headings, not content.

The headings themselves were not the mistake. Across the 17 pre-policy bodies,
unprompted, `Why` appears 15 times, `Verification` 15, `Scope / out of scope` 12,
`Expected behavior after merge` 12 — alongside one-offs written for the specific
change (`Branch matters as much as path`, `The two mechanisms, and why they're in
this PR rather than a later one`, `Drive-by`). They emerged, and then died the
moment they became a form to fill.

**What replaced them is three sections, each capped.** *What do we change* and
*Why do we change* are one or two sentences each; *How can you test it by hand* is
an ordered bullet list ending in what should be seen. The cap is the point: the
old template failed by asking for five open-ended sections, and the pre-policy
bodies, good as they were, ran to 81 lines that nobody re-reads.

Two rules keep it from decaying back into #27. **No section may be left empty** —
a change with nothing to test by hand says so in one bullet and names what does
cover it, because a blank heading is worse than no heading. And **`## AI use` is
gone, not optional**: the tier resolver that could have told an owned repo from an
upstream one was deleted with the rest of the stack, so a disclosure rule would
have had nothing to key on.

The PR carries the argument for the whole change; the commit bodies carry the
per-decision record. Merges here are merge commits rather than squashes, so both
persist, and restating one in the other is the failure mode to avoid.

PR bodies were also an attribution leak path — #14's body ends with a
`Claude-Session:` URL — so the ban is stated there specifically and not only for
commits.

### What the tree still owes the rule

The rule was written against the code, not applied to it. Outstanding:

`comment-candidates.py .` reports **2165 findings across 239 files**:

| category | count |
|---|---|
| `BARE-COMMENT` | 1192 |
| `TEST-UNJUSTIFIED` | 377 |
| `PUB-UNDOC` | 299 |
| `PRIV-DOC` | 248 |
| `MOD-UNDOC` | 49 |

Deleting is mechanical. The 299 public contracts, 377 test justifications and 49
module headers are real writing, and `TEST-UNJUSTIFIED` is the one that is not
merely writing: the rule says a test whose justification cannot be written should
not exist, so some fraction of those 377 is a question about the tests rather than
about their comments.

19 `TODO(jb-comment)` / `TODO(jb-doc)` markers also remain, in
`tools/hdash/README.md`, `hooks/edit-before-execute/README.md`,
`skills/task/build/README.md` and `dotfiles/nushell/hwt.nu`. They are unwritten
documentation, not policy artifacts, so deleting the convention did not delete
them. `just check-placeholders` no longer exists to find them:
`rg 'TODO\(jb-(comment|doc)\):'`.

[bevy-website#2551]: https://github.com/bevyengine/bevy-website/pull/2551

## Task shaping lives in CLAUDE.md, not in the skill that applies it

The rules for carving a project into issues — six of them, with one labelled
escape — are in `dotfiles/claude-md/hobby.CLAUDE.md` under **Task shaping**. The
skill that runs the pass is `/task-split`, and `/gh-issue-create` checks the three
text-decidable rules before it files anything.

**The rules are in the always-loaded file on purpose.** That file is symlinked to
`~/.claude-hobby/CLAUDE.md`, so it governs every hobby session in every repo,
including the cases that never invoke a skill: a task proposed mid-conversation, a
`/subtask` split, an issue typed by hand. A skill is opt-in, and opt-in enforcement
would have covered only the one case that was already being done deliberately.

So the split is: `CLAUDE.md` holds the rules, `skills/task-split/skill.md` holds
the procedure for applying them and the re-cuts to reach for when one fails, and
`skills/task-split/README.md` holds the argument for why each rule is shaped the
way it is. Nothing is duplicated across the three, because the failure mode of a
rule stated twice is the two copies drifting.

`/task-split` deliberately **files nothing.** Its output is a proposed list to
edit; an accepted entry becomes an issue through `/gh-issue-create`, one at a
time, because the topic label and the body are per-issue decisions and a batch
would make both of them mine to fix afterwards rather than mine to make.

`/task:build` applies them too, in its `assess-split` phase — the one-time
question of whether this task is really one task, asked before any planning and
ending at a hard gate. The two tiers do not overlap: a **subtask** is a separate
deliverable with its own worktree, branch, PR and session, and everything below
that floor is one pass of the pipeline on one branch. The gate **refuses** on a
branch that already has commits or a dirty tree, and that refusal is what makes
the tier sound: `/subtask` cuts from the parent's branch, so at assessment time
the two are the same thing and each subtask is genuinely independent, while
later they would inherit unmerged work. Depth is bounded at one by `is_subtask`
in the seeded state file rather than by asking the model to remember. Most tasks
are one task, and the phase is expected to say so in a line.

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
