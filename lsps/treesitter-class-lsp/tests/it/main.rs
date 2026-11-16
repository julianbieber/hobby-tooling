//! End-to-end tests of the indexer and the language server against real helix
//! grammars, each skipped silently when the helix runtime is not available.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod definition;
mod document_symbols;
mod kinds;
mod receiver;
mod references;
mod smoke;
