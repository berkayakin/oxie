//! Runs cargo and maps rustc diagnostics in generated code back to .brs sources.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use brs_compiler::SourceMap;
use serde_json::Value;

pub struct BuildResult {
    pub success: bool,
    pub executables: Vec<PathBuf>,
    pub errors: usize,
    pub warnings: usize,
}

/// A rustc diagnostic mapped to a source location.
#[derive(Clone, Debug)]
pub struct MappedDiag {
    pub level: String,
    pub code: Option<String>,
    pub message: String,
    pub file: PathBuf,
    /// zero-based
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub label: Option<String>,
    pub notes: Vec<String>,
    /// Whether the location came from a .brs source map.
    pub from_brs: bool,
}

#[derive(Default)]
pub struct Mapper {
    maps: HashMap<PathBuf, Option<SourceMap>>,
}

impl Mapper {
    fn map_for(&mut self, file: &Path) -> Option<&SourceMap> {
        self.maps
            .entry(file.to_path_buf())
            .or_insert_with(|| {
                let p = PathBuf::from(format!("{}.map", file.display()));
                std::fs::read_to_string(p).ok().and_then(|t| SourceMap::parse(&t))
            })
            .as_ref()
    }

    /// Maps a rustc span (1-based) to a source location.
    pub fn map_span(&mut self, root: &Path, span: &Value) -> Option<(PathBuf, u32, u32, u32, u32, bool)> {
        let file = span.get("file_name")?.as_str()?;
        let mut path = PathBuf::from(file);
        if path.is_relative() {
            path = root.join(path);
        }
        let ls = span.get("line_start")?.as_u64()? as u32 - 1;
        let cs = span.get("column_start")?.as_u64()? as u32 - 1;
        let le = span.get("line_end")?.as_u64()? as u32 - 1;
        let ce = span.get("column_end")?.as_u64()? as u32 - 1;
        if let Some(map) = self.map_for(&path) {
            let source = map.source.clone();
            let (sl, sc) = map.lookup(ls, cs)?;
            let (el, ec) = if le == ls {
                // keep the width when the span stays on one generated line
                (sl, sc + (ce.saturating_sub(cs)).max(1))
            } else {
                (sl, sc + 1)
            };
            return Some((source, sl, sc, el, ec, true));
        }
        if file.ends_with("__brs.rs") {
            return None;
        }
        Some((path, ls, cs, le, ce, false))
    }

    pub fn map_message(&mut self, root: &Path, msg: &Value) -> Option<MappedDiag> {
        let level = msg.get("level")?.as_str()?.to_string();
        let message = msg.get("message")?.as_str()?.to_string();
        let code = msg.get("code").and_then(|c| c.get("code")).and_then(|c| c.as_str()).map(|s| s.to_string());
        let spans = msg.get("spans")?.as_array()?;
        // prefer the deepest macro expansion site that is in user code
        let primary = spans.iter().find(|s| s.get("is_primary").and_then(|p| p.as_bool()).unwrap_or(false)).or(spans.first())?;
        let span = expansion_site(primary);
        let (file, line, col, end_line, end_col, from_brs) = self.map_span(root, span)?;
        let label = primary.get("label").and_then(|l| l.as_str()).map(|s| s.to_string());
        let mut notes = Vec::new();
        if let Some(children) = msg.get("children").and_then(|c| c.as_array()) {
            for c in children {
                let lvl = c.get("level").and_then(|l| l.as_str()).unwrap_or("note");
                let m = c.get("message").and_then(|l| l.as_str()).unwrap_or("");
                if m.is_empty() {
                    continue;
                }
                let mut text = format!("{}: {}", lvl, m);
                if let Some(sp) = c.get("spans").and_then(|s| s.as_array()).and_then(|s| s.first()) {
                    if let Some(sugg) = sp.get("suggested_replacement").and_then(|s| s.as_str()) {
                        if !sugg.is_empty() && sugg.len() < 80 && !sugg.contains('\n') {
                            text.push_str(&format!(": `{}`", sugg));
                        }
                    }
                }
                notes.push(text);
            }
        }
        Some(MappedDiag { level, code, message, file, line, col, end_line, end_col, label, notes, from_brs })
    }
}

fn expansion_site(span: &Value) -> &Value {
    let mut cur = span;
    // For errors inside macros (println! etc.), use the call site.
    while let Some(exp) = cur.get("expansion").filter(|e| !e.is_null()) {
        let file = cur.get("file_name").and_then(|f| f.as_str()).unwrap_or("");
        if file.starts_with('<') || file.contains("/rustlib/") || file.contains("/.cargo/registry/") {
            match exp.get("span") {
                Some(s) => cur = s,
                None => break,
            }
        } else {
            break;
        }
    }
    cur
}

pub fn color() -> bool {
    std::io::stderr().is_terminal() && std::env::var("NO_COLOR").is_err()
}

