---
description: Read a branch's diff back as one concise .hmml review delta — the touched units' behaviour walks (systems with their schedule, observers with their event) with every changed step marked +/-/~ and linked to the diff, the data (with ECS roles) and public signatures it touches, and its tests in summary — for reading before merge
argument-hint: '[--against <ref>] [<checkout>]'
allowed-tools: Read, Grep, Glob, Write, Bash(ls:*), Bash(pwd:*), Bash(rg:*), Bash(wc:*), Bash(sed -n:*), Bash(rm -f:*), Bash(hobby-mml-parser:*), Bash(git -C * rev-parse:*), Bash(git rev-parse:*), Bash(git -C * diff:*), Bash(git -C * show:*), Bash(git -C * log:*), Bash(git -C * merge-base:*), Bash(git -C * branch:*), Bash(git -C * grep:*), Bash(git -C * remote:*), Bash(git -C * status:*), Bash(git -C * ls-files:*), Bash(gh pr view:*), mcp__tree-extract__list_modules, mcp__tree-extract__get_neighbors, mcp__tree-extract__call_path, mcp__tree-extract__impact_of, mcp__tree-extract__get_symbol, mcp__tree-extract__search_symbols, mcp__lsp-mcp__document_symbol, mcp__lsp-mcp__find_references, mcp__lsp-mcp__goto_definition, mcp__lsp-mcp__hover
---

Read one branch's diff back as a **review delta**: a single `.hmml` file that
tells what the PR changes at the altitude I would narrate it to a colleague,
with every changed line pointing at the place in the diff that makes it. I read
the delta first and the hunks second.

The grammar is `~/fun_repos/hobby-tooling/lsps/hobby-mml-lsp/grammar.md`
(HMML v3). Read it before the first write in a session; the rules below assume
it.

The delta is **derived, never maintained**. Every run rewrites the file from the
diff, without asking. Nothing is carried over from an earlier run.

**The budget is the reader.** The delta exists because a diff is too big to hold
in your head. The limits:

- **A unit walk has at most 25 lines**, counting every step line at every depth
  and leaving out `invariants:`. A walk over 25 is **lifted** until it fits:
  - Merge sibling steps that serve one decision into one step, the way a person
    would tell it. For example, "if the purse is short, the item is sold out or
    the shop is closed: shake the button" replaces one `if` per case.
  - Fold a step's nested detail into the step itself when the detail is not what
    changed.
  - A lifted step keeps the gutter of what it merged: `+` when all of it is new,
    `~` or a `-`/`+` pair when old and new are mixed. It also keeps one diff
    link, to the first hunk.
  - Never drop context steps to save lines; they are what the change is read
    against.
  - Never fold a scope, ordering, failure or persistence invariant into a step
    (grammar §7). Invariants do not count against the 25 lines.
- **`## api`** holds only the signatures the grammar's §9 lists.
- **`## tests`** is a handful of grouped lines.

When more units are touched than fit, keep the ones whose behaviour a player or
a caller sees, and report the rest.

## Instructions

01. **Pin down the change.**

    - **Checkout**: the argument, else the current directory when it is a task
      worktree (`~/fun_worktrees/{project}/{topic}/{task}`) or an aspect
      worktree (`~/fun_aspects/{project}/{topic}/{task}/{aspect}`). Never
      `~/fun_repos/<repo>`.
    - **`head`**: `worktree` when `git -C <checkout> status --porcelain` lists
      anything — the change, or part of it, is not committed yet, which is how
      `/task:build` and `/task:auto` call this skill. Otherwise
      `git -C <checkout> rev-parse --short=10 HEAD`.
    - **`branch`**: the checked-out branch.
    - **`base`**: `--against <ref>` resolved to a commit, else
      `git merge-base origin/<default branch> HEAD`.
    - **`pr`**: `gh pr view <branch> --json url,title,body`, run in the
      checkout. Leave `pr` out when there is no PR yet.
    - **`repo`**: `{project}`, the first path component under `~/fun_worktrees`
      or `~/fun_aspects`.
    - **Output**: the notes dir of the checkout,
      `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/delta-<branch>.hmml`
      for a task worktree and
      `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/aspects/{aspect}/delta-<branch>.hmml`
      for an aspect's, with `/` in the branch replaced by `-`. When the file
      exists, `rm -f` it without reading it; the Write tool would otherwise ask
      you to read it first. Leave any older `<unit>.hmml` specs in the dir
      alone: they are v2 notes, not inputs.
    - **Linter**: `hobby-mml-parser --version` must print `hobby-mml-parser 3.`.
      If it prints anything else, or the command is missing, it is the v2 linter
      or none: `just -f ~/fun_repos/hobby-tooling/justfile install-mml`.

