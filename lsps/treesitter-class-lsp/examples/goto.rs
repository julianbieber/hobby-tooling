//! Drives `treesitter-class-lsp` over stdio to answer one
//! `textDocument/definition` request and print where it lands, so a goto can
//! be checked without an editor.
//!
//! Usage: `goto <root> <file> <line> <col> [--append <text>]`, with `<file>`
//! relative to `<root>` and `<line>`/`<col>` 1-based. `--append` opens the file
//! with `<text>` added (`\n` reads as a newline) and asks, then writes it to
//! disk, sends `didSave` and asks again until a location comes back. The server
//! binary is `$TSLSP_BIN`, default `target/debug/treesitter-class-lsp`.

mod cli;
mod lsp_session;

use std::io;
use std::num::ParseIntError;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use cli::{init_logging, report};
use lsp_session::{Session, SessionError};

const SAVE_POLL_DEADLINE: Duration = Duration::from_secs(5);
const SAVE_POLL_INTERVAL: Duration = Duration::from_millis(100);
const USAGE: &str = "usage: goto <root> <file> <line> <col> [--append <text>]";

#[derive(Debug, thiserror::Error)]
enum GotoError {
    #[error("{USAGE}")]
    Usage,
    #[error("{name} is not a number")]
    NotANumber {
        name: &'static str,
        #[source]
        source: ParseIntError,
    },
    #[error("--append needs a text")]
    AppendWithoutText,
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
    #[error("cannot write {}", path.display())]
    Write {
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
    append: Option<String>,
}

impl Args {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, GotoError> {
        let (Some(root), Some(file), Some(line), Some(col)) =
            (args.next(), args.next(), args.next(), args.next())
        else {
            return Err(GotoError::Usage);
        };
        let line = line.parse().map_err(|source| GotoError::NotANumber {
            name: "<line>",
            source,
        })?;
        let col = col.parse().map_err(|source| GotoError::NotANumber {
            name: "<col>",
            source,
        })?;
        let append = match args.next().as_deref() {
            Some("--append") => Some(
                args.next()
                    .ok_or(GotoError::AppendWithoutText)?
                    .replace("\\n", "\n"),
            ),
            Some(other) => return Err(GotoError::UnexpectedArgument(other.to_string())),
            None => None,
        };
        Ok(Self {
            root: PathBuf::from(root),
            file,
            line,
            col,
            append,
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

fn run() -> Result<(), GotoError> {
    let args = Args::parse(std::env::args().skip(1))?;
    let root = std::fs::canonicalize(&args.root).map_err(|source| GotoError::Root {
        path: args.root.clone(),
        source,
    })?;
    let path = root.join(&args.file);
    let uri = format!("file://{}", path.display());
    let binary =
        std::env::var("TSLSP_BIN").unwrap_or_else(|_| "target/debug/treesitter-class-lsp".into());

    let mut session = Session::start(&binary)?;
    session.request(
        "initialize",
        json!({"processId": null, "rootUri": format!("file://{}", root.display()), "capabilities": {}}),
    )?;
    session.notify("initialized", json!({}))?;
    session.wait_until_indexed()?;

    let disk = std::fs::read_to_string(&path).map_err(|source| GotoError::Read {
        path: path.clone(),
        source,
    })?;
    let position =
        json!({"line": args.line.saturating_sub(1), "character": args.col.saturating_sub(1)});
    let ask = |session: &mut Session| {
        session.request(
            "textDocument/definition",
            json!({"textDocument": {"uri": uri}, "position": position}),
        )
    };

    let answers = match args.append {
        None => {
            open(&mut session, &uri, &disk)?;
            vec![describe(&root, &ask(&mut session)?)]
        }
        Some(text) => {
            let edited = format!("{disk}{text}");
            open(&mut session, &uri, &edited)?;
            let unsaved = format!("unsaved: {}", describe(&root, &ask(&mut session)?));

            std::fs::write(&path, &edited).map_err(|source| GotoError::Write {
                path: path.clone(),
                source,
            })?;
            session.notify(
                "textDocument/didSave",
                json!({"textDocument": {"uri": uri}}),
            )?;
            let deadline = Instant::now() + SAVE_POLL_DEADLINE;
            let mut answer = ask(&mut session)?;
            while answer.is_null() && Instant::now() < deadline {
                std::thread::sleep(SAVE_POLL_INTERVAL);
                answer = ask(&mut session)?;
            }
            vec![unsaved, format!("saved: {}", describe(&root, &answer))]
        }
    };
    for answer in answers {
        println!("{answer}");
    }
    Ok(session.finish()?)
}

fn open(session: &mut Session, uri: &str, text: &str) -> Result<(), SessionError> {
    session.notify(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": uri, "languageId": "plain", "version": 1, "text": text}}),
    )
}
fn describe(root: &Path, answer: &Value) -> String {
    let locations: Vec<&Value> = match answer {
        Value::Array(items) => items.iter().collect(),
        Value::Object(_) => vec![answer],
        _ => Vec::new(),
    };
    let lines: Vec<String> = locations
        .into_iter()
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
