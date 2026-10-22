//! Text emitter with indentation and source-map marks.

use crate::diag::Span;

#[derive(Clone, Copy, Debug)]
pub struct Mark {
    pub gen_line: u32,
    pub gen_col: u32,
    pub src: u32,
}

#[derive(Default)]
pub struct Emitter {
    pub buf: String,
    line: u32,
    col: u32,
    indent: usize,
    at_line_start: bool,
    pub marks: Vec<Mark>,
}

impl Emitter {
    pub fn new() -> Emitter {
        Emitter { at_line_start: true, ..Default::default() }
    }

    fn write_indent(&mut self) {
        if self.at_line_start {
            for _ in 0..self.indent {
                self.buf.push_str("    ");
            }
            self.col = (self.indent * 4) as u32;
            self.at_line_start = false;
        }
    }

    /// Writes text. Embedded newlines are followed by the current indentation.
    pub fn w(&mut self, s: &str) {
        for (i, part) in s.split('\n').enumerate() {
            if i > 0 {
                self.buf.push('\n');
                self.line += 1;
                self.col = 0;
                self.at_line_start = true;
            }
            if !part.is_empty() {
                self.write_indent();
                self.buf.push_str(part);
                self.col += part.chars().count() as u32;
            }
        }
    }

    pub fn nl(&mut self) {
        self.buf.push('\n');
        self.line += 1;
        self.col = 0;
        self.at_line_start = true;
    }

    /// Ensures we're at the start of a fresh line.
    pub fn line_break(&mut self) {
        if !self.at_line_start {
            self.nl();
        }
    }

    pub fn indent(&mut self) {
        self.indent += 1;
    }
    pub fn dedent(&mut self) {
        self.indent = self.indent.saturating_sub(1);
    }

    pub fn mark(&mut self, span: Span) {
        if span == Span::default() {
            return;
        }
        let col = if self.at_line_start { (self.indent * 4) as u32 } else { self.col };
        if let Some(last) = self.marks.last() {
            if last.gen_line == self.line && last.gen_col == col {
                self.marks.pop();
            }
        }
        self.marks.push(Mark { gen_line: self.line, gen_col: col, src: span.start });
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    /// Writes raw multi-line text, re-indenting each line.
    pub fn raw_lines(&mut self, text: &str) {
        for (i, l) in text.lines().enumerate() {
            if i > 0 {
                self.nl();
            }
            self.w(l);
        }
    }
}
