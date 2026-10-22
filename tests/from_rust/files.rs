// std::io and std::fs: io::Result, BufRead, Write, ? on io errors, and a CSV-like parser.
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;

#[derive(Debug)]
struct Row {
    name: String,
    score: u32,
}

fn temp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("oxie-from-rust-{}-{}", std::process::id(), name))
}

fn write_rows(path: &PathBuf, rows: &[(&str, u32)]) -> io::Result<()> {
    let mut f = File::create(path)?;
    writeln!(f, "name,score")?;
    for (name, score) in rows {
        writeln!(f, "{},{}", name, score)?;
    }
    f.flush()?;
    Ok(())
}

fn read_rows(path: &PathBuf) -> io::Result<Vec<Row>> {
    let reader = BufReader::new(File::open(path)?);
    let mut rows = Vec::new();
    for line in reader.lines().skip(1) {
        let line = line?;
        let mut parts = line.split(',');
        let name = parts.next().unwrap_or_default().to_string();
        let score = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        rows.push(Row { name, score });
    }
    Ok(rows)
}

fn summarize(rows: &[Row]) -> String {
    let best = rows.iter().max_by_key(|r| r.score).map(|r| r.name.as_str()).unwrap_or("nobody");
    let total: u32 = rows.iter().map(|r| r.score).sum();
    format!("{} rows, total {}, best {}", rows.len(), total, best)
}

fn main() -> io::Result<()> {
    let path = temp_file("scores.csv");
    write_rows(&path, &[("ada", 90), ("bob", 72), ("cy", 85)])?;
    let rows = read_rows(&path)?;
    for r in &rows {
        println!("{:<4} {:>3}", r.name, r.score);
    }
    println!("{}", summarize(&rows));

    let text = fs::read_to_string(&path)?;
    println!("{} bytes, {} lines", text.len(), text.lines().count());
    match fs::read_to_string(temp_file("missing.txt")) {
        Ok(_) => println!("unexpected"),
        Err(e) => println!("missing: {:?}", e.kind()),
    }
    fs::remove_file(&path)?;
    println!("exists after remove: {}", path.exists());

    let mut out = io::stdout().lock();
    writeln!(out, "written through a lock")?;
    let mut buf: Vec<u8> = Vec::new();
    write!(&mut buf, "{}-{}", 1, 2)?;
    println!("{}", String::from_utf8(buf).unwrap());
    Ok(())
}
