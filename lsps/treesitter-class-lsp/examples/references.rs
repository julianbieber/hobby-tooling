//! Drives `treesitter-class-lsp` over stdio to answer one
//! `textDocument/references` request and print where the name is used, so a
//! references answer can be checked without an editor.
//!
//! Usage: `references <root> <file> <line> <col> [--no-declaration]`, with
//! `<file>` relative to `<root>` and `<line>`/`<col>` 1-based. It prints
//! `referencesProvider: true|false` as the server advertised it, then one
//! `<file>:<line>:<col>` row per location, or `none`. `--no-declaration` sends
//! `includeDeclaration: false`. The server binary is `$TSLSP_BIN`, default
//! `target/debug/treesitter-class-lsp`.

mod cli;
mod lsp_session;

use std::io;
use std::num::ParseIntError;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

use cli::{init_logging, report};
use lsp_session::{Session, SessionError};

const USAGE: &str = "usage: references <root> <file> <line> <col> [--no-declaration]";

#[derive(Debug, thiserror::Error)]
enum ReferencesError {
    #[error("{USAGE}")]
    Usage,
    #[error("{name} is not a number")]
    NotANumber {
        name: &'static str,
        #[source]
        source: ParseIntError,
    },
    #[error("unexpected argument {0}")]
    UnexpectedArgument(String),
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

struct Args {
    root: PathBuf,
    file: String,
    line: u32,
    col: u32,
    include_declaration: bool,
}

impl Args {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, ReferencesError> {
        let (Some(root), Some(file), Some(line), Some(col)) =
            (args.next(), args.next(), args.next(), args.next())
        else {
            return Err(ReferencesError::Usage);
        };
        let line = line.parse().map_err(|source| ReferencesError::NotANumber {
            name: "<line>",
            source,
        })?;
        let col = col.parse().map_err(|source| ReferencesError::NotANumber {
            name: "<col>",
            source,
        })?;
        let include_declaration = match args.next().as_deref() {
            Some("--no-declaration") => false,
            Some(other) => return Err(ReferencesError::UnexpectedArgument(other.to_string())),
            None => true,
        };
        Ok(Self {
            root: PathBuf::from(root),
            file,
            line,
            col,
            include_declaration,
        })
    }
}

fn main() -> ExitCode {
    init_logging("info");
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => report(&e),
    }
}

fn run() -> Result<(), ReferencesError> {
    let args = Args::parse(std::env::args().skip(1))?;
    let root = std::fs::canonicalize(&args.root).map_err(|source| ReferencesError::Root {
        path: args.root.clone(),
        source,
    })?;
    let path = root.join(&args.file);
    let uri = format!("file://{}", path.display());
    let binary =
        std::env::var("TSLSP_BIN").unwrap_or_else(|_| "target/debug/treesitter-class-lsp".into());

    let mut session = Session::start(&binary)?;
    let initialized = session.request(
        "initialize",
        json!({"processId": null, "rootUri": format!("file://{}", root.display()), "capabilities": {}}),
    )?;
    let advertised = initialized
        .pointer("/capabilities/referencesProvider")
        .is_some_and(|provider| !provider.is_null() && provider != &json!(false));
    println!("referencesProvider: {advertised}");
    session.notify("initialized", json!({}))?;
    session.wait_until_indexed()?;

    let disk = std::fs::read_to_string(&path).map_err(|source| ReferencesError::Read {
        path: path.clone(),
        source,
    })?;
    session.notify(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": uri, "languageId": "plain", "version": 1, "text": disk}}),
    )?;
    let answer = session.request(
        "textDocument/references",
        json!({
            "textDocument": {"uri": uri},
            "position": {"line": args.line.saturating_sub(1), "character": args.col.saturating_sub(1)},
            "context": {"includeDeclaration": args.include_declaration},
        }),
    )?;
    println!("{}", describe(&root, &answer));
    Ok(session.finish()?)
}

fn describe(root: &Path, answer: &Value) -> String {
    let lines: Vec<String> = answer
        .as_array()
        .map(|items| items.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter_map(|loc| {
            let uri = loc.get("uri")?.as_str()?;
            let start = loc.pointer("/range/start")?;
            let path = PathBuf::from(uri.strip_prefix("file://")?);
            let shown = path.strip_prefix(root).unwrap_or(&path);
            Some(format!(
                "{}:{}:{}",
                shown.display(),
                start.get("line")?.as_u64()? + 1,
                start.get("character")?.as_u64()? + 1
            ))
        })
        .collect();
    if lines.is_empty() {
        "none".to_string()
    } else {
        lines.join("\n")
    }
}
