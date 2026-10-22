//! Drives `brs lsp` over stdio like an editor would.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    notifications: Vec<Value>,
}

impl Client {
    fn start() -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_brs"))
            .arg("lsp")
            .env("BRS_LSP_NO_CHECK", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Client { child, stdin, stdout, next_id: 1, notifications: Vec::new() }
    }

    fn write(&mut self, v: Value) {
        let t = v.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", t.len(), t).unwrap();
        self.stdin.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let mut len = 0;
        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line).unwrap();
            let t = line.trim_end();
            if t.is_empty() {
                break;
            }
            if let Some(v) = t.strip_prefix("Content-Length:") {
                len = v.trim().parse().unwrap();
            }
        }
        let mut buf = vec![0; len];
        self.stdout.read_exact(&mut buf).unwrap();
        serde_json::from_slice(&buf).unwrap()
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.write(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let m = self.read();
            if m.get("id") == Some(&json!(id)) {
                return m.get("result").cloned().unwrap_or_else(|| m.clone());
            }
            self.notifications.push(m);
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.write(json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    fn wait_diagnostics(&mut self, uri: &str) -> Value {
        loop {
            if let Some(i) = self.notifications.iter().position(|n| n["method"] == "textDocument/publishDiagnostics" && n["params"]["uri"] == uri) {
                return self.notifications.remove(i)["params"]["diagnostics"].clone();
            }
            let m = self.read();
            self.notifications.push(m);
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

const SRC: &str = r#"/// A point.
data class Point(x: i32, y: i32) {
    fun moved(dx: i32): Point = Point(x + dx, y)
}

class Counter(var n: i32 = 0) {
    fun inc() {
        n += 1
    }
}

fun add(a: i32, b: i32 = 1): i32 = a + b

fun main() {
    val p = Point(1, 2)
    val q = p.moved(3)
    println(add(q.x, ))
    var c = Counter()
    c.inc()
}
"#;

fn pos(src: &str, needle: &str, delta: usize) -> Value {
    let off = src.find(needle).unwrap() + delta;
    let before = &src[..off];
    let line = before.matches('\n').count();
    let col = off - before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    json!({"line": line, "character": col})
}

#[test]
fn language_server_features() {
    let dir = std::env::temp_dir().join(format!("brs-lsp-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file: PathBuf = dir.join("demo.brs");
    std::fs::write(&file, SRC).unwrap();
    let uri = format!("file://{}", std::fs::canonicalize(&file).unwrap().display());

    let mut c = Client::start();
    let init = c.request("initialize", json!({"rootUri": format!("file://{}", dir.display()), "capabilities": {}}));
    assert_eq!(init["capabilities"]["hoverProvider"], true);
    c.notify("initialized", json!({}));
    c.notify("textDocument/didOpen", json!({"textDocument": {"uri": uri, "languageId": "brs", "version": 1, "text": SRC}}));
    let diags = c.wait_diagnostics(&uri);
    assert_eq!(diags.as_array().unwrap().len(), 0, "{diags}");

    // hover on a data class shows the brs signature, docs and generated Rust
    let h = c.request("textDocument/hover", json!({"textDocument": {"uri": uri}, "position": pos(SRC, "Point(1, 2)", 1)}));
    let text = h["contents"]["value"].as_str().unwrap();
    assert!(text.contains("data class Point(x: i32, y: i32)"), "{text}");
    assert!(text.contains("A point."), "{text}");
    assert!(text.contains("pub struct Point"), "{text}");

    // go to definition of a method through a typed local
    let d = c.request("textDocument/definition", json!({"textDocument": {"uri": uri}, "position": pos(SRC, "moved(3)", 1)}));
    assert_eq!(d[0]["range"]["start"]["line"], 2, "{d}");

    // completion after `p.` lists Point members
    let comp = c.request("textDocument/completion", json!({"textDocument": {"uri": uri}, "position": pos(SRC, "p.moved(3)", 2)}));
    let labels: Vec<String> = comp["items"].as_array().unwrap().iter().map(|i| i["label"].as_str().unwrap().to_string()).collect();
    assert!(labels.contains(&"moved".to_string()) && labels.contains(&"x".to_string()), "{labels:?}");

    // signature help inside add(…)
    let sig = c.request("textDocument/signatureHelp", json!({"textDocument": {"uri": uri}, "position": pos(SRC, "add(q.x, )", 9)}));
    assert_eq!(sig["activeParameter"], 1, "{sig}");
    assert!(sig["signatures"][0]["label"].as_str().unwrap().contains("fun add(a: i32, b: i32 = …): i32"), "{sig}");

    // inlay hints show the inferred receiver of `inc`
    let hints = c.request("textDocument/inlayHint", json!({"textDocument": {"uri": uri}, "range": {"start": {"line": 0, "character": 0}, "end": {"line": 30, "character": 0}}}));
    let labels: Vec<String> = hints.as_array().unwrap().iter().map(|h| h["label"].as_str().unwrap().to_string()).collect();
    assert!(labels.contains(&"&mut self".to_string()), "{labels:?}");
    assert!(labels.contains(&"&self, ".to_string()), "{labels:?}");

    // document symbols
    let syms = c.request("textDocument/documentSymbol", json!({"textDocument": {"uri": uri}}));
    let names: Vec<&str> = syms.as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["Point", "Counter", "add", "main"]);

    // rename a local
    let edit = c.request("textDocument/rename", json!({"textDocument": {"uri": uri}, "position": pos(SRC, "val q", 4), "newName": "moved_point"}));
    assert_eq!(edit["changes"][&uri].as_array().unwrap().len(), 2, "{edit}");

    // formatting
    let messy = "fun main(){\nval x=1\n}\n";
    c.notify("textDocument/didChange", json!({"textDocument": {"uri": uri, "version": 2}, "contentChanges": [{"text": messy}]}));
    let _ = c.wait_diagnostics(&uri);
    let fmt = c.request("textDocument/formatting", json!({"textDocument": {"uri": uri}, "options": {"tabSize": 4, "insertSpaces": true}}));
    assert_eq!(fmt[0]["newText"], "fun main() {\n    val x = 1\n}\n");

    // syntax errors are reported immediately
    c.notify("textDocument/didChange", json!({"textDocument": {"uri": uri, "version": 3}, "contentChanges": [{"text": "fun main() {\n    val x = (1 + \n}\n"}]}));
    let diags = c.wait_diagnostics(&uri);
    assert!(!diags.as_array().unwrap().is_empty());
    assert_eq!(diags[0]["source"], "brs");

    let _ = c.request("shutdown", Value::Null);
    c.notify("exit", Value::Null);
    let _ = std::fs::remove_dir_all(&dir);
}
