//! End-to-end smoke test: spawn the real binary, speak LSP over its stdio, and
//! check completion + goto-definition against a synthetic `~/.claude` tree.

use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl Server {
    fn spawn(home: &Path) -> Server {
        Server::spawn_with_config_dir(home, None)
    }

    /// `config_dir` becomes `$CLAUDE_CONFIG_DIR`; `None` clears it, so the default
    /// `$HOME/.claude` applies regardless of what the test runner inherited.
    fn spawn_with_config_dir(home: &Path, config_dir: Option<&Path>) -> Server {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_claude-skills-lsp"));
        cmd.env("HOME", home);
        match config_dir {
            Some(d) => cmd.env("CLAUDE_CONFIG_DIR", d),
            None => cmd.env_remove("CLAUDE_CONFIG_DIR"),
        };
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn claude-skills-lsp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Server {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn send(&mut self, msg: &Value) {
        let body = serde_json::to_string(msg).unwrap();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        self.wait_for_response(id)
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(&json!({"jsonrpc":"2.0","method":method,"params":params}));
    }

    /// Read framed messages until the response with `id` arrives, skipping any
    /// server-originated notifications.
    fn wait_for_response(&mut self, id: i64) -> Value {
        loop {
            let msg = self.read_message();
            if msg.get("id").and_then(Value::as_i64) == Some(id) {
                return msg;
            }
        }
    }

    fn read_message(&mut self) -> Value {
        // Read headers up to the blank line.
        let mut header = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            self.stdout.read_exact(&mut byte).expect("read header byte");
            header.push(byte[0]);
            if header.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let header = String::from_utf8(header).unwrap();
        let len: usize = header
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .expect("Content-Length header")
            .trim()
            .parse()
            .unwrap();
        let mut body = vec![0u8; len];
        self.stdout.read_exact(&mut body).expect("read body");
        serde_json::from_slice(&body).unwrap()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn write_file(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

#[test]
fn completion_and_goto_over_stdio() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    // A known user command.
    let source = home.join(".claude/commands/known-name.md");
    write_file(&source, "---\ndescription: A known command\n---\nbody\n");
    let canonical_source = std::fs::canonicalize(&source).unwrap();

    // The Ctrl+G prompt buffer referencing it.
    let buffer = home.join("claude-prompt-test.md");
    let buffer_text = "/known-name";
    write_file(&buffer, buffer_text);

    // A plain markdown file that must NOT trigger the server.
    let plain = home.join("notes.md");
    write_file(&plain, "/known-name");

    let mut server = Server::spawn(home);

    server.request(
        "initialize",
        json!({
            "processId": null,
            "rootUri": uri(home),
            "capabilities": {}
        }),
    );
    server.notify("initialized", json!({}));

    // Open the prompt buffer.
    server.notify(
        "textDocument/didOpen",
        json!({"textDocument": {
            "uri": uri(&buffer),
            "languageId": "markdown",
            "version": 1,
            "text": buffer_text
        }}),
    );
    // Open the plain note too.
    server.notify(
        "textDocument/didOpen",
        json!({"textDocument": {
            "uri": uri(&plain),
            "languageId": "markdown",
            "version": 1,
            "text": "/known-name"
        }}),
    );

    // Completion at end of "/known-name" (char 11).
    let comp = server.request(
        "textDocument/completion",
        json!({
            "textDocument": {"uri": uri(&buffer)},
            "position": {"line": 0, "character": 11}
        }),
    );
    let labels = completion_labels(&comp);
    assert!(
        labels.iter().any(|l| l == "known-name"),
        "expected 'known-name' in completions, got {labels:?}"
    );

    // Goto-definition on the same token → the canonical source file.
    let def = server.request(
        "textDocument/definition",
        json!({
            "textDocument": {"uri": uri(&buffer)},
            "position": {"line": 0, "character": 11}
        }),
    );
    let def_path = definition_path(&def).expect("definition returned a location");
    assert_eq!(
        std::fs::canonicalize(&def_path).unwrap(),
        canonical_source,
        "definition should point at the command source"
    );

    // The plain note is out of scope → completion returns null.
    let plain_comp = server.request(
        "textDocument/completion",
        json!({
            "textDocument": {"uri": uri(&plain)},
            "position": {"line": 0, "character": 11}
        }),
    );
    assert!(
        plain_comp.get("result").map(Value::is_null).unwrap_or(true),
        "plain markdown must not yield completions, got {plain_comp}"
    );

    server.request("shutdown", json!(null));
    server.notify("exit", json!(null));
}

/// `CLAUDE_CONFIG_DIR` moves the user tier off `$HOME/.claude` entirely.
///
/// This is what makes one binary serve two Claude accounts: in a hobby session the
/// variable points at `~/.claude-hobby`, and the work commands under `~/.claude`
/// must not leak in — offering the wrong account's slash commands is the exact bug
/// the variable exists to prevent.
#[test]
fn config_dir_override_replaces_the_user_tier() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    // The account NOT selected. Its command must not be offered.
    write_file(
        &home.join(".claude/commands/work-only.md"),
        "---\ndescription: Work account\n---\n",
    );
    // The account selected via CLAUDE_CONFIG_DIR, deliberately outside $HOME/.claude.
    let hobby = home.join("elsewhere/.claude-hobby");
    write_file(
        &hobby.join("commands/hobby-only.md"),
        "---\ndescription: Hobby account\n---\n",
    );

    // The workspace root must not be $HOME: `<root>/.claude` is the *project* tier,
    // so rooting at $HOME would pull `.claude/commands` in by that route and prove
    // nothing about the user tier.
    let workspace = home.join("workspace");
    let buffer = workspace.join("claude-prompt-test.md");
    let buffer_text = "/";
    write_file(&buffer, buffer_text);

    let mut server = Server::spawn_with_config_dir(home, Some(&hobby));
    server.request(
        "initialize",
        json!({"processId": null, "rootUri": uri(&workspace), "capabilities": {}}),
    );
    server.notify("initialized", json!({}));
    server.notify(
        "textDocument/didOpen",
        json!({"textDocument": {
            "uri": uri(&buffer),
            "languageId": "markdown",
            "version": 1,
            "text": buffer_text
        }}),
    );

    let comp = server.request(
        "textDocument/completion",
        json!({
            "textDocument": {"uri": uri(&buffer)},
            "position": {"line": 0, "character": 1}
        }),
    );
    let labels = completion_labels(&comp);
    assert!(
        labels.iter().any(|l| l == "hobby-only"),
        "expected the CLAUDE_CONFIG_DIR account's command, got {labels:?}"
    );
    assert!(
        !labels.iter().any(|l| l == "work-only"),
        "$HOME/.claude must not leak in when CLAUDE_CONFIG_DIR is set, got {labels:?}"
    );

    server.request("shutdown", json!(null));
    server.notify("exit", json!(null));
}

fn completion_labels(response: &Value) -> Vec<String> {
    let result = &response["result"];
    let items = if result.is_array() {
        result.as_array().cloned().unwrap_or_default()
    } else {
        result["items"].as_array().cloned().unwrap_or_default()
    };
    items
        .iter()
        .filter_map(|i| i["label"].as_str().map(String::from))
        .collect()
}

fn definition_path(response: &Value) -> Option<PathBuf> {
    let result = &response["result"];
    let uri = if result.is_array() {
        result[0]["uri"].as_str()?.to_string()
    } else {
        result["uri"].as_str()?.to_string()
    };
    let path = uri.strip_prefix("file://").unwrap_or(&uri);
    Some(PathBuf::from(path))
}
