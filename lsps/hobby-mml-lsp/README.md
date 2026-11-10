# hobby-mml-lsp

| Field        | Value                                                 |
| ------------ | ----------------------------------------------------- |
| Type         | lsp + CLI                                             |
| Binaries     | `hobby-mml-lsp`, `hobby-mml-parser`                   |
| Owner        | @julianbieber                                         |
| Dependencies | `git` on `PATH`; `tree-sitter-hmml` in the Helix fork |
| Status       | experimental                                          |

## Description

Language tooling for **HMML** (`.hmml`), the hobby dialect of the work MML. An
`.hmml` file is the **review delta of one pull request**: a header naming the
repo, branch and the `base`/`head` commits, then `## intent`, `## data`, one
`## unit` per touched entry point, `## api` and `## tests`. Every body line
carries a gutter (` ` context, `+` added, `-` removed, `~` changed as
`<old> → <new>`), and a changed line points at the place in the diff that makes
it true with a diff link `@ <file>[:<line>]`.

[`grammar.md`](grammar.md) is the source of truth. `/hmml-extract` writes the
files; nothing else edits them.

This is a Cargo workspace:

- **`mml-parser`** (package `hobby-mml-parser`) — `.hmml` text → AST with byte
  spans and recoverable diagnostics, the semantic checks, the diff-link checks
  against the checkout, and the CLI.
- **`mml-lsp`** (package `hobby-mml-lsp`) — the `tower-lsp` server wrapping it.

## How it differs from the work `mml-lsp`

The two share a grammar shape and most of their code; this copy was taken from
the work MML v2 rewrite and changed in two places.

| | work (`.mml`) | hobby (`.hmml`) |
|---|---|---|
| delta lives in | `notes/{topic}/{task}/aspect_{id}/` | `notes/{project}/{topic}/{task}/`, or its `aspects/{name}/` |
| checkout | `~/worktrees/{topic}/{task}[/aspects/{id}]/{repo}` | `~/fun_aspects/{p}/{t}/{task}/{name}`, then `~/fun_worktrees/{p}/{t}/{task}` |
| fallback | `~/projects/{repo}` | `~/fun_repos/{repo}` |
| Bevy | — | `in <Schedule>` / `on <Event>` on a unit; an ECS role on a `## data` type |
| `head` | a commit | a commit, or `worktree` for an uncommitted change |

The task directory *is* the checkout here, so the first two candidates are only
tried when the header's `repo` names the delta's project.

The **Bevy layer** is deliberately light (grammar §6.1, §7.1). A unit names the
schedule that runs it or the event its observer takes, and a type may say it is
a `component`, `resource`, `event`, `message` or `state`. Neither is checked
against the code or against each other: they are there so a changed system or a
changed ECS shape reads as one, not to type-check access. The v2 dialect's
world sections, access blocks and role checks did that for a spec maintained by
hand, and a delta derived from the diff has no use for them.

## hobby-mml-parser CLI

```bash
hobby-mml-parser <delta.hmml>           # grammar checks (same as --check)
hobby-mml-parser <delta.hmml> --check   # file:line:col: severity [code]: message
hobby-mml-parser <delta.hmml> --lint    # --check + diff links against the checkout
hobby-mml-parser <delta.hmml> --json    # { "ast": ..., "diagnostics": [...] }
hobby-mml-parser --version              # hobby-mml-parser 3.x for this grammar
```

Exit status: `0` ok; `1` any error, or under `--lint` any error or warning; `2`
usage or I/O error. Info diagnostics never gate. The codes and their severities
are grammar §11.

`head: worktree` is the one hobby addition to the header: the change is not
committed yet, so the head side is the checkout's files on disk, untracked ones
included, and the checkout is the candidate that holds `base`. `/task:build`
derives its delta before GATE 2, when nothing is committed.

Each `@ <file>[:<line>]` is matched by whole path components from the end, first
among the files `git diff --name-only base...head` lists, then among all files
at that side's commit; the line is checked on the `head` side for context, `+`
and `~` lines and on the `base` side for `-` lines.

## hobby-mml-lsp server

- **diagnostics** — parse, semantic and diff-link checks on open and change.
- **goto-definition** — `@ api X::f` → the `fn` line; `^a` in `@ guards` → the
  anchored line; a type or module name in prose or in `on <Event>` → its
  definition; `@ file:line` → that line in the checkout. The head side opens the
  working file when the checkout's `HEAD` is `head`, otherwise a copy of
  `git show head:path` under `$XDG_CACHE_HOME/hobby-mml-lsp/<sha>/<path>`;
  `-` lines open the `base` side the same way.
- **hover** — an `@ api` ref shows the signature and its module; a `~` line
  shows both sentences in full; a type shows its block, role included.
- **document symbols** — intent, each data type with its role and fields, each
  unit with its trigger and anchors, each api module with its fns, the tests.
- **references** — an anchor and the `@ guards` naming it; an api fn and the
  steps linking it; a type or module name and every mention of it.
- **completion** — section headers, missing header keys, `Module::fn` after
  `@ api `, anchors after `@ guards `, ECS roles at the start of a `## data`
  line, and control-flow keywords at the start of a step.

## Helix routing

The extension keeps the two dialects apart: `.hmml` is its own language and
`.mml` stays the work one. The Helix fork's `languages.toml` carries it:

```toml
[language-server]
hobby-mml-lsp = { command = "hobby-mml-lsp" }

[[language]]
name = "hmml"
scope = "source.hmml"
file-types = ["hmml"]
language-servers = ["hobby-mml-lsp"]
```

Highlighting is `tree-sitter-hmml`, under the Helix fork's `grammars/`, with its
queries in `runtime/queries/hmml/`.

## Install

```bash
just install-mml     # installs hobby-mml-lsp, hobby-mml-parser
```

## Develop

```bash
cargo test                    # parser, CLI, git-link and LSP tests
cargo clippy --all-targets
cargo fmt
```

The git-link tests build throwaway repos under a temporary `$HOME`, so they need
`git` on `PATH` and nothing else.

## Known limitations

- **`tree-sitter-hmml` still parses v2.** It lives in the Helix fork and has to
  be ported from the work `tree-sitter-mml` v2 grammar, with the trigger and the
  role added; until then a v3 delta highlights only partly. Diagnostics are
  unaffected — they come from this server.
- Work `mml-parser` fixes are taken by hand. The shared code is close enough
  that most apply as they are, outside `git.rs` and the two Bevy additions.
