// Borrowed data: lifetimes in functions and structs, string slices.
struct Tokenizer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Tokenizer { input, pos: 0 }
    }

    fn next_token(&mut self) -> Option<&'a str> {
        let rest = &self.input[self.pos..];
        let rest_trimmed = rest.trim_start();
        if rest_trimmed.is_empty() {
            return None;
        }
        let start = self.pos + (rest.len() - rest_trimmed.len());
        let len = rest_trimmed.find(' ').unwrap_or(rest_trimmed.len());
        self.pos = start + len;
        Some(&self.input[start..start + len])
    }
}

fn longest<'a>(a: &'a str, b: &'a str) -> &'a str {
    if a.len() >= b.len() {
        a
    } else {
        b
    }
}

fn first_word(s: &str) -> &str {
    s.split_whitespace().next().unwrap_or("")
}

struct Highlight<'t> {
    text: &'t str,
    marks: Vec<(usize, usize)>,
}

impl<'t> Highlight<'t> {
    fn render(&self) -> String {
        let mut out = String::new();
        let mut last = 0;
        for &(s, e) in &self.marks {
            out.push_str(&self.text[last..s]);
            out.push('[');
            out.push_str(&self.text[s..e]);
            out.push(']');
            last = e;
        }
        out.push_str(&self.text[last..]);
        out
    }
}

fn main() {
    let text = String::from("  the quick   brown fox ");
    let mut t = Tokenizer::new(&text);
    let mut tokens = Vec::new();
    while let Some(tok) = t.next_token() {
        tokens.push(tok);
    }
    println!("{:?}", tokens);
    println!("{}", longest("apple", "fig"));
    let a = String::from("short");
    let result;
    {
        let b = String::from("much longer");
        result = longest(&a, &b).to_string();
    }
    println!("{}", result);
    println!("{}", first_word("hello world"));
    let h = Highlight { text: "abcdefgh", marks: vec![(1, 3), (5, 6)] };
    println!("{}", h.render());
}
