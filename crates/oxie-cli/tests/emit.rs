//! Golden tests of the generated Rust: `oxie emit` of every `tests/emit/*.oxi` must equal the
//! `.rs` file next to it, and `oxie emit --annotate` the `.annotated.rs` file when there is one.
//! After checking a change in the output, rewrite them with
//! `OXIE_BLESS=1 cargo test -p oxie --test emit`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    oxie_compiler::compile::canonicalize(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap()
}

/// The emitted Rust, with the support module's path (which depends on the machine) left out.
fn emit(file: &Path, dir: &Path, annotate: bool) -> String {
    let mut c = Command::new(env!("CARGO_BIN_EXE_oxie"));
    c.arg("emit").arg(file).current_dir(dir).env("NO_COLOR", "1");
    if annotate {
        c.arg("--annotate");
    }
    let out = c.output().unwrap();
    assert!(out.status.success(), "oxie emit {} failed:\n{}", file.display(), String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).lines().map(|l| if l.starts_with("#[path = ") { "#[path = \"...\"]" } else { l }).collect::<Vec<_>>().join("\n") + "\n"
}

#[test]
fn generated_rust_matches_golden_files() {
    let dir = repo().join("tests/emit");
    let mut programs: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "oxi")).collect();
    programs.sort();
    assert!(!programs.is_empty());
    let bless = std::env::var_os("OXIE_BLESS").is_some();
    let mut failures = Vec::new();
    for p in programs {
        let plain = emit(&p, &dir, false);
        // the same output from another directory
        if emit(&p, &repo(), false) != plain {
            failures.push(format!("{}: output depends on the current directory", p.display()));
        }
        let annotated_path = p.with_extension("annotated.rs");
        let mut cases = vec![(p.with_extension("rs"), plain)];
        if annotated_path.exists() {
            cases.push((annotated_path, emit(&p, &dir, true)));
        }
        for (golden, actual) in cases {
            if bless {
                std::fs::write(&golden, &actual).unwrap();
                continue;
            }
            let expected = std::fs::read_to_string(&golden).unwrap_or_default();
            if expected != actual {
                failures.push(format!("{} differs:\n--- expected\n{}\n--- actual\n{}", golden.display(), expected, actual));
            }
        }
    }
    assert!(failures.is_empty(), "{}\n\nAfter checking the changes, update the files with\n    OXIE_BLESS=1 cargo test -p oxie --test emit", failures.join("\n\n"));
}
