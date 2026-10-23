//! Runs cargo and maps rustc diagnostics in generated code back to .oxi sources.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use oxie_compiler::SourceMap;
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
    /// Whether the location came from a .oxi source map.
    pub from_oxie: bool,
    /// Other labeled spans in the same file ("value moved here", ...).
    pub secondary: Vec<Label>,
    /// rustc's suggestions whose spans map exactly onto the .oxi source.
    pub suggestions: Vec<Suggestion>,
    /// rustc suggestions as plain text (for `use` paths and the like).
    pub raw_suggestions: Vec<String>,
    /// rustc's own message when the explanation replaced it.
    pub rust_message: Option<String>,
    /// The generated Rust text under the primary span (single-line spans only).
    pub gen_text: Option<String>,
    pub fixes: Vec<Fix>,
}

/// A labeled span on one line (zero-based, character columns).
#[derive(Clone, Debug)]
pub struct Label {
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
    pub text: String,
    /// The span mapped exactly, or an explanation placed it; others are dropped.
    pub exact: bool,
}

#[derive(Clone, Debug)]
pub struct Suggestion {
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub replacement: String,
}

/// A text replacement in a source file (zero-based lines, character columns).
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub text: String,
}

/// A quick fix: edits to the diagnostic's file.
#[derive(Clone, Debug)]
pub struct Fix {
    pub title: String,
    pub edits: Vec<Edit>,
}

