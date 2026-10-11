# hobby-mml-lsp

| Field        | Value                                              |
| ------------ | -------------------------------------------------- |
| Type         | lsp + CLI                                          |
| Binaries     | `hobby-mml-lsp`, `hobby-mml-parser`                |
| Owner        | @julianbieber                                      |
| Dependencies | tree-sitter-mml grammar (from the work tooling repo) |
| Status       | experimental                                       |

## Description

Fork of the work `mml-lsp` for hobby specs, now carrying its **own grammar**
([`grammar.md`](grammar.md), HMML v2). Language server (completion, hover,
goto-definition, references, document symbols, diagnostics) plus the
`hobby-mml-parser --lint` CLI the `hmml-*` skills shell out to.

## Why a fork rather than a shared binary

Originally: spec→source path resolution.

| | work | hobby |
|---|---|---|
| spec | `notes/{topic}/{task}/<unit>.hmml` | `notes/{project}/{topic}/{task}/<unit>.hmml` |
| checkout | `~/worktrees/{topic}/{task}/{repo}` | `~/fun_worktrees/{project}/{topic}/{task}` |
| fallback | `~/projects/<repo>` | `~/fun_repos/<repo>` |

The hobby layout has **no repo level** — a task is one repo, so the task
directory *is* the checkout.

Since **v2 the fork is a genuine dialect, not a path tweak.** Hobby projects are
overwhelmingly Bevy games, and MML v1's founding assumption — a unit is a
function with `in: T` / `out: T` — does not hold in an ECS, where data flows
through the World. v2 adds `kind: plugin` / `kind: system` and the sections
`## components`, `## resources`, `## events`, `## systems`, `## access`, and
makes the legal section set a function of `kind:`. The classic kinds are
untouched, so every v1 spec still parses.

v2.1 hands `## data` back to the Bevy kinds for the shapes that carry **no** ECS
role — a field type, an enum a component is built from. `in:`/`out:` stay
illegal there, and a `## data` name in an access list is a role mismatch: the
section is a type namespace in this dialect, not an interface.

v2.2 lets a spec **point at a type instead of restating it**. `struct Wave @
src/waves/mod.rs` says the type is already there: the body becomes optional, and
when present it is partial — it lists the members the spec asserts, and anything
unlisted stays as it is. (v2.2's inline `+` / `-` member markers still parse but
are legacy; v2.3's `# +` / `# -` are the one change spelling.) Because the target is
real Rust on disk, the reference is checked: the type must exist, its
`#[derive(…)]` must match the section's role (filing a `Resource` under
`## components` is the classic spec error, and now a warning), and
goto-definition lands on the struct. `use extern avian3d::prelude: Collider`
covers a dependency's type, which nothing local can verify — a second keyword,
because the guarantee differs and a reader should not have to resolve a path to
tell which they are looking at. Reading the Rust is `mml-parser/src/rust_source.rs`,
via the vendored `tree-sitter-rust` grammar rather than the Helix grammar cache,
so `hobby-mml-parser --lint` keeps working on a machine with no built grammars.

