//! Tree-sitter extraction of a repository's symbols into a markdown map and a
//! queryable code graph, and the MCP server that answers questions about them.

pub mod builtin_queries;
pub mod containment;
pub mod docstring;
pub mod extractor;
pub mod git;
pub mod gitignore;
pub mod graph;
pub mod graph_cache;
pub mod graph_query;
pub mod mcp;
pub mod md_render;
pub mod refresh;
