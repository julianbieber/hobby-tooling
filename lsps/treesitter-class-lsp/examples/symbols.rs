//! Drives `treesitter-class-lsp` over stdio to answer one
//! `textDocument/documentSymbol` request and print the tree it returns, so the
//! document symbol picker can be checked without an editor.
//!
//! Usage: `symbols <root> <file>`, with `<file>` relative to `<root>`. Prints
//! `documentSymbolProvider: true|false` from the initialize result, then one
//! row per symbol, depth-first and indented two spaces per level:
//! `<kind> <name> :<line>` with a 1-based line. A flat answer prints every row
//! at depth zero. The server binary is `$TSLSP_BIN`, default
//! `target/debug/treesitter-class-lsp`.

mod cli;
mod lsp_session;

use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::{Value, json};

use cli::{init_logging, report};
use lsp_session::{Session, SessionError};

#[derive(Debug, thiserror::Error)]
enum SymbolsError {
    #[error("usage: symbols <root> <file>")]
    Usage,
    #[error("cannot resolve the root {}", path.display())]
    Root {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot read {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Session(#[from] SessionError),
}

fn main() -> ExitCode {
    init_logging("info");
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => report(&e),
    }
}

fn run() -> Result<(), SymbolsError> {
    let mut args = std::env::args().skip(1);
    let (Some(root), Some(file)) = (args.next(), args.next()) else {
        return Err(SymbolsError::Usage);
    };

    let root = std::fs::canonicalize(&root).map_err(|source| SymbolsError::Root {
        path: PathBuf::from(&root),
        source,
    })?;
    let path = root.join(&file);
    let uri = format!("file://{}", path.display());
    let binary =
        std::env::var("TSLSP_BIN").unwrap_or_else(|_| "target/debug/treesitter-class-lsp".into());

    let mut session = Session::start(&binary)?;
    let initialized = session.request(
        "initialize",
        json!({"processId": null, "rootUri": format!("file://{}", root.display()), "capabilities": {}}),
    )?;
    let advertised = initialized
        .pointer("/capabilities/documentSymbolProvider")
        .is_some_and(|provider| !provider.is_null() && provider != &json!(false));
    println!("documentSymbolProvider: {advertised}");
    session.notify("initialized", json!({}))?;
    session.wait_until_indexed()?;

    let text = std::fs::read_to_string(&path).map_err(|source| SymbolsError::Read {
        path: path.clone(),
        source,
    })?;
    session.notify(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": uri, "languageId": "plain", "version": 1, "text": text}}),
    )?;
    let answer = session.request(
        "textDocument/documentSymbol",
        json!({"textDocument": {"uri": uri}}),
    )?;
    for row in rows(&answer) {
        println!("{row}");
    }
    Ok(session.finish()?)
}

fn rows(answer: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for symbol in answer.as_array().into_iter().flatten() {
        push_rows(symbol, 0, &mut out);
    }
    out
}

fn push_rows(symbol: &Value, depth: usize, out: &mut Vec<String>) {
    let selection = symbol
        .get("selectionRange")
        .filter(|range| range.is_object())
        .or_else(|| symbol.pointer("/location/range"));
    out.push(format!(
        "{}{} {} :{}",
        "  ".repeat(depth),
        kind_name(symbol.get("kind").and_then(Value::as_u64).unwrap_or(0)),
        symbol.get("name").and_then(Value::as_str).unwrap_or(""),
        selection
            .and_then(|range| range.pointer("/start/line"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
            + 1
    ));
    for child in symbol
        .get("children")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        push_rows(child, depth + 1, out);
    }
}

fn kind_name(kind: u64) -> String {
    match kind {
        5 => "class".to_string(),
        6 => "method".to_string(),
        10 => "enum".to_string(),
        11 => "interface".to_string(),
        12 => "function".to_string(),
        23 => "struct".to_string(),
        26 => "typeParameter".to_string(),
        other => other.to_string(),
    }
}
