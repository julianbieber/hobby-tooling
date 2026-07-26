//! LSP that completes Claude Code slash-commands / skills and jumps to their
//! source files. Its primary target is the Ctrl+G prompt buffer
//! (`<tmpdir>/claude-<uid>/claude-prompt-*.md`); it also activates on the
//! skill/command source files under any `.claude/` directory.

pub mod frontmatter;
pub mod index;
pub mod name;
pub mod server;
pub mod words;
