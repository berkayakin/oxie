//! End-to-end tests: every `tests/e2e/*.brs` program is built with `brs run` and its stdout
//! compared with the matching `.out` file. Programs in `tests/diagnostics` must fail to compile
//! with the errors listed in their `// error: <line>: <text>` comments.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn brs() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_brs"));
    c.env("NO_COLOR", "1");
    c
}

fn programs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |e| e == "brs"))
        .collect();
    v.sort();
    v
}

#[test]
fn e2e_programs() {
    let dir = repo().join("tests/e2e");
    let mut failures = Vec::new();
    for p in programs(&dir) {
        let Ok(expected) = std::fs::read_to_string(p.with_extension("out")) else { continue };
        let out = brs().arg("run").arg(&p).current_dir(&dir).output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        if stdout != expected {
            failures.push(format!(
                "{}:\n--- expected\n{}\n--- actual\n{}\n--- stderr\n{}",
                p.display(),
                expected,
                stdout,
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{} program(s) failed:\n{}", failures.len(), failures.join("\n\n"));
}

#[test]
fn diagnostics_point_at_brs_sources() {
    let dir = repo().join("tests/diagnostics");
    let mut failures = Vec::new();
    for p in programs(&dir) {
        let src = std::fs::read_to_string(&p).unwrap();
        let expected: Vec<(String, String)> = src
            .lines()
            .filter_map(|l| l.trim().strip_prefix("// error:"))
            .map(|rest| {
                let (line, text) = rest.trim().split_once(':').unwrap();
                (line.trim().to_string(), text.trim().to_string())
            })
            .collect();
        let out = brs().arg("run").arg(&p).current_dir(&dir).output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        if out.status.success() {
            failures.push(format!("{}: expected compilation to fail", p.display()));
            continue;
        }
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        for (line, text) in expected {
            let loc = format!("{}:{}:", name, line);
            let found = stderr.split("\n\n").any(|block| block.contains(&loc) && block.contains(&text));
            if !found {
                failures.push(format!("{}: missing error at line {} containing `{}`\nstderr:\n{}", p.display(), line, text, stderr));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Every example project under `examples/` with an `expected.txt` must print exactly that,
/// and every example's `test` blocks must pass.
#[test]
fn examples_run_and_test() {
    let dir = repo().join("examples");
    let mut failures = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.join("Cargo.toml").exists()).collect();
    entries.sort();
    for ex in entries {
        if let Ok(expected) = std::fs::read_to_string(ex.join("expected.txt")) {
            let out = brs().arg("run").current_dir(&ex).env("TODO_FILE", "/nonexistent/todo.txt").output().unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout != expected {
                failures.push(format!("{}:\n--- expected\n{}\n--- actual\n{}\n--- stderr\n{}", ex.display(), expected, stdout, String::from_utf8_lossy(&out.stderr)));
            }
        }
        let out = brs().arg("test").current_dir(&ex).output().unwrap();
        if !out.status.success() {
            failures.push(format!("{}: `brs test` failed\n{}{}", ex.display(), String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
