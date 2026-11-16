//! A language server that indexes class-like definitions, functions and methods
//! across a workspace with Helix's tree-sitter grammars.

pub mod definition;
pub mod document_symbol;
pub mod fuzzy;
pub mod grammars;
pub mod indexer;
pub mod languages;
pub mod references;
pub mod server;
pub mod worktree;
