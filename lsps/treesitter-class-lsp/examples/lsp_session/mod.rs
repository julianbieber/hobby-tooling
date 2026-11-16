//! A synchronous LSP client that drives `treesitter-class-lsp` over stdio.

use std::io::{self, BufRead, BufReader, Write};
use std::num::ParseIntError;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const INDEX_DEADLINE: Duration = Duration::from_secs(60);
const MESSAGE_BACKLOG: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("cannot start the server {binary}")]
    Spawn {
        binary: String,
        #[source]
        source: io::Error,
    },
    #[error("the server's stdin or stdout is not piped")]
    NotPiped,
    #[error("talking to the server failed")]
    Io(#[from] io::Error),
    #[error("the server sent a Content-Length that is not a number")]
    BadLength(#[from] ParseIntError),
    #[error("the server sent a message that is not JSON")]
    Malformed(#[from] serde_json::Error),
    #[error("the server closed its stdout")]
    Closed,
    #[error("index not complete after {}s", INDEX_DEADLINE.as_secs())]
    IndexTimeout,
}

pub struct Session {
    child: Child,
    stdin: ChildStdin,
    messages: Receiver<Value>,
    reader: JoinHandle<()>,
    next_id: u64,
}

impl Session {
    pub fn start(binary: &str) -> Result<Self, SessionError> {
        let mut child = Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|source| SessionError::Spawn {
                binary: binary.to_string(),
                source,
            })?;
        let stdin = child.stdin.take().ok_or(SessionError::NotPiped)?;
        let mut stdout = BufReader::new(child.stdout.take().ok_or(SessionError::NotPiped)?);
        let (tx, messages) = mpsc::sync_channel(MESSAGE_BACKLOG);
        let reader = std::thread::spawn(move || {
            while let Ok(msg) = recv(&mut stdout) {
                if tx.send(msg).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            messages,
            reader,
            next_id: 1,
        })
    }

    pub fn notify(&mut self, method: &str, params: Value) -> Result<(), SessionError> {
        send(
            &mut self.stdin,
            &json!({"jsonrpc": "2.0", "method": method, "params": params}),
        )
    }

    /// Sends a request and waits for its answer, discarding every message
    /// that arrives before it. The answer is the response's `result`.
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, SessionError> {
        let id = self.next_id;
        self.next_id += 1;
        send(
            &mut self.stdin,
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
        )?;
        loop {
            let msg = self.messages.recv().map_err(|_| SessionError::Closed)?;
            if msg.get("id") == Some(&json!(id)) {
                return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }

    /// Blocks until the server reports that its initial index is built.
    pub fn wait_until_indexed(&self) -> Result<(), SessionError> {
        let deadline = Instant::now() + INDEX_DEADLINE;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.messages.recv_timeout(left) {
                Ok(msg) => {
                    let text = msg
                        .pointer("/params/message")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if msg.get("method").and_then(Value::as_str) == Some("window/logMessage")
                        && text.contains("treesitter-class-lsp: indexed")
                    {
                        return Ok(());
                    }
                }
                Err(RecvTimeoutError::Timeout) => return Err(SessionError::IndexTimeout),
                Err(RecvTimeoutError::Disconnected) => return Err(SessionError::Closed),
            }
        }
    }

    /// Shuts the server down and waits for it to exit.
    pub fn finish(mut self) -> Result<(), SessionError> {
        self.request("shutdown", Value::Null)?;
        self.notify("exit", Value::Null)?;
        drop(self.stdin);
        self.child.wait()?;
        drop(self.messages);
        if self.reader.join().is_err() {
            return Err(SessionError::Closed);
        }
        Ok(())
    }
}

fn send<W: Write>(w: &mut W, msg: &Value) -> Result<(), SessionError> {
    let body = msg.to_string();
    write!(w, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    w.flush()?;
    Ok(())
}

fn recv<R: BufRead>(r: &mut R) -> Result<Value, SessionError> {
    let mut len = 0usize;
    let mut header = String::new();
    loop {
        header.clear();
        if r.read_line(&mut header)? == 0 {
            return Err(SessionError::Closed);
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
