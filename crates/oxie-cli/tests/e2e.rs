//! End-to-end tests: every `tests/e2e/*.oxi` program is built with `oxie run` and its stdout
//! compared with the matching `.out` file. Programs in `tests/diagnostics` must fail to compile
//! with the errors listed in their `// error: <line>: <text>` comments.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn oxie() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_oxie"));
    // scripts and new projects build with this checkout's oxie-build, not the one on crates.io
    c.env("NO_COLOR", "1").env("OXIE_HOME", repo());
    c
}

fn programs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |e| e == "oxi"))
        .collect();
    v.sort();
    v
}

#[test]
fn e2e_programs() {
    run_programs("tests/e2e");
}

/// One small program per Rust feature (generics, lifetimes, GATs, unions, FFI, let chains, ...):
/// Oxie must accept the Rust feature and produce the expected output.
#[test]
fn rust_feature_coverage() {
    run_programs("tests/coverage");
}

fn run_programs(rel: &str) {
    let dir = repo().join(rel);
    let mut failures = Vec::new();
    for p in programs(&dir) {
        let Ok(expected) = std::fs::read_to_string(p.with_extension("out")) else { continue };
        let out = oxie().arg("run").arg(&p).current_dir(&dir).output().unwrap();
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
fn diagnostics_point_at_oxie_sources() {
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
        let out = oxie().arg("run").arg(&p).current_dir(&dir).output().unwrap();
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
            let out = oxie().arg("run").current_dir(&ex).env("TODO_FILE", "/nonexistent/todo.txt").output().unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout != expected {
                failures.push(format!("{}:\n--- expected\n{}\n--- actual\n{}\n--- stderr\n{}", ex.display(), expected, stdout, String::from_utf8_lossy(&out.stderr)));
            }
        }
        let out = oxie().arg("test").current_dir(&ex).output().unwrap();
        if !out.status.success() {
            failures.push(format!("{}: `oxie test` failed\n{}{}", ex.display(), String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Every ```oxie block with `fun main` in README.md and docs/*.md must compile and run.
#[test]
fn documentation_examples_run() {
    let root = repo();
    let mut files = vec![root.join("README.md")];
    let mut docs: Vec<PathBuf> = std::fs::read_dir(root.join("docs")).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map_or(false, |e| e == "md")).collect();
    docs.sort();
    files.extend(docs);
    let tmp = std::env::temp_dir().join(format!("oxie-doc-examples-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let mut failures = Vec::new();
    let mut count = 0;
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        let mut in_block = false;
        let mut block = String::new();
        let mut start_line = 0;
        for (i, line) in text.lines().enumerate() {
            if !in_block && line.trim_start().starts_with("```oxie") {
                in_block = true;
                block.clear();
                start_line = i + 2;
                continue;
            }
            if in_block && line.trim_start().starts_with("```") {
                in_block = false;
                if block.contains("fun main") && !block.contains("// not-runnable") {
                    count += 1;
                    let name = format!("{}_{}.oxi", f.file_stem().unwrap().to_string_lossy().replace('-', "_"), start_line);
                    let path = tmp.join(&name);
                    std::fs::write(&path, &block).unwrap();
                    let out = oxie().arg("run").arg(&path).current_dir(&tmp).output().unwrap();
                    let expect_failure = block.contains("throw") && block.contains("fun main() throws") && block.contains("// exits with an error");
                    if out.status.success() == expect_failure {
                        failures.push(format!("{}:{}\n{}", f.display(), start_line, String::from_utf8_lossy(&out.stderr)));
                    }
                }
                continue;
            }
            if in_block {
                block.push_str(line);
                block.push('\n');
            }
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(count > 10, "found only {} runnable examples", count);
    assert!(failures.is_empty(), "{} documentation example(s) failed:\n{}", failures.len(), failures.join("\n\n"));
}

/// `oxie new` and `oxie init` take oxie-build from crates.io, or from `OXIE_HOME` when it is set.
#[test]
fn new_projects_depend_on_oxie_build() {
    let tmp = std::env::temp_dir().join(format!("oxie-new-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let build_dep = |dir: &Path| {
        let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        manifest.lines().find(|l| l.starts_with("oxie-build")).unwrap_or_default().to_string()
    };

    assert!(oxie().env_remove("OXIE_HOME").args(["new", "from_crates_io"]).current_dir(&tmp).status().unwrap().success());
    assert_eq!(build_dep(&tmp.join("from_crates_io")), format!(r#"oxie-build = "{}""#, env!("CARGO_PKG_VERSION")));

    assert!(oxie().args(["new", "from_checkout"]).current_dir(&tmp).status().unwrap().success());
    let local = repo().join("crates/oxie-build").display().to_string().replace('\\', "/");
    assert_eq!(build_dep(&tmp.join("from_checkout")), format!(r#"oxie-build = {{ path = "{local}" }}"#));

    let rust = tmp.join("rust_crate");
    std::fs::create_dir_all(rust.join("src")).unwrap();
    std::fs::write(rust.join("Cargo.toml"), "[package]\nname = \"rust_crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
    assert!(oxie().env_remove("OXIE_HOME").arg("init").current_dir(&rust).status().unwrap().success());
    assert_eq!(build_dep(&rust), format!(r#"oxie-build = "{}""#, env!("CARGO_PKG_VERSION")));
    assert!(rust.join("build.rs").exists());

    // a crate that already has build-dependencies gets oxie-build in the same table
    let with_deps = tmp.join("with_build_deps");
    std::fs::create_dir_all(with_deps.join("src")).unwrap();
    std::fs::write(with_deps.join("Cargo.toml"), "[package]\nname = \"with_build_deps\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[build-dependencies]\ncc = \"1\"\n").unwrap();
    std::fs::write(with_deps.join("src/main.rs"), "fn main() {}\n").unwrap();
    assert!(oxie().arg("init").current_dir(&with_deps).status().unwrap().success());
    let manifest = std::fs::read_to_string(with_deps.join("Cargo.toml")).unwrap();
    assert_eq!(manifest.matches("[build-dependencies]").count(), 1, "{manifest}");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let read = Command::new(cargo).args(["metadata", "--no-deps", "--format-version", "1"]).current_dir(&with_deps).output().unwrap();
    assert!(read.status.success(), "{}", String::from_utf8_lossy(&read.stderr));

    let _ = std::fs::remove_dir_all(&tmp);
}
