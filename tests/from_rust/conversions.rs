// From/Into/TryFrom, FromStr, Default, associated consts and traits with generics.
use std::convert::TryFrom;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Celsius(i32);

#[derive(Debug, Clone, Copy, PartialEq)]
struct Fahrenheit(f64);

impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Self {
        Fahrenheit(c.0 as f64 * 9.0 / 5.0 + 32.0)
    }
}

#[derive(Debug, PartialEq)]
struct Even(u32);

impl TryFrom<u32> for Even {
    type Error = String;
    fn try_from(v: u32) -> Result<Self, Self::Error> {
        if v % 2 == 0 {
            Ok(Even(v))
        } else {
            Err(format!("{} is odd", v))
        }
    }
}

#[derive(Debug, Default, Clone)]
struct Settings {
    name: String,
    verbose: bool,
    level: u8,
    tags: Vec<String>,
}

#[derive(Debug, PartialEq)]
enum Color {
    Rgb(u8, u8, u8),
    Named(String),
}

impl FromStr for Color {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(hex) = s.strip_prefix('#') {
            if hex.len() != 6 {
                return Err(format!("bad hex '{}'", s));
            }
            let p = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| e.to_string());
            return Ok(Color::Rgb(p(0)?, p(2)?, p(4)?));
        }
        if s.is_empty() {
            Err("empty color".to_string())
        } else {
            Ok(Color::Named(s.to_lowercase()))
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Color::Rgb(r, g, b) => write!(f, "#{:02x}{:02x}{:02x}", r, g, b),
            Color::Named(n) => write!(f, "{}", n),
        }
    }
}

trait Describe {
    const KIND: &'static str;
    fn describe(&self) -> String;
    fn kind(&self) -> &'static str {
        Self::KIND
    }
}

impl Describe for Celsius {
    const KIND: &'static str = "temperature";
    fn describe(&self) -> String {
        format!("{} degrees", self.0)
    }
}

fn to_strings<T: Into<String>>(items: Vec<T>) -> Vec<String> {
    items.into_iter().map(Into::into).collect()
}

fn main() {
    let f: Fahrenheit = Celsius(100).into();
    println!("{:?} {:?}", f, Fahrenheit::from(Celsius(-40)));
    println!("{:?} {:?}", Even::try_from(8), Even::try_from(7));
    let e: Result<Even, _> = 10u32.try_into();
    println!("{:?}", e);

    let s = Settings { name: "app".into(), level: 3, ..Default::default() };
    println!("{:?}", s);
    let d = Settings::default();
    println!("{} {} {} {}", d.name.is_empty(), d.verbose, d.level, d.tags.len());

    for input in ["#ff8000", "Teal", "#12", ""] {
        match input.parse::<Color>() {
            Ok(c) => println!("{} -> {} ({:?})", input, c, c),
            Err(e) => println!("{} -> error: {}", input, e),
        }
    }
    let c = Celsius(21);
    println!("{} is a {}", c.describe(), c.kind());
    println!("{}", <Celsius as Describe>::KIND);
    let mut temps = vec![Celsius(30), Celsius(-5), Celsius(12)];
    temps.sort();
    println!("{:?} max {:?}", temps, temps.iter().max());
    println!("{:?}", to_strings(vec!["a", "b"]));
    let n: i64 = i64::from(7u8) + i64::try_from(300u64).unwrap();
    println!("{}", n);
}