pub fn render(d: &MappedDiag, root: &Path) -> String {
    let c = color();
    let (red, yellow, blue, bold, reset) = if c { ("\x1b[31m", "\x1b[33m", "\x1b[34m", "\x1b[1m", "\x1b[0m") } else { ("", "", "", "", "") };
    let lc = if d.level == "error" { red } else { yellow };
    let shown = d.file.strip_prefix(root).unwrap_or(&d.file).display().to_string();
    let text = std::fs::read_to_string(&d.file).unwrap_or_default();
    let line_text = text.lines().nth(d.line as usize).unwrap_or("");
    let num = (d.line + 1).to_string();
    let pad = " ".repeat(num.len());
    let width = if d.end_line == d.line { d.end_col.saturating_sub(d.col).max(1) } else { 1 } as usize;
    let width = width.min(line_text.chars().count().saturating_sub(d.col as usize).max(1));
    let code = d.code.as_ref().map(|c| format!("[{}]", c)).unwrap_or_default();
    let mut out = format!("{bold}{lc}{}{}{reset}{bold}: {}{reset}\n", d.level, code, d.message);
    out += &format!("{pad}{blue}-->{reset} {}:{}:{}\n", shown, d.line + 1, d.col + 1);
    out += &format!("{pad} {blue}|{reset}\n");
    out += &format!("{blue}{num} |{reset} {}\n", line_text);
    out += &format!("{pad} {blue}|{reset} {}{lc}{}{reset}", " ".repeat(d.col as usize), "^".repeat(width));
    if let Some(l) = &d.label {
        out += &format!(" {lc}{}{reset}", l);
    }
    out.push('\n');
    for n in &d.notes {
        out += &format!("{pad} {blue}={reset} {}\n", n);
    }
    out
}

/// Runs `cargo <args> --message-format=json-diagnostic-rendered-ansi` and prints mapped diagnostics.
pub fn cargo_json(root: &Path, args: &[String], quiet: bool) -> std::io::Result<BuildResult> {
    let fmt = if color() { "--message-format=json-diagnostic-rendered-ansi" } else { "--message-format=json" };
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cmd.args(args).arg(fmt).current_dir(root).stdout(Stdio::piped());
    if quiet {
        cmd.arg("--quiet");
    }
    if color() {
        cmd.arg("--color=always");
    }
    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().unwrap();
    let mut mapper = Mapper::default();
    let mut seen = HashSet::new();
    let mut result = BuildResult { success: false, executables: Vec::new(), errors: 0, warnings: 0 };
    for line in BufReader::new(stdout).lines() {
        let line = line?;
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            println!("{}", line);
            continue;
        };
        match v.get("reason").and_then(|r| r.as_str()) {
            Some("compiler-message") => {
                // only diagnostics for the user's package (not path dependencies like brs-build)
                let manifest = v.get("manifest_path").and_then(|m| m.as_str()).map(PathBuf::from);
                if manifest.as_deref().map_or(false, |m| m.parent() != Some(root)) {
                    continue;
                }
                let Some(msg) = v.get("message") else { continue };
                let level = msg.get("level").and_then(|l| l.as_str()).unwrap_or("");
                if level == "failure-note" {
                    continue;
                }
                let rendered = match mapper.map_message(root, msg) {
                    Some(d) if d.from_brs => render(&d, root),
                    Some(_) | None => {
                        let r = msg.get("rendered").and_then(|r| r.as_str()).unwrap_or("").to_string();
                        // summary lines like "aborting due to ..." have no spans
                        if msg.get("spans").and_then(|s| s.as_array()).map_or(true, |s| s.is_empty()) {
                            let m = msg.get("message").and_then(|m| m.as_str()).unwrap_or("");
                            if m.starts_with("aborting due to") || m.contains("warnings emitted") || m.contains("warning emitted") {
                                continue;
                            }
                        }
                        r
                    }
                };
                if rendered.trim().is_empty() || !seen.insert(rendered.clone()) {
                    continue;
                }
                match level {
                    "error" => result.errors += 1,
                    "warning" => result.warnings += 1,
                    _ => {}
                }
                eprintln!("{}", rendered);
            }
            Some("compiler-artifact") => {
                if let Some(exe) = v.get("executable").and_then(|e| e.as_str()) {
                    result.executables.push(PathBuf::from(exe));
                }
            }
            Some("build-finished") => {
                result.success = v.get("success").and_then(|s| s.as_bool()).unwrap_or(false);
            }
            _ => {}
        }
    }
    let status = child.wait()?;
    result.success = result.success && status.success();
    Ok(result)
}

/// Collects mapped diagnostics without printing (used by the language server).
pub fn check_collect(root: &Path) -> std::io::Result<Vec<MappedDiag>> {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["check", "--message-format=json", "--quiet"])
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    let mut mapper = Mapper::default();
    let mut diags = Vec::new();
    let mut seen = HashSet::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
            continue;
        }
        let manifest = v.get("manifest_path").and_then(|m| m.as_str()).map(PathBuf::from);
        if manifest.as_deref().map_or(false, |m| m.parent() != Some(root)) {
            continue;
        }
        if let Some(msg) = v.get("message") {
            if let Some(d) = mapper.map_message(root, msg) {
                let key = format!("{}:{}:{}:{}", d.file.display(), d.line, d.col, d.message);
                if seen.insert(key) {
                    diags.push(d);
                }
            }
        }
    }
    Ok(diags)
}
