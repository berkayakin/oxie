//! Recovers the ordinary (non-doc) comments of a Rust file, which `syn` drops, so they can be
//! re-attached to the translated items and statements by line number.

#[derive(Clone, Debug)]
pub struct Comment {
    /// 1-based line of the comment start.
    pub line: usize,
    /// 1-based line of the comment end (differs from `line` for multi-line block comments).
    pub end_line: usize,
    pub text: String,
    /// Nothing but whitespace precedes the comment on its line.
    pub own_line: bool,
    pub used: bool,
}

/// Scans `src` for `//` and `/* */` comments, skipping doc comments (`///`, `//!`, `/**`, `/*!`),
/// which `syn` keeps as attributes, and anything inside string or character literals.
pub fn scan(src: &str) -> Vec<Comment> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let mut line_start = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b'\n' => {
                line += 1;
                i += 1;
                line_start = i;
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                let start = i;
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                let text = &src[start..i];
                let doc = (text.starts_with("///") && !text.starts_with("////")) || text.starts_with("//!");
                if !doc {
                    let own_line = src[line_start..start].trim().is_empty();
                    out.push(Comment { line, end_line: line, text: text.trim_end().to_string(), own_line, used: false });
                }
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                let start = i;
                let start_line = line;
                let mut depth = 0;
                while i < b.len() {
                    if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
                        depth += 1;
                        i += 2;
                    } else if b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        if b[i] == b'\n' {
                            line += 1;
                            line_start = i + 1;
                        }
                        i += 1;
                    }
                }
                let text = &src[start..i];
                let doc = (text.starts_with("/**") && !text.starts_with("/***") && text != "/**/") || text.starts_with("/*!");
                if !doc {
                    let own_line = src[src[..start].rfind('\n').map_or(0, |p| p + 1)..start].trim().is_empty();
                    out.push(Comment { line: start_line, end_line: line, text: text.to_string(), own_line, used: false });
                }
            }
            b'"' => {
                i = skip_string(b, i + 1, &mut line, &mut line_start);
            }
            b'r' | b'b' if is_raw_string_start(b, i) => {
                // r"...", r#"..."#, br"..."
                let mut j = i;
                if b[j] == b'b' {
                    j += 1;
                }
                j += 1; // r
                let mut hashes = 0;
                while j < b.len() && b[j] == b'#' {
                    hashes += 1;
                    j += 1;
                }
                j += 1; // opening quote
                loop {
                    if j >= b.len() {
                        break;
                    }
                    if b[j] == b'\n' {
                        line += 1;
                        line_start = j + 1;
                    }
                    if b[j] == b'"' && (0..hashes).all(|k| j + 1 + k < b.len() && b[j + 1 + k] == b'#') {
                        j += 1 + hashes;
                        break;
                    }
                    j += 1;
                }
                i = j;
            }
            b'\'' => {
                // char literal or lifetime
                if i + 1 < b.len() && b[i + 1] == b'\\' {
                    let mut j = i + 2;
                    while j < b.len() && b[j] != b'\'' {
                        j += 1;
                    }
                    i = j + 1;
                } else {
                    // a char literal is 'x' where x may be multi-byte
                    let rest = &src[i + 1..];
                    let mut chars = rest.char_indices();
                    match (chars.next(), chars.next()) {
                        (Some((_, ch)), Some((k, '\''))) if ch != '\'' => i = i + 1 + k + 1,
                        _ => i += 1,
                    }
                }
            }
            _ if (c.is_ascii_alphanumeric() || c == b'_') => {
                // skip identifiers so that `r` / `b` inside names are not raw-string starts
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    out
}

fn is_raw_string_start(b: &[u8], i: usize) -> bool {
    if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_') {
        return false;
    }
    let mut j = i;
    if b[j] == b'b' {
        j += 1;
        if j < b.len() && b[j] == b'"' {
            return false; // b"..." handled as a normal string by the caller
        }
    }
    if j >= b.len() || b[j] != b'r' {
        return false;
    }
    j += 1;
    while j < b.len() && b[j] == b'#' {
        j += 1;
    }
    j < b.len() && b[j] == b'"'
}

fn skip_string(b: &[u8], mut i: usize, line: &mut usize, line_start: &mut usize) -> usize {
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            b'\n' => {
                *line += 1;
                i += 1;
                *line_start = i;
            }
            _ => i += 1,
        }
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_comments_outside_strings() {
        let src = "// a\nfn f() { let s = \"// no\"; let c = '/'; // b\n /* c */ }\n/// doc\nfn g<'a>() {}\n";
        let c = scan(src);
        let texts: Vec<&str> = c.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, vec!["// a", "// b", "/* c */"]);
        assert!(c[0].own_line && !c[1].own_line && c[2].own_line);
        assert_eq!(c[1].line, 2);
    }
}
