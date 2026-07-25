# hobby-mml-index

| Field        | Value                                  |
| ------------ | -------------------------------------- |
| Type         | CLI                                    |
| Binary       | `hobby-mml-index`                      |
| Owner        | @julianbieber                          |
| Dependencies | `hobby-mml-parser` (workspace crate)   |
| Status       | experimental                           |

## Description

Walks the hobby notes tree, parses every `.mml`, and writes an `index.md`
summarizing units by system — plus warnings for cross-spec refs and imports that
point at specs which do not exist.

Fork of the work `mml-index`. Two changes: the default spec root is
`~/fun_repos/hobby-mimisbrunnr/notes` (hobby specs live per-task inside the notes
tree, not in a standalone spec repo), and `source.repo` resolves against
`~/fun_repos`.

## Usage

```bash
hobby-mml-index                 # default root: the hobby notes tree
hobby-mml-index <spec-root>     # or an explicit root
```

Output goes to `<root>/index.md`, which is gitignored in the notes repo since it
is generated.

## Installation

```bash
just install-mml
```
