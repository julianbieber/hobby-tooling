//! Talking to language servers: one client per process, and the manager that
//! picks and caches them.

pub mod client;
pub mod manager;
pub mod types;

pub use client::{LspClient, LspError};
pub use manager::{LspManager, ManagerError, clear_global_cache};
pub use types::{symbol_kind_to_string, symbol_to_searchable, symbol_to_string};
