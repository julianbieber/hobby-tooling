//! An MCP server that discovers and runs `just` recipes across a tree of
//! projects, asking the installed `just` for every recipe's shape.

pub mod config;
pub mod discover;
pub mod dump;
pub mod models;
pub mod params;
pub mod run;
pub mod server;

pub use server::JustMcpServer;
