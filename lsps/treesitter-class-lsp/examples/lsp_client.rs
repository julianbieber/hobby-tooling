//! Tiny synchronous LSP client that drives `treesitter-class-lsp` over stdio
//! and issues `initialize` → `initialized` → `workspace/symbol` → `shutdown` →
//! `exit`. Prints the symbol response to stdout.
//!
//! Usage: `lsp_client [<binary>] [<root>] [<query>]`.

mod cli;
mod lsp_session;

use std::process::ExitCode;

use serde_json::json;

use cli::{init_logging, report};
use lsp_session::{Session, SessionError};

fn main() -> ExitCode {
    init_logging("info");
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => report(&e),
    }
}

fn run() -> Result<(), SessionError> {
    let mut args = std::env::args().skip(1);
    let binary = args
        .next()
        .unwrap_or_else(|| "target/debug/treesitter-class-lsp".into());
    let root = args.next().unwrap_or_else(|| "/tmp/tslsp_test".into());
    let query = args.next().unwrap_or_else(|| "Foo".into());

    let mut session = Session::start(&binary)?;
    session.request(
        "initialize",
        json!({"processId": null, "rootUri": format!("file://{root}"), "capabilities": {}}),
    )?;
    session.notify("initialized", json!({}))?;
    session.wait_until_indexed()?;

    let symbols = session.request("workspace/symbol", json!({"query": query}))?;
    println!("workspace/symbol response:");
    println!("{symbols}");

    session.finish()
}