#[derive(Default)]
pub struct Mapper {
    maps: HashMap<PathBuf, Option<SourceMap>>,
    texts: HashMap<PathBuf, String>,
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
        if file.ends_with("__oxie.rs") {
            return None;
        }
        Some((path, ls, cs, le, ce, false))
    }

    fn text_of(&mut self, file: &Path) -> &str {
        self.texts.entry(file.to_path_buf()).or_insert_with(|| std::fs::read_to_string(file).unwrap_or_default())
    }

    /// Maps a zero-based generated position to the exact source position, or None when the
    /// generated text between the nearest source-map mark and the position differs from the
    /// source text (so the position has no exact counterpart).
    fn map_exact(&mut self, gen: &Path, line: u32, col: u32) -> Option<(PathBuf, u32, u32)> {
        let map = self.map_for(gen)?;
        let source = map.source.clone();
        let &(gl, gc, sl, sc) = map.entries.iter().take_while(|e| (e.0, e.1) <= (line, col)).last()?;
        if gl != line {
            return None;
        }
        let gen_line: String = self.text_of(gen).lines().nth(line as usize)?.to_string();
        let piece: String = gen_line.chars().skip(gc as usize).take((col - gc) as usize).collect();
        let src_line: String = self.text_of(&source).lines().nth(sl as usize)?.to_string();
        let src_piece: String = src_line.chars().skip(sc as usize).take(piece.chars().count()).collect();
        if piece != src_piece {
            return None;
        }
        Some((source, sl, sc + piece.chars().count() as u32))
    }

    fn gen_path(root: &Path, span: &Value) -> Option<PathBuf> {
        let mut path = PathBuf::from(span.get("file_name")?.as_str()?);
        if path.is_relative() {
            path = root.join(path);
        }
        Some(path)
    }

    fn span_text(&mut self, root: &Path, span: &Value) -> Option<String> {
        let path = Self::gen_path(root, span)?;
        let n = |k: &str| span.get(k).and_then(|v| v.as_u64()).map(|v| v as usize - 1);
        let (l, c, el, ec) = (n("line_start")?, n("column_start")?, n("line_end")?, n("column_end")?);
        if l != el {
            return None;
        }
        let line = self.text_of(&path).lines().nth(l)?.to_string();
        Some(line.chars().skip(c).take(ec.saturating_sub(c)).collect())
    }

    /// Maps a span exactly (both ends), for applying suggestions.
    fn map_span_exact(&mut self, root: &Path, span: &Value) -> Option<(PathBuf, u32, u32, u32, u32)> {
        let path = Self::gen_path(root, span)?;
        let n = |k: &str| span.get(k).and_then(|v| v.as_u64()).map(|v| v as u32 - 1);
        let (f, sl, sc) = self.map_exact(&path, n("line_start")?, n("column_start")?)?;
        let (_, el, ec) = self.map_exact(&path, n("line_end")?, n("column_end")?)?;
        Some((f, sl, sc, el, ec))
    }

    pub fn map_message(&mut self, root: &Path, msg: &Value) -> Option<MappedDiag> {
        let level = msg.get("level")?.as_str()?.to_string();
        let message = msg.get("message")?.as_str()?.to_string();
        let code = msg.get("code").and_then(|c| c.get("code")).and_then(|c| c.as_str()).map(|s| s.to_string());
        let spans = msg.get("spans")?.as_array()?;
        // prefer the deepest macro expansion site that is in user code
        let primary = spans.iter().find(|s| s.get("is_primary").and_then(|p| p.as_bool()).unwrap_or(false)).or(spans.first())?;
        let span = expansion_site(primary);
        let (file, line, col, end_line, end_col, from_oxie) = self.map_span(root, span)?;
        let label = primary.get("label").and_then(|l| l.as_str()).map(|s| s.to_string());
        let gen_text = self.span_text(root, span);
        let mut secondary = Vec::new();
        for s in spans {
            if std::ptr::eq(s, primary) {
                continue;
            }
            let Some(text) = s.get("label").and_then(|l| l.as_str()).filter(|l| !l.is_empty()) else { continue };
            let site = expansion_site(s);
            if let Some((f, l, c, el, ec)) = self.map_span_exact(root, site) {
                if f == file && el == l {
                    secondary.push(Label { line: l, col: c, end_col: ec, text: text.to_string(), exact: true });
                    continue;
                }
            }
            if let Some((f, l, c, el, ec, true)) = self.map_span(root, site) {
                if f == file {
                    secondary.push(Label { line: l, col: c, end_col: if el == l { ec } else { c + 1 }, text: text.to_string(), exact: false });
                }
            }
        }
        let mut notes = Vec::new();
        let mut suggestions = Vec::new();
        let mut raw_suggestions = Vec::new();
        if let Some(children) = msg.get("children").and_then(|c| c.as_array()) {
            for c in children {
                let lvl = c.get("level").and_then(|l| l.as_str()).unwrap_or("note");
                let m = c.get("message").and_then(|l| l.as_str()).unwrap_or("");
                if m.is_empty() {
                    continue;
                }
                let mut text = format!("{}: {}", lvl, m);
                let child_spans = c.get("spans").and_then(|s| s.as_array()).cloned().unwrap_or_default();
                if let Some(sp) = child_spans.first() {
                    if let Some(sugg) = sp.get("suggested_replacement").and_then(|s| s.as_str()) {
                        if !sugg.is_empty() && sugg.len() < 80 && !sugg.contains('\n') {
                            text.push_str(&format!(": `{}`", sugg));
                        }
                    }
                }
                for sp in &child_spans {
                    let Some(sugg) = sp.get("suggested_replacement").and_then(|s| s.as_str()) else { continue };
                    raw_suggestions.push(sugg.to_string());
                    if let Some((f, l, c, el, ec)) = self.map_span_exact(root, sp) {
                        if f == file {
                            suggestions.push(Suggestion { line: l, col: c, end_line: el, end_col: ec, replacement: sugg.to_string() });
                        }
                    }
                }
                notes.push(text);
            }
        }
        let mut d = MappedDiag {
            level,
            code,
            message,
            file,
            line,
            col,
            end_line,
            end_col,
            label,
            notes,
            from_oxie,
            secondary,
            suggestions,
            raw_suggestions,
            rust_message: None,
            gen_text,
            fixes: Vec::new(),
        };
        if d.from_oxie {
            let src = self.text_of(&d.file).to_string();
            crate::explain::explain(&mut d, &src);
        }
        Some(d)
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
    let cwd = std::env::current_dir().unwrap_or_default();
    let shown = d.file.strip_prefix(&cwd).or_else(|_| d.file.strip_prefix(root)).unwrap_or(&d.file).display().to_string();
    let text = std::fs::read_to_string(&d.file).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let primary_width = if d.end_line == d.line { d.end_col.saturating_sub(d.col).max(1) } else { 1 };
    // (line, col, width, label, is_primary)
    let mut labels: Vec<(u32, u32, u32, String, bool)> = vec![(d.line, d.col, primary_width, d.label.clone().unwrap_or_default(), true)];
    for s in &d.secondary {
        labels.push((s.line, s.col, s.end_col.saturating_sub(s.col).max(1), s.text.clone(), false));
    }
    labels.sort_by_key(|l| (l.0, !l.4, l.1));
    let num_width = labels.iter().map(|l| (l.0 + 1).to_string().len()).max().unwrap_or(1);
    let pad = " ".repeat(num_width);
    let code = d.code.as_ref().map(|c| format!("[{}]", c)).unwrap_or_default();
    let mut out = format!("{bold}{lc}{}{}{reset}{bold}: {}{reset}\n", d.level, code, d.message);
    out += &format!("{pad}{blue}-->{reset} {}:{}:{}\n", shown, d.line + 1, d.col + 1);
    out += &format!("{pad} {blue}|{reset}\n");
    let mut last_line: Option<u32> = None;
    for (line, col, width, label, is_primary) in &labels {
        let line_text = lines.get(*line as usize).copied().unwrap_or("");
        if last_line != Some(*line) {
            if last_line.is_some_and(|l| *line > l + 1) {
                out += &format!("{blue}{}{reset}\n", ".".repeat(num_width.max(3)));
            }
            out += &format!("{blue}{:>num_width$} |{reset} {}\n", line + 1, line_text);
        }
        last_line = Some(*line);
        let width = (*width as usize).min(line_text.chars().count().saturating_sub(*col as usize).max(1));
        let (mark, color) = if *is_primary { ("^", lc) } else { ("-", blue) };
        out += &format!("{pad} {blue}|{reset} {}{color}{}{reset}", " ".repeat(*col as usize), mark.repeat(width));
        if !label.is_empty() {
            out += &format!(" {color}{}{reset}", label);
        }
        out.push('\n');
    }
    for n in &d.notes {
        let n = n.replace('\n', &format!("\n{pad}   "));
        out += &format!("{pad} {blue}={reset} {}\n", n);
    }
    for f in &d.fixes {
        out += &format!("{pad} {blue}={reset} {bold}fix{reset}: {}", f.title);
        if let Some(preview) = fix_preview(&lines, f) {
            out += &format!(": `{}`", preview);
        }
        out.push('\n');
    }
    if let Some(m) = &d.rust_message {
        out += &format!("{pad} {blue}={reset} rustc: {}\n", m);
    }
    out
}

