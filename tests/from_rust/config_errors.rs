// Error handling: custom errors, From conversions, `?`, Box<dyn Error> and a fallible main.
use std::error::Error;
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug)]
enum ConfigError {
    Missing(String),
    BadNumber { key: String, source: ParseIntError },
    OutOfRange(String, i64),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing(k) => write!(f, "missing key '{}'", k),
            ConfigError::BadNumber { key, source } => write!(f, "bad number for {}: {}", key, source),
            ConfigError::OutOfRange(k, v) => write!(f, "{} = {} is out of range", k, v),
        }
    }
}

impl Error for ConfigError {}

struct Config {
    entries: Vec<(String, String)>,
}

impl Config {
    fn parse(text: &str) -> Config {
        let mut entries = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                entries.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        Config { entries }
    }

    fn get(&self, key: &str) -> Result<&str, ConfigError> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .ok_or_else(|| ConfigError::Missing(key.to_string()))
    }

    fn number(&self, key: &str) -> Result<i64, ConfigError> {
        let raw = self.get(key)?;
        let n: i64 = raw.parse().map_err(|e| ConfigError::BadNumber { key: key.to_string(), source: e })?;
        if n < 0 || n > 65535 {
            return Err(ConfigError::OutOfRange(key.to_string(), n));
        }
        Ok(n)
    }
}

fn sum_numbers(items: &[&str]) -> Result<i64, Box<dyn Error>> {
    let mut total = 0;
    for item in items {
        let n: i64 = item.parse()?;
        if n == 13 {
            return Err("unlucky number".into());
        }
        total += n;
    }
    Ok(total)
}

fn describe(r: Result<i64, Box<dyn Error>>) -> String {
    match r {
        Ok(v) => format!("ok {}", v),
        Err(e) => format!("error: {}", e),
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let cfg = Config::parse("# settings\nport = 8080\nname = demo\nretries = x\nlimit = 99999\n");
    println!("port: {}", cfg.number("port")?);
    println!("name: {}", cfg.get("name")?);
    for key in ["retries", "limit", "timeout"] {
        match cfg.number(key) {
            Ok(v) => println!("{key} = {v}"),
            Err(e) => println!("error: {e}"),
        }
    }
    println!("{}", describe(sum_numbers(&["1", "2", "3"])));
    println!("{}", describe(sum_numbers(&["1", "x"])));
    println!("{}", describe(sum_numbers(&["13"])));
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    run()?;
    let v: i32 = "42".parse()?;
    println!("parsed {}", v);
    Ok(())
}
