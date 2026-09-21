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

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use serde_json::{Value, json};

const INDEX_DEADLINE: Duration = Duration::from_secs(60);

struct Session {
    child: Child,
    stdin: ChildStdin,
    messages: Receiver<Value>,
    next_id: u64,
}

impl Session {
    fn start(binary: &str) -> anyhow::Result<Self> {
        let mut child = Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("spawning {binary}"))?;
        let stdin = child.stdin.take().unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let (tx, messages) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(msg) = recv(&mut reader) {
                if tx.send(msg).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            messages,
            next_id: 1,
        })
    }

    fn notify(&mut self, method: &str, params: Value) -> anyhow::Result<()> {
        send(&mut self.stdin, &json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    fn request(&mut self, method: &str, params: Value) -> anyhow::Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        send(
            &mut self.stdin,
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
        )?;
        loop {
            let msg = self.messages.recv().context("server closed its stdout")?;
            if msg.get("id") == Some(&json!(id)) {
                return Ok(msg["result"].clone());
            }
        }
    }

    fn wait_until_indexed(&self) -> anyhow::Result<()> {
        let deadline = Instant::now() + INDEX_DEADLINE;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.messages.recv_timeout(left) {
                Ok(msg) => {
                    let text = msg["params"]["message"].as_str().unwrap_or("");
                    if msg["method"] == "window/logMessage"
                        && text.contains("treesitter-class-lsp: indexed")
                    {
                        return Ok(());
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    bail!("index not complete after {}s", INDEX_DEADLINE.as_secs())
                }
                Err(RecvTimeoutError::Disconnected) => bail!("server closed its stdout"),
            }
        }
    }

    fn finish(mut self) -> anyhow::Result<()> {
        self.request("shutdown", Value::Null)?;
        self.notify("exit", Value::Null)?;
        drop(self.stdin);
        self.child.wait()?;
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (Some(root), Some(file)) = (args.next(), args.next()) else {
        bail!("usage: symbols <root> <file>");
    };

    let root = std::fs::canonicalize(&root).with_context(|| format!("root {root}"))?;
    let path = root.join(&file);
    let uri = format!("file://{}", path.display());
    let binary = std::env::var("TSLSP_BIN").unwrap_or_else(|_| "target/debug/treesitter-class-lsp".into());

    let mut session = Session::start(&binary)?;
    let initialized = session.request(
        "initialize",
        json!({"processId": null, "rootUri": format!("file://{}", root.display()), "capabilities": {}}),
    )?;
    let provider = &initialized["capabilities"]["documentSymbolProvider"];
    println!("documentSymbolProvider: {}", !provider.is_null() && provider != &json!(false));
    session.notify("initialized", json!({}))?;
    session.wait_until_indexed()?;

    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    session.notify(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": uri, "languageId": "plain", "version": 1, "text": text}}),
    )?;
    let answer = session.request("textDocument/documentSymbol", json!({"textDocument": {"uri": uri}}))?;
    for row in rows(&answer) {
        println!("{row}");
    }
    session.finish()
}

fn rows(answer: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for symbol in answer.as_array().into_iter().flatten() {
        push_rows(symbol, 0, &mut out);
    }
    out
}

fn push_rows(symbol: &Value, depth: usize, out: &mut Vec<String>) {
    let selection = if symbol["selectionRange"].is_object() {
        &symbol["selectionRange"]
    } else {
        &symbol["location"]["range"]
    };
    out.push(format!(
        "{}{} {} :{}",
        "  ".repeat(depth),
        kind_name(symbol["kind"].as_u64().unwrap_or(0)),
        symbol["name"].as_str().unwrap_or(""),
        selection["start"]["line"].as_u64().unwrap_or(0) + 1
    ));
    for child in symbol["children"].as_array().into_iter().flatten() {
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

fn send<W: Write>(w: &mut W, msg: &Value) -> anyhow::Result<()> {
    let body = msg.to_string();
    write!(w, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    w.flush()?;
    Ok(())
}

fn recv<R: BufRead>(r: &mut R) -> anyhow::Result<Value> {
    let mut len = 0usize;
    let mut header = String::new();
    loop {
        header.clear();
        if r.read_line(&mut header)? == 0 {
            bail!("eof");
        }
        if header == "\r\n" {
            break;
        }
        if let Some(rest) = header.strip_prefix("Content-Length:") {
            len = rest.trim().parse()?;
        }
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}
