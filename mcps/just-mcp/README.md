# just_mcp

| Field        | Value                                                     |
| ------------ | --------------------------------------------------------- |
| Type         | mcp (Rust source + binary)                                |
| Owner        | @julianbieber                                             |
| Dependencies | Rust (stable, edition 2024), `just` on PATH, `mise` (opt) |
| Status       | experimental                                              |

## Description

MCP server for **discovering and running `just` recipes across a multi-project
tree**. It walks a root and finds every justfile beneath it, at any depth, so it
works whether Claude Code was started in one repo or in a directory holding
several.

Four tools:

- `just_projects` — walk a root directory and list every justfile (path, working
  directory, recipe count, default recipe).
- `just_recipes` — list one justfile's recipes (including submodules) with their
  doc comments, parameters (name / required / default / variadic / help),
  groups, dependencies and aliases.
- `just_show` — print a single recipe's source via `just --show`.
- `just_run` — run a recipe with arguments (by name or positional), capturing
  stdout, stderr, exit code and duration. Supports `dry_run`.

Introspection is delegated to `just` itself (`just --dump --dump-format json`),
so parameter metadata always matches the installed `just` version.

Full architecture reference lives in `CLAUDE.md` in this folder (auto-loaded by
Claude when working here).

## Installation

### Build & install

```bash
cargo install --path ~/fun_repos/hobby-tooling/mcps/just-mcp
```

Installs the `just_mcp` binary (stdio MCP server) into `~/.cargo/bin/`.

### Wire into Claude Code

Add to `~/.claude/settings.json` (or wherever you configure MCP servers):

```json
{
  "mcpServers": {
    "just-mcp": {
      "command": "/home/<you>/.cargo/bin/just_mcp"
    }
  }
}
```

The server discovers justfiles relative to its **current working directory** by
default — Claude Code launches MCP servers with the workspace root as cwd, which
for a hobby task is `~/fun_worktrees/PROJECT/TOPIC/TASK`, the worktree itself.
Override per call with the `root` argument to `just_projects`, or globally via
config.

### Config (optional)

Create `~/.config/just_mcp.toml` to override defaults:

```toml
# Discovery roots (default: the server's working directory)
roots = ["/home/you/fun_worktrees/my-project/my-topic/my-task"]
# Extra directory names to skip while walking
ignore = [".git", "target", "node_modules", ".venv", "build"]
# Max directory depth to descend
max_depth = 10
# Wrap recipe runs in `mise exec` when a mise config is nearby (default: true)
use_mise = true
# Default kill timeout for just_run, in seconds
default_timeout_secs = 300
```

## Notes

- **Discovery** respects `.gitignore` files *inside* the tree but deliberately
  ignores parent/global gitignore sources, so a broad `~/.gitignore` rule can't
  hide your projects.
- **mise**: when a `mise.toml` / `.tool-versions` is found at or above a
  justfile's directory, `just_run` wraps the invocation in
  `mise exec --cd <dir> -- just …` so project tool versions (Java, Node, Python)
  activate — the same activation a plain shell-out would miss.
- **Safety**: recipes run arbitrary shell code. Use `dry_run: true` to preview
  what a recipe would run before executing it.