02. **Read the diff.**

    - With a commit `head`: `git -C <checkout> diff --stat <base>...HEAD`,
      then the full diff.
    - With `head: worktree`: `git -C <checkout> diff --stat <base>` and the
      full `git -C <checkout> diff <base>`, which cover committed and
      uncommitted changes alike, plus every file
      `git -C <checkout> ls-files --others --exclude-standard` lists, read
      whole: an untracked file is new and no `git diff` shows it.
    - Sort the changed files into three groups: source, tests, and the rest
      (assets, config, shaders, docs, build).
    - The rest shows up only through what it does to a unit. An asset, RON
      table or shader change that changes behaviour becomes a step (grammar §7).
    - Note every changed file no line of the delta will cover, for the report.

03. **Find the units** (grammar §7).

    - In a Bevy app a unit is what the `App` runs: a system (find its
      `add_systems(<Schedule>, …)` call), an observer (`add_observer` or
      `.observe`, taking `On<E>`), or a `Plugin::build` whose registration
      changes beyond the systems it adds. Walk up from the changed functions'
      callers with `rg` until you reach one.
    - A non-Bevy crate (a CLI, an LSP) keeps the classic entry points: a
      command, a request handler, a queue consumer.
    - `tree-extract` (`impact_of` / `call_path`) helps only when the checkout
      already has a `.project-graph.json`. Never build one: `build_graph` writes
      an untracked file into the checkout.
    - `find_references` from `lsp-mcp` resolves where rust-analyzer is set up.
    - Say in the report which way you went.
    - Stop at the first entry point on each path, unless a collaborator's output
      has a different consumer. Then it is a unit of its own.
    - Name and order the units as §7 says. A system unit writes
      `in <Schedule>` and an observer `on <Event>` (§7.1), read off the
      registration as `head` leaves it.

04. **Write each unit's walk as HEAD leaves it.** Read the system or observer
    and its collaborators, and write the whole walk at mental-model altitude.

    - Each step is a decision or an action a player or caller would notice.
    - A step naming a local variable, a `Query` parameter or a language
      construct is too low.
    - A step that could describe ten implementations is too high.
    - What a system reads and writes is told through its steps ("credit the
      bounty to the Purse"), not listed.
    - Unchanged steps stay as context (gutter ` `).

05. **Mark the change from the diff, not from intent.** For every step, ask
    whether the diff touches the code behind it:

    - **The behaviour is new:** `+`, and so is everything nested under it.
    - **The behaviour is gone:** `-`, written as it was at `base`
      (`git show <base>:<path>`). Place it where it used to sit relative to its
      surviving neighbours. Everything nested under it is `-` as well.
    - **One clause changed:** a `~` line,
      `<shared words> <old clause> → <new clause>`.
      - Cut each side down to what differs.
      - When the shared words would only come after the change, rephrase the
        step so they lead.
      - When the change only adds a clause, write the new side as
        `… <added clause>`.
      - When more than one clause changed, write a `-` line and a `+` line.
    - **The step only moved:** context, at its `head` position (grammar §7).
    - **When it runs changed** — a new schedule, run condition or ordering
      constraint on the registration: a step at the top of the walk linked to
      the `add_systems` line (§7.1).
    - **Untouched:** context. A refactor that leaves the behaviour as it was is
      context too; that is the point of the altitude.

    Every `+`, `-` and `~` step gets a diff link, `@ <file>:<line>`.

    - Use the shortest path suffix that names one file, usually the file name.
      In a Bevy repo `mod.rs` and `plugin.rs` repeat, so these need their
      directory: `waves/mod.rs`.
    - Use the first line of the expression that decides the behaviour: the
      `head` side (the file on disk, for `head: worktree`), or the `base` side
      for a `-`.
    - A changed step without a link is a defect, because the link is what lets
      me jump from the step to its hunk.

    Then write the unit's `invariants:`. Read them off the code (a cap, an
    ordering, a scope, an idempotency rule), not off intuition. Mark each one by
    whether it holds at `base` and at `head`, including one whose truth changed
    while the code behind it did not. An invariant that holds at both sides is
    context, whatever enforces it now (grammar §7).

    Look for these in particular, and write every one the PR adds or changes:

    - **scope:** which entities a query can reach, e.g. "only enemies inside the
      arena take splash damage";
    - **ordering:** what runs before what, e.g. "the bounty is paid before the
      enemy is despawned";
    - **failure:** what an error or a missing entity leaves behind;
    - **persistence:** what outlives a state change or a despawn, and where.