v2.3 makes the spec hold **both** versions of the unit: the baseline it started
from, and the change written on top of it as `# +` / `# -` comment markers
(grammar §10). See [Change annotations](#change-annotations-grammar-10) below for
the checks; the reason it is worth a convention is that the delta then survives
being committed, and `/hmml-translate` is told what to add and remove instead of
inferring it from git.

v2.4 adds the **detail layer**: `kind: module`, whose third section is `## api` —
one `fn` signature per public function, the bullets describing it nested beneath
(grammar §11). An `@ spec` ref may name one of those entries
(`wave-table::entry_for`, `wave-rng::WaveRng::next_u32`), and the two ref
failures get different severities on purpose: a missing target file stays a
warning because the spec may be planned and not yet written, while a member the
target file does not define is an error — the file is right there to check the
name against. `Future`, `Result`, `Either` and `Tuple` join the containers so a
signature can stay faithful to the code it is read beside.

That divergence ends any prospect of re-merging with the work binary, and makes
upstream `mml-parser` cherry-picks a manual job — a cost accepted deliberately,
since the Bevy dialect has no business in a work repo.

## Grammar

[`grammar.md`](grammar.md) in this directory is the source of truth, and the
`hmml-*` skills point at it. The work `01-language-spec/grammar.md` describes v1
and no longer governs anything here.

The always-parsing examples are the parser's own fixtures under
`mml-parser/tests/fixtures/examples/` — they are compiled into the test suite, so
docs and parser cannot drift:

| Fixture | Shows |
|---|---|
| `enemy-waves.hmml` | `kind: plugin` — world sections, role-less `## data`, systems, ordering, `marker`, `where:` |
| `spawn-enemies.hmml` | `kind: system` — `## access`, cross-spec imports into `## data` and a world section, query exclusion, a Rust reference with a partial body carrying a `# +` annotation, `use extern` |
| `get-map-tile.hmml` | `kind: endpoint` — `method`, param sources |
| `mesh-terrain-chunks.hmml` | `kind: workflow` — `Queue`/`Stream` |
| `shared-data-imports.hmml` / `cross-spec-refs.hmml` | `use` imports, `@ spec` refs |
| `wave-table.hmml` / `wave-rng.hmml` | `kind: module` — `## api`, a method entry, a `unit::Type::method` member ref between the two |

### Change annotations (grammar §10)

A spec carries a change as `# +` / `# - <text>` comment markers on top of the
baseline — the version the change starts from. The markers stay after the change
lands, so the spec keeps representing it. They are comments, so no other pass
sees them — and the property that makes the convention work, **baseline
recoverability**, is therefore checked in `mml-parser/src/annotations.rs`:
dropping every `# +` block and un-commenting every `# -` line must give back the
baseline spec, coherent on its own.

`unrecoverable-baseline` is the check that earns its keep. The reconstructed
baseline is re-parsed and re-validated, and an **error it has that the annotated
spec does not** is reported against the line that caused it — typically a `# +`
on something the baseline itself already needs, which means the two versions the
file is supposed to hold are not both coherent. The rest are shape checks:
`empty-change-removal` (a `# -` that kept no text, so the baseline is gone),
`empty-change-addition` (a `# +` with nothing beneath it), and
`redundant-change-bullet` (a removed line that kept the bullet the marker's own
`-` stands in for).

All four are **warnings**, so `--lint` gates on them. The work copy makes
`redundant-change-bullet` an info hint instead, because its `--lint` does not
gate on info severity; this one does gate on any diagnostic at all, so an info
hint here would gate anyway while claiming not to. A doubled bullet is a
one-character fix, so gating on it costs nothing.

## Helix routing

Two binaries on `PATH` would fight, since Helix maps a language to one server.
The file extension keeps them apart: `.hmml` is its own language, `.mml` stays
the work one. No path globs, no block ordering. The Helix fork's own
`languages.toml` carries it:

```toml
[language-server]
hobby-mml-lsp = { command = "hobby-mml-lsp" }

[[language]]
name = "hmml"
scope = "source.hmml"
file-types = ["hmml"]
language-servers = ["hobby-mml-lsp"]
```

There is no `grammar =` line: HMML has its own grammar, `tree-sitter-hmml`,
which lives in the Helix fork under `grammars/`. It used to
borrow `grammar = "mml"`, which quietly made highlighting depend on the work
checkout being present.

Helix resolves highlight queries by **language name**, not grammar, so
`runtime/queries/hmml/highlights.scm` must exist; it is committed in the Helix
fork. (It used to contain only `; inherits: mml` — a second hidden
dependency on the work grammar.)

Verify with `hx --health hmml` and `hx --health mml` — each should bind its own
server, and both should show a tree-sitter parser and highlight queries.

## Installation

```bash
just install-mml     # installs hobby-mml-lsp, hobby-mml-parser
```

## Known Limitations

- **The grammar knows v2's shapes, not v2's rules.** `tree-sitter-hmml` now
  parses the Bevy sections, `marker Name` and the access block, so they highlight
  distinctly rather than as generic lines. It still validates nothing: which
  sections a `kind:` may use, whether a name in `reads:` is a component or a
  resource, whether a schedule label exists — all of that comes from
  `hobby-mml-parser`. A spec that parses cleanly can still be full of diagnostics.
- Upstream `mml-parser` fixes must be cherry-picked by hand, and v2 has moved
  the two apart far enough that this is real work rather than a patch apply.
- The built-in Bevy name lists (`BUILTIN_COMPONENTS`, `BUILTIN_RESOURCES`,
  `BUILTIN_EVENTS` in `mml-parser/src/validate.rs`) are hand-maintained and
  cover the common engine types, not all of Bevy. An unlisted engine type reads
  as `undefined-access-item`; the fix is to add it to the list, not to declare
  it locally.
