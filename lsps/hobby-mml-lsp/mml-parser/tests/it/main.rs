//! Integration tests of the parser library and the `hobby-mml-parser` binary.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod diagnostics;
mod fixture;
mod git_links;
