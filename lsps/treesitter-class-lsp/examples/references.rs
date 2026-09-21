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

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
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
    let (Some(root), Some(file), Some(line), Some(col)) =
        (args.next(), args.next(), args.next(), args.next())
    else {
        bail!("usage: references <root> <file> <line> <col> [--no-declaration]");
    };
    let line: u32 = line.parse().context("<line> is not a number")?;
    let col: u32 = col.parse().context("<col> is not a number")?;
    let include_declaration = match args.next().as_deref() {
        Some("--no-declaration") => false,
        Some(other) => bail!("unexpected argument {other}"),
        None => true,
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
    let advertised = !initialized["capabilities"]["referencesProvider"].is_null()
        && initialized["capabilities"]["referencesProvider"] != json!(false);
    println!("referencesProvider: {advertised}");
    session.notify("initialized", json!({}))?;
    session.wait_until_indexed()?;

    let disk = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    session.notify(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": uri, "languageId": "plain", "version": 1, "text": disk}}),
    )?;
    let answer = session.request(
        "textDocument/references",
        json!({
            "textDocument": {"uri": uri},
            "position": {"line": line.saturating_sub(1), "character": col.saturating_sub(1)},
            "context": {"includeDeclaration": include_declaration},
        }),
    )?;
    println!("{}", describe(&root, &answer));
    session.finish()
}

fn describe(root: &Path, answer: &Value) -> String {
    let lines: Vec<String> = answer
        .as_array()
        .map(|items| items.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter_map(|loc| {
            let uri = loc["uri"].as_str()?;
            let start = &loc["range"]["start"];
            let path = PathBuf::from(uri.strip_prefix("file://")?);
            let shown = path.strip_prefix(root).unwrap_or(&path);
            Some(format!(
                "{}:{}:{}",
                shown.display(),
                start["line"].as_u64()? + 1,
                start["character"].as_u64()? + 1
            ))
        })
        .collect();
    if lines.is_empty() {
        "none".to_string()
    } else {
        lines.join("\n")
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