/// The changed line, when a fix only touches one line.
fn fix_preview(lines: &[&str], f: &Fix) -> Option<String> {
    let line = f.edits.first()?.line;
    if f.edits.iter().any(|e| e.line != line || e.end_line != line || e.text.contains('\n')) {
        return None;
    }
    let mut chars: Vec<char> = lines.get(line as usize)?.chars().collect();
    let mut edits = f.edits.clone();
    edits.sort_by_key(|e| std::cmp::Reverse(e.col));
    for e in edits {
        let (s, t) = (e.col as usize, (e.end_col as usize).min(chars.len()));
        if s > t {
            return None;
        }
        chars.splice(s..t, e.text.chars());
    }
    Some(chars.into_iter().collect::<String>().trim().to_string())
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
                // only diagnostics for the user's package (not path dependencies like oxie-build)
                let manifest = v.get("manifest_path").and_then(|m| m.as_str()).map(PathBuf::from);
                if manifest.as_deref().is_some_and(|m| m.parent() != Some(root)) {
                    continue;
                }
                let Some(msg) = v.get("message") else { continue };
                let level = msg.get("level").and_then(|l| l.as_str()).unwrap_or("");
                if level == "failure-note" {
                    continue;
                }
                let rendered = match mapper.map_message(root, msg) {
                    Some(d) if d.from_oxie => render(&d, root),
                    Some(_) | None => {
                        let r = msg.get("rendered").and_then(|r| r.as_str()).unwrap_or("").to_string();
                        // summary lines like "aborting due to ..." have no spans
                        if msg.get("spans").and_then(|s| s.as_array()).is_none_or(|s| s.is_empty()) {
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
pub fn check_collect(root: &Path, target_dir: Option<&Path>) -> std::io::Result<Vec<MappedDiag>> {
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cmd.args(["check", "--message-format=json", "--quiet"]).current_dir(root).stdout(Stdio::piped()).stderr(Stdio::null());
    if let Some(dir) = target_dir {
        cmd.env("CARGO_TARGET_DIR", dir);
    }
    let out = cmd.output()?;
    let mut mapper = Mapper::default();
    let mut diags = Vec::new();
    let mut seen = HashSet::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
            continue;
        }
        let manifest = v.get("manifest_path").and_then(|m| m.as_str()).map(PathBuf::from);
        if manifest.as_deref().is_some_and(|m| m.parent() != Some(root)) {
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
