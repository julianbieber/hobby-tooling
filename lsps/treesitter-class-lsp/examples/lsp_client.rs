//! Tiny synchronous LSP client that drives `treesitter-class-lsp` over stdio
//! and issues `initialize` → `initialized` → `workspace/symbol` → `shutdown` →
//! `exit`. Prints the symbol response to stdout.

use std::io::{BufRead, BufReader, Write};

fn recv_response_for_id<R: BufRead>(r: &mut R, id: u64) -> anyhow::Result<String> {
    loop {
        let msg = recv(r)?;
        let needle = format!("\"id\":{id}");
        if msg.contains(&needle) {
            return Ok(msg);
        }
        // Skip notifications (window/logMessage etc).
    }
}
use std::process::{Command, Stdio};

fn main() -> anyhow::Result<()> {
    let binary = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/debug/treesitter-class-lsp".into());
    let root = std::env::args().nth(2).unwrap_or_else(|| "/tmp/tslsp_test".into());
    let query = std::env::args().nth(3).unwrap_or_else(|| "Foo".into());

    let mut child = Command::new(&binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    let root_uri = format!("file://{root}");

    // 1. initialize
    send(
        &mut stdin,
        &format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"processId":null,"rootUri":"{root_uri}","capabilities":{{}}}}}}"#
        ),
    )?;
    let _init_resp = recv_response_for_id(&mut reader, 1)?;

    // 2. initialized (notification; triggers the index build)
    send(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    )?;

    // Give the background indexer some time.
    std::thread::sleep(std::time::Duration::from_millis(1500));

    // 3. workspace/symbol
    send(
        &mut stdin,
        &format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"workspace/symbol","params":{{"query":"{query}"}}}}"#
        ),
    )?;
    let sym_resp = recv_response_for_id(&mut reader, 2)?;
    println!("workspace/symbol response:\n{sym_resp}");

    // 4. shutdown + exit — we don't bother blocking on the shutdown response
    // since tower-lsp flushes it before processing `exit`.
    send(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}"#,
    )?;
    send(&mut stdin, r#"{"jsonrpc":"2.0","method":"exit"}"#)?;
    drop(stdin);

    let _ = child.wait()?;
    Ok(())
}

fn send<W: Write>(w: &mut W, msg: &str) -> std::io::Result<()> {
    write!(w, "Content-Length: {}\r\n\r\n{}", msg.len(), msg)?;
    w.flush()
}

fn recv<R: BufRead>(r: &mut R) -> anyhow::Result<String> {
    let mut len = 0usize;
    let mut header = String::new();
    loop {
        header.clear();
        r.read_line(&mut header)?;
        if header == "\r\n" || header.is_empty() {
            break;
        }
        if let Some(rest) = header.strip_prefix("Content-Length:") {
            len = rest.trim().parse()?;
        }
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    Ok(String::from_utf8(body)?)
}
