//! `hobby-mml-lsp`: a `tower-lsp` language server for HMML v3 deltas, built on
//! the `hobby-mml-parser` crate.

pub mod analysis;
pub mod server;

pub use server::serve_stdio;
