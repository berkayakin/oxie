//! Rust format strings (`"{} {name:>5} {0:?}"`) and Oxie string literals and templates.

#[derive(Clone, Debug, PartialEq)]
pub enum ArgRef {
    Next,
    Index(usize),
    Name(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Lit(String),
    Arg { arg: ArgRef, spec: String },
}

/// Parses the value of a format string. Returns `None` for specs Oxie templates can't express
/// (`{:width$}`, `{:.*}`) and for malformed strings.
pub fn parse(s: &str) -> Option<Vec<Piece>> {
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                lit.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                lit.push('}');
            }
            '}' => return None,
            '{' => {
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some('{') | None => return None,
                        Some(c) => inner.push(c),
                    }
                }
                let (arg, spec) = match inner.find(':') {
                    Some(i) => (inner[..i].trim().to_string(), inner[i + 1..].to_string()),
                    None => (inner.trim().to_string(), String::new()),
                };
                if spec.contains('$') || spec.contains('*') {
                    return None;
                }
                let arg = if arg.is_empty() {
                    ArgRef::Next
                } else if arg.chars().all(|c| c.is_ascii_digit()) {
                    ArgRef::Index(arg.parse().ok()?)
                } else if arg.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    ArgRef::Name(arg.trim_start_matches("r#").to_string())
                } else {
                    return None;
                };
                if !lit.is_empty() {
                    out.push(Piece::Lit(std::mem::take(&mut lit)));
                }
                out.push(Piece::Arg { arg, spec });
            }
            c => lit.push(c),
        }
    }
    if !lit.is_empty() {
        out.push(Piece::Lit(lit));
    }
    Some(out)
}

/// Escapes text for an Oxie string literal or template: Rust escapes plus `\$`.
pub fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '$' => out.push_str("\\$"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// An Oxie string literal with the given value.
pub fn quote(s: &str) -> String {
    format!("\"{}\"", escape(s))
}

/// One interpolated value of a template: the Oxie expression text, whether it is a plain
/// identifier, and the format spec.
pub struct Interp {
    pub expr: String,
    pub simple_ident: bool,
    pub spec: String,
}

pub enum TPart {
    Lit(String),
    Interp(Interp),
}

/// Builds an Oxie template (`"a $b ${c + 1:?}"`). Returns `None` when an expression can't be
/// placed inside `${...}` safely.
pub fn template(parts: &[TPart]) -> Option<String> {
    let mut out = String::from("\"");
    for (i, p) in parts.iter().enumerate() {
        match p {
            TPart::Lit(s) => out.push_str(&escape(s)),
            TPart::Interp(it) => {
                let risky_char = ["'{'", "'}'", "'\"'", "'\\"].iter().any(|c| it.expr.contains(c));
                if it.expr.contains('\n') || risky_char || it.expr.contains('$') || it.expr.contains("\"\"\"") {
                    return None;
                }
                // braces inside strings in the expression would confuse the template scanner
                if it.expr.contains('"') && (it.expr.contains('{') || it.expr.contains('}') || it.expr.contains('\\')) {
                    return None;
                }
                let next_is_ident = match parts.get(i + 1) {
                    Some(TPart::Lit(s)) => s.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_'),
                    _ => false,
                };
                if it.simple_ident && it.spec.is_empty() && !next_is_ident && !it.expr.starts_with("r#") {
                    out.push('$');
                    out.push_str(&it.expr);
                } else {
                    // a top-level `:` in the expression would be read as a format spec
                    if it.spec.is_empty() && has_top_level_single_colon(&it.expr) {
                        return None;
                    }
                    out.push_str("${");
                    out.push_str(&it.expr);
                    if !it.spec.is_empty() {
                        out.push(':');
                        out.push_str(&it.spec);
                    }
                    out.push('}');
                }
            }
        }
    }
    out.push('"');
    Some(out)
}

fn has_top_level_single_colon(s: &str) -> bool {
    let b = s.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if in_str {
            if c == b'\\' {
                i += 1;
            } else if c == b'"' {
                in_str = false;
            }
        } else {
            match c {
                b'"' => in_str = true,
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b':' if depth == 0 => {
                    let prev = i > 0 && b[i - 1] == b':';
                    let next = i + 1 < b.len() && b[i + 1] == b':';
                    if !prev && !next {
                        return true;
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_format_strings() {
        let p = parse("a {} {x:>5} {0:?} {{lit}}").unwrap();
        assert_eq!(p.len(), 7);
        assert_eq!(p[1], Piece::Arg { arg: ArgRef::Next, spec: String::new() });
        assert_eq!(p[3], Piece::Arg { arg: ArgRef::Name("x".into()), spec: ">5".into() });
        assert_eq!(p[5], Piece::Arg { arg: ArgRef::Index(0), spec: "?".into() });
        assert_eq!(p[6], Piece::Lit(" {lit}".into()));
        assert!(parse("{:width$}").is_none());
        assert!(parse("{:.*}").is_none());
    }

    #[test]
    fn builds_templates() {
        let t = template(&[
            TPart::Lit("cost $".into()),
            TPart::Interp(Interp { expr: "n".into(), simple_ident: true, spec: String::new() }),
            TPart::Lit("x ".into()),
            TPart::Interp(Interp { expr: "a + b".into(), simple_ident: false, spec: ".2".into() }),
        ])
        .unwrap();
        assert_eq!(t, "\"cost \\$${n}x ${a + b:.2}\"");
    }
}
