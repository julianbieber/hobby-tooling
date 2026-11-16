//! `textDocument/definition` on method calls end to end over stdio, against a
//! fixture workspace where two classes define the same method names. Skipped
//! silently when the helix runtime isn't available or resolves no Python
//! language.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use serde_json::{Value, json};
use treesitter_class_lsp::languages::LanguageRegistry;

const CLIENT_PY: &str = "class Widget:\n    def get_state(self):\n        pass\n\n    def run(self):\n        return self.get_state()\n\n\nclass Gadget:\n    def get_state(self):\n        pass\n\n    def run(self):\n        pass\n";
const APP_PY: &str = "import client\n\ndef go():\n    w = client.Widget()\n    w.run()\n    g: Gadget = make()\n    g.get_state()\n    unknown.get_state()\n";

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Option<Self> {
        let registry = match LanguageRegistry::load() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("skipping: helix runtime not available: {e}");
                return None;
            }
        };
        if registry.resolve(Path::new("probe.py")).is_none() {
            eprintln!("skipping: no python language configured");
            return None;
        }
        let root = std::env::temp_dir().join(format!("tslsp-recv-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("client.py"), CLIENT_PY).unwrap();
        std::fs::write(root.join("app.py"), APP_PY).unwrap();
        Some(Self {
            root: std::fs::canonicalize(root).unwrap(),
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Server {
    child: Child,
    stdin: ChildStdin,
    messages: Receiver<Value>,
    next_id: u64,
}

impl Server {
    fn start(root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_treesitter-class-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let (tx, messages) = mpsc::channel();
        std::thread::spawn(move || {
            while let Some(msg) = recv(&mut reader) {
                if tx.send(msg).is_err() {
                    break;
                }
            }
        });
        let mut server = Self {
            child,
            stdin,
            messages,
            next_id: 1,
        };
        server.request(
            "initialize",
            json!({"processId": null, "rootUri": format!("file://{}", root.display()), "capabilities": {}}),
        );
        server.notify("initialized", json!({}));
        server.wait_until_indexed();
        server
    }

    fn notify(&mut self, method: &str, params: Value) {
        send(
            &mut self.stdin,
            &json!({"jsonrpc": "2.0", "method": method, "params": params}),
        );
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        send(
            &mut self.stdin,
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
        );
        loop {
            let msg = self.messages.recv_timeout(Duration::from_secs(60)).unwrap();
            if msg.get("id") == Some(&json!(id)) {
                return msg["result"].clone();
            }
        }
    }

    fn wait_until_indexed(&self) {
        loop {
            let msg = self.messages.recv_timeout(Duration::from_secs(60)).unwrap();
            let text = msg["params"]["message"].as_str().unwrap_or("");
            if msg["method"] == "window/logMessage"
                && text.contains("treesitter-class-lsp: indexed")
            {
                return;
            }
        }
    }

    fn open(&mut self, path: &Path, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": file_uri(path), "languageId": "python", "version": 1, "text": text}}),
        );
    }

    fn definition(&mut self, path: &Path, line: u32, character: u32) -> Value {
        self.request(
            "textDocument/definition",
            json!({"textDocument": {"uri": file_uri(path)}, "position": {"line": line, "character": character}}),
        )
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn file_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn send<W: Write>(w: &mut W, msg: &Value) {
    let body = msg.to_string();
    write!(w, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    w.flush().unwrap();
}

fn recv<R: BufRead>(r: &mut R) -> Option<Value> {
    let mut len = 0usize;
    let mut header = String::new();
    loop {
        header.clear();
        if r.read_line(&mut header).ok()? == 0 {
            return None;
        }
        if header == "\r\n" {
            break;
        }
        if let Some(rest) = header.strip_prefix("Content-Length:") {
            len = rest.trim().parse().ok()?;
        }
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn only_location(answer: &Value) -> (String, u64, u64) {
    let locations = answer.as_array().expect("an array of locations");
    assert_eq!(locations.len(), 1, "expected one location: {answer}");
    let start = &locations[0]["range"]["start"];
    let uri = locations[0]["uri"].as_str().unwrap();
    let file = uri.rsplit('/').next().unwrap().to_string();
    (
        file,
        start["line"].as_u64().unwrap(),
        start["character"].as_u64().unwrap(),
    )
}

fn every_location(answer: &Value) -> Vec<(String, u64, u64)> {
    answer
        .as_array()
        .expect("an array of locations")
        .iter()
        .map(|location| {
            let start = &location["range"]["start"];
            let file = location["uri"]
                .as_str()
                .unwrap()
                .rsplit('/')
                .next()
                .unwrap();
            (
                file.to_string(),
                start["line"].as_u64().unwrap(),
                start["character"].as_u64().unwrap(),
            )
        })
        .collect()
}

// Pins S2: `self.get_state()` in `Widget` answers `Widget`'s method alone, though `Gadget` defines the same name.
#[test]
fn self_call_resolves_to_the_enclosing_classes_method() {
    let Some(fx) = Fixture::new("self_call") else {
        return;
    };
    let mut server = Server::start(&fx.root);
    let client = fx.root.join("client.py");
    server.open(&client, CLIENT_PY);

    let answer = server.definition(&client, 5, 22);

    assert_eq!(only_location(&answer), ("client.py".to_string(), 1, 8));
}

// Pins S10: a variable assigned from `client.Widget()` resolves `w.run()` to `Widget`'s method alone, though `Gadget` defines `run` too.
#[test]
fn variable_assigned_from_a_constructor_resolves_to_that_classes_method() {
    let Some(fx) = Fixture::new("constructor") else {
        return;
    };
    let mut server = Server::start(&fx.root);
    let app = fx.root.join("app.py");
    server.open(&app, APP_PY);

    let answer = server.definition(&app, 4, 6);

    assert_eq!(only_location(&answer), ("client.py".to_string(), 4, 8));
}

// Pins that an annotated variable resolves its method call to the annotated class's method alone.
#[test]
fn annotated_variable_resolves_to_its_types_method() {
    let Some(fx) = Fixture::new("annotated") else {
        return;
    };
    let mut server = Server::start(&fx.root);
    let app = fx.root.join("app.py");
    server.open(&app, APP_PY);

    let answer = server.definition(&app, 6, 6);

    assert_eq!(only_location(&answer), ("client.py".to_string(), 9, 8));
}

// Pins that a receiver the file never types still answers every definition of the name, which is what keeps by-name navigation working.
#[test]
fn an_unresolvable_receiver_still_answers_by_name() {
    let Some(fx) = Fixture::new("unresolved") else {
        return;
    };
    let mut server = Server::start(&fx.root);
    let app = fx.root.join("app.py");
    server.open(&app, APP_PY);

    let answer = server.definition(&app, 7, 12);

    assert_eq!(
        every_location(&answer),
        [
            ("client.py".to_string(), 1, 8),
            ("client.py".to_string(), 9, 8)
        ]
    );
}
