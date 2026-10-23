//! Rewrites locations in generated Rust (`…/out/oxie/main.rs:10:5`) in a running program's
//! output, such as panic messages and backtraces, to the `.oxi` positions they came from.

use std::io::{Read, Write};
use std::path::Path;

use crate::cargo::Mapper;

/// Rewrites every `path.rs:line:col` whose file has a source map; other text is unchanged.
pub fn rewrite(text: &str, mapper: &mut Mapper) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(".rs:") {
        let (before, after) = rest.split_at(i + 3);
        // the path starts after the last space, quote or parenthesis
        let start = before.rfind(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | '(' | '<')).map_or(0, |s| s + 1);
        if let Some((line, col, len)) = line_col(&after[1..]) {
            if let Some((src, l, c)) = mapper.map_location(Path::new(&before[start..]), line, col) {
                let cwd = std::env::current_dir().unwrap_or_default();
                out.push_str(&before[..start]);
                out.push_str(&format!("{}:{}:{}", src.strip_prefix(&cwd).unwrap_or(&src).display(), l, c));
                rest = &after[1 + len..];
                continue;
            }
        }
        out.push_str(before);
        rest = after;
    }
    out.push_str(rest);
    out
}

/// `10:5` at the start of `s`: the line, the column and the length of the text.
fn line_col(s: &str) -> Option<(u32, u32, usize)> {
    let digits = |s: &str| s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let n = digits(s);
    let line = s[..n].parse().ok()?;
    let rest = s[n..].strip_prefix(':')?;
    let m = digits(rest);
    let col = rest[..m].parse().ok()?;
    Some((line, col, n + 1 + m))
}

/// Copies `from` to `to` as it arrives, rewriting locations. Whole lines are rewritten; a
/// partial line (a prompt) is passed on at once unless it may be the start of a path.
pub fn forward(mut from: impl Read, mut to: impl Write) {
    let mut mapper = Mapper::default();
    let mut buf = [0u8; 8192];
    let mut pending: Vec<u8> = Vec::new();
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        pending.extend_from_slice(&buf[..n]);
        let lines_end = pending.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        let mut cut = if pending[lines_end..].iter().any(|&b| b == b'/' || b == b'\\') { lines_end } else { pending.len() };
        // keep a character split between two reads for the next one
        if let Err(e) = std::str::from_utf8(&pending[..cut]) {
            if e.error_len().is_none() {
                cut = e.valid_up_to();
            }
        }
        if cut > 0 {
            let _ = to.write_all(rewrite(&String::from_utf8_lossy(&pending[..cut]), &mut mapper).as_bytes());
            let _ = to.flush();
            pending.drain(..cut);
        }
    }
    if !pending.is_empty() {
        let _ = to.write_all(rewrite(&String::from_utf8_lossy(&pending), &mut mapper).as_bytes());
        let _ = to.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_mapped_locations_only() {
        let dir = std::env::temp_dir().join(format!("oxie-locations-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("app.oxi");
        std::fs::write(&src, "fun nth(xs: &[i32], i: usize): i32 = xs[i]\n").unwrap();
        let gen = dir.join("main.rs");
        std::fs::write(&gen, "pub(crate) fn nth(xs: &[i32], i: usize) -> i32 {\n    xs[i]\n}\n").unwrap();
        std::fs::write(dir.join("main.rs.map"), format!("oxie-sourcemap 1\n{}\n0 0 0 0\n1 4 0 37\n", src.display())).unwrap();
        let mut m = Mapper::default();
        let text = format!("thread 'main' panicked at {}:2:5:\nindex out of bounds\n  at /rustc/library/core/src/slice/index.rs:12:3\n", gen.display());
        let out = rewrite(&text, &mut m);
        assert!(out.contains(&format!("panicked at {}:1:38:\n", src.display())), "{out}");
        assert!(out.contains("/rustc/library/core/src/slice/index.rs:12:3"), "{out}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn forwards_partial_lines() {
        let mut out = Vec::new();
        forward("name? ".as_bytes(), &mut out);
        assert_eq!(out, b"name? ");
        assert_eq!(line_col("10:5: x"), Some((10, 5, 4)));
        assert_eq!(line_col("10 x"), None);
    }
}
