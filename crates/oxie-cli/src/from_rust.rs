//! `oxie from-rust <file.rs> [-o out.oxi]`: translates Rust to Oxie.

use std::process::ExitCode;

pub fn run(args: &[String]) -> Result<ExitCode, String> {
    let usage = "usage: oxie from-rust <file.rs> [-o out.oxi]";
    let mut input = None;
    let mut output = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                output = Some(args.get(i + 1).ok_or(usage)?.clone());
                i += 1;
            }
            a if input.is_none() => input = Some(a.to_string()),
            _ => return Err(usage.into()),
        }
        i += 1;
    }
    let input = input.ok_or(usage)?;
    let src = if input == "-" {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s).map_err(|e| e.to_string())?;
        s
    } else {
        std::fs::read_to_string(&input).map_err(|e| format!("{}: {}", input, e))?
    };
    let name = if input == "-" { "<stdin>" } else { input.as_str() };
    let t = oxie_from_rust::translate(&src).map_err(|e| format!("{}:{}", name, e))?;
    for w in &t.warnings {
        eprintln!("warning: {}: {}", name, w);
    }
    match output {
        Some(o) => std::fs::write(&o, &t.oxie).map_err(|e| format!("{}: {}", o, e))?,
        None => print!("{}", t.oxie),
    }
    Ok(ExitCode::SUCCESS)
}
