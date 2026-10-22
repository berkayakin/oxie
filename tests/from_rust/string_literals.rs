// String literals where a String is expected: let types, branches, Vec<String>, constructors.
#[derive(Debug)]
struct Acct {
    owner: String,
    n: u32,
}

impl Acct {
    fn new(owner: String, n: u32) -> Self {
        Acct { owner, n }
    }
}

fn label(c: bool) -> String {
    match c {
        true => "yes".to_string(),
        false => String::from("no"),
    }
}

fn main() {
    let c = std::env::args().count() > 5;
    let s: String = if c { "a".to_string() } else { "b".to_string() };
    let t: String = match c {
        true => "x".to_string(),
        false => "y".into(),
    };
    let v: Vec<String> = vec!["p".to_string(), "q".to_owned()];
    let a = Acct::new(String::from("cy"), 1);
    let o: Option<String> = Some("z".to_string());
    println!("{} {} {:?} {:?} {} {:?}", s, t, v, a, label(c), o);
}
