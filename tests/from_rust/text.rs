// String processing and every kind of format specification.
fn word_freq(text: &str) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for word in text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        let w = word.to_lowercase();
        match counts.iter_mut().find(|(k, _)| *k == w) {
            Some((_, n)) => *n += 1,
            None => counts.push((w, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    counts
}

fn caesar(s: &str, shift: u8) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_lowercase() {
                (((c as u8 - b'a' + shift) % 26) + b'a') as char
            } else if c.is_ascii_uppercase() {
                (((c as u8 - b'A' + shift) % 26) + b'A') as char
            } else {
                c
            }
        })
        .collect()
}

fn is_palindrome(s: &str) -> bool {
    let cleaned: Vec<char> = s.chars().filter(|c| c.is_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect();
    cleaned.iter().eq(cleaned.iter().rev())
}

fn main() {
    let text = "The cat and the hat. The END, the end!";
    for (w, n) in word_freq(text).iter().take(3) {
        println!("{:>6}: {}", w, n);
    }
    println!("{}", caesar("Hello, World", 3));
    println!("{} {}", is_palindrome("A man, a plan, a canal: Panama"), is_palindrome("rust"));

    let name = "Oxie";
    let pi = 3.14159265;
    let n = 255;
    println!("[{:<10}] [{:^10}] [{:>10}]", name, name, name);
    println!("[{:*^9}] [{:08.3}] [{:+}]", "mid", pi, 42);
    println!("{:x} {:X} {:o} {:b} {:#x} {:#b}", n, n, n, n, n, n);
    println!("{:e} {:.0} {:10.2}|", 1234.5, 2.5, pi);
    println!("{0} {1} {0}", "a", "b");
    println!("{name} has {} letters", name.len());
    println!("{n:>5} {pi:.2}");
    let width = 7;
    println!("[{:>width$}] [{:>1$}] [{:.*}]", 42, 6, 3, pi, width = width);
    println!("{{literal braces}} and a $dollar and \"quotes\"");
    println!("{:?} {:?} {:#?}", "quoted\n", 'c', (1, "two"));
    let multi = "line one
line two";
    println!("{}", multi);
    let raw = r"C:\path\to $file";
    println!("{}", raw);
    let s = String::from("héllo wörld");
    println!("{} chars, {} bytes, upper {}", s.chars().count(), s.len(), s.to_uppercase());
    let parts: Vec<&str> = "a,b,,c".split(',').collect();
    println!("{:?} {}", parts, parts.join("+"));
    let mut owned = String::new();
    for (i, ch) in "abc".char_indices() {
        owned.push_str(&format!("{}{}", ch, i));
    }
    owned.insert(0, '>');
    println!("{} {} {:?}", owned, owned.contains("b1"), owned.find('c'));
    let trimmed = "  padded  ".trim_end();
    println!("[{}] {}", trimmed, "x".repeat(3));
    let num: Result<u8, _> = "300".parse::<u8>();
    println!("{:?}", num.is_err());
    eprintln!("this goes to stderr");
}
