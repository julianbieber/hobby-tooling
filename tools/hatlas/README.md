# hatlas

| Field        | Value                                                                  |
| ------------ | ---------------------------------------------------------------------- |
| Type         | CLI                                                                    |
| Binary       | `hatlas`                                                               |
| Owner        | @julianbieber                                                          |
| Dependencies | `cargo` and a nightly toolchain (rustdoc JSON is nightly-only)         |
| Status       | experimental                                                           |

## Description

A Bevy workspace's plugins, components and resources as linked local HTML pages, built
from rustdoc JSON.

```
hatlas <repo dir> [--out <dir>]
```

- **Builds the target workspace.** A run executes its build scripts and proc macros, as
  `cargo build` does. Do not point it at a repo you would not build.
- **Toolchain.** `rustc -vV` runs in the repo; a nightly there is used as is. Otherwise every
  cargo call is `+nightly`, into `<target>/hatlas-nightly` so the cache never collides with the site.
- **Output.** `<target>/hatlas/index.html`, or `--out`. The index lists public modules, plugins,
  components and resources per crate. A module's page lists its own items, private ones included,
  and each plugin, component and resource has a page with its rustdoc.
- **Links are a hard check.** A page that links to something the site does not contain fails the
  run. Doc-comment links that resolve to nothing in the site are shown as plain text.
- **The swap.** The site is written next to its destination, link-checked, then replaces the old
  one. A directory without the `.hatlas-site` marker is never replaced; the run stops with an error
  before compiling anything.
- **Supported `format_version`s:** 57 and 61. Any other fails naming the version. To add one, add a
  toolchain to the `fixtures` recipe in `justfile`, run `just fixtures`, and add the version to
  `SUPPORTED` in `src/rustdoc.rs`.

Not yet: systems, events, dependency pages and the graph overview.