06. **`## data`** (grammar §6). Fill it in after `## api`, though it stays
    before the units in the file. List a type only when a unit step or an
    `## api` signature of this delta names it, and one of these holds:

    - its shape changed;
    - the PR added or removed it;
    - the reader needs its shape to follow a changed step.

    - Give it its role off the derive (§6.1): `#[derive(Component)]` →
      `component`, `Resource` → `resource`, `Event`/`EntityEvent` → `event`,
      `Message` → `message`, `States`/`SubStates` → `state`, anything else none.
    - A marker (`struct Boss;`) is `component struct Boss {}`.
    - Write struct fields as context or marked lines, so a changed field reads
      in place. Field names are written as the code writes them.
    - Translate types into the subset: `&str`/`String→String`, `usize→u64`,
      `isize→i64`, `HashMap<K,V>/BTreeMap→Map<K, V>`, `HashSet→Set<T>`,
      `(A, B)→Tuple<A, B>`, `Box<T>`/`Arc<T>`/`&T→T`, `Handle<T>` as written.
      Bevy's `Entity Vec2 Vec3 Quat Color Timer Duration` stay as they are.
    - Never list Bevy's own types (`Transform`, `Time`, `Children`, …).

07. **`## api`** (grammar §9). Collect the public functions the unit steps do
    their work in. End each such step with `@ api Module::fn`, listing several
    functions with commas when the work spans them. Then add:

    - every public function that exists at `base` and that the PR removes or
      whose signature it changes, and every function the PR makes public or
      private (`+` / `-`);
    - a module line for each module the PR adds, even when none of its functions
      is linked.

    Leave out systems and observers (they are units), a function the PR adds
    that no step links to, private helpers, constructors, and `Plugin` /
    `FromWorld` impls. Write signatures faithful to the code without the
    receiver, with `@ <file>:<line>`.

    - **Modules** are as grammar §9 defines them: a Rust module, or a type with
      an `impl`. A resource with methods is its own module.
    - **A changed signature** is a `-`/`+` pair, unless only its tail changed,
      such as the return type.

08. **`## tests`: a summary** (grammar §10).

    - Take the changed test files and `#[cfg(test)]` modules from the diff.
    - Group the tests that change together for one reason into one
      `tests <N> — <why>` line. N counts `#[test]` functions as written in the
      source.
    - Give a test its own `test <name> — …` line only when it is the sole guard
      of a changed step, or when its change needs saying on its own.
    - Removed tests are `-` lines, and a reworked test is a `~` line.
    - Every test line links to the first test it covers. A group whose tests sit
      in several files is one line per file.
    - When a line tests a changed step or invariant, put `^anchor` on that step
      or invariant (snake_case, named after the behaviour). List the anchor in
      the test line's `@ guards`.
    - Anchor only lines a test points at, and leave unchanged tests out.

09. **`## intent`.** One to three lines saying what the PR sets out to do for
    the player or caller.

    - Read it from the PR title and body, the commits, the task's `issue.md`,
      and the done-when in `build-task.json` when there is one.
    - Write it last, once the walks show what the change does.
    - Where the stated intent and the diff disagree, the walks follow the diff,
      and the report says so.

10. **Write and lint.** Write the file (header, then sections in grammar order)
    and run `hobby-mml-parser <path> --lint`. Fix every error and warning.
    `unresolved-diff-link` means the suffix is ambiguous or the line is wrong:
    find the real one, never drop the link.

11. **Report and stop.** Report:

    - the path and the line count;
    - per unit, its trigger, its walk's line count and its `+`/`-`/`~` lines
      (steps and invariants separately, every marked line counted);
    - the `## data`, `## api` and `## tests` counts;
    - how the units were found (`tree-extract`, `lsp-mcp`, or `rg`);
    - the units left out for budget, and the units you lifted;
    - every changed file no line of the delta covers, and why: no behaviour,
      docs only, or build only;
    - any disagreement between the stated intent and the diff.

    Do not iterate on the file, and never `git add` or `git commit` in the notes
    repo.
