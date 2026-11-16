//! A language server that turns `claude:` comments in source files into code
//! actions and symbols, and hands their instructions to a Claude Code session in
//! a Zellij pane.

pub mod actions;
pub mod bridge;
pub mod server;
pub mod symbols;
