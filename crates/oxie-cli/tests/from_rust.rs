//! `oxie from-rust` round trips: Rust programs are translated to Oxie, run with `oxie run`, and
//! must print exactly what the original Rust prints.
//!
//! * `tests/from_rust/*.rs` are hand-written Rust programs; the expected output comes from
//!   compiling them with `rustc`.
//! * The `oxie emit` output of every `tests/e2e` and `tests/coverage` program is translated back
//!   to Oxie and must still print the program's `.out` file.

mod common;

use common::parallel;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    oxie_compiler::compile::canonicalize(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap()
}

fn oxie() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_oxie"));
    c.env("NO_COLOR", "1").env("OXIE_HOME", repo());
    c
}

fn files(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> =
        std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == ext)).collect();
    v.sort();
    v
}

fn work_dir(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Translates `rs` to `<dir>/<stem>.oxi`, runs it (building in `cache`) and returns its stdout,
/// or a failure report.
fn translate_and_run(rs: &Path, dir: &Path, stem: &str, cwd: &Path, cache: &Path) -> Result<String, String> {
    let oxi = dir.join(format!("{}.oxi", stem));
    let out = oxie().arg("from-rust").arg(rs).arg("-o").arg(&oxi).output().unwrap();
    if !out.status.success() {
        return Err(format!("{}: `oxie from-rust` failed:\n{}", rs.display(), String::from_utf8_lossy(&out.stderr)));
    }
    let run = oxie().arg("run").arg(&oxi).current_dir(cwd).env("OXIE_CACHE", cache).output().unwrap();
    if !run.status.success() && run.stdout.is_empty() {
        return Err(format!(
            "{}: the translation failed to run:\n--- oxie\n{}\n--- stderr\n{}",
            rs.display(),
            std::fs::read_to_string(&oxi).unwrap_or_default(),
            String::from_utf8_lossy(&run.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&run.stdout).to_string())
}

#[test]
fn hand_written_rust_programs() {
    let src = repo().join("tests/from_rust");
    let dir = work_dir("from_rust");
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let programs = files(&src, "rs");
    let count = programs.len();
    let results = parallel(&programs, |rs, cache| {
        let stem = rs.file_stem().unwrap().to_string_lossy().to_string();
        let bin = dir.join(format!("{}-rust", stem));
        let c = Command::new(&rustc).args(["--edition", "2021", "-A", "warnings", "-o"]).arg(&bin).arg(rs).output().unwrap();
        assert!(c.status.success(), "{} does not compile with rustc:\n{}", rs.display(), String::from_utf8_lossy(&c.stderr));
        let expected = String::from_utf8_lossy(&Command::new(&bin).output().unwrap().stdout).to_string();
        match translate_and_run(rs, &dir, &stem, &dir, cache) {
            Ok(actual) if actual == expected => None,
            Ok(actual) => Some(format!(
                "{}:\n--- rust printed\n{}\n--- oxie printed\n{}\n--- oxie source\n{}",
                rs.display(),
                expected,
                actual,
                std::fs::read_to_string(dir.join(format!("{}.oxi", stem))).unwrap_or_default()
            )),
            Err(e) => Some(e),
        }
    });
    let failures: Vec<String> = results.into_iter().flatten().collect();
    assert!(count >= 15, "expected at least 15 programs in tests/from_rust");
    assert!(failures.is_empty(), "{} of {} program(s) failed:\n{}", failures.len(), count, failures.join("\n\n"));
}

#[test]
fn emitted_rust_translates_back() {
    let dir = work_dir("from_emit");
    // (program, expected output, Rust source, stem, working directory)
    let mut jobs: Vec<(PathBuf, String, PathBuf, String, PathBuf)> = Vec::new();
    for sub in ["tests/e2e", "tests/coverage"] {
        for p in files(&repo().join(sub), "oxi") {
            let Ok(expected) = std::fs::read_to_string(p.with_extension("out")) else { continue };
            let src = std::fs::read_to_string(&p).unwrap();
            let emitted = oxie().arg("emit").arg(&p).output().unwrap();
            assert!(emitted.status.success(), "oxie emit {} failed", p.display());
            // crate dependencies are declared in header comments, which `emit` does not copy
            let mut rust: String = src.lines().filter(|l| l.starts_with("// dep:")).map(|l| format!("{}\n", l)).collect();
            rust.push_str(&String::from_utf8_lossy(&emitted.stdout));
            let stem = format!("{}_{}", sub.trim_start_matches("tests/"), p.file_stem().unwrap().to_string_lossy());
            let rs = dir.join(format!("{}.rs", stem));
            std::fs::write(&rs, rust).unwrap();
            jobs.push((p, expected, rs, stem, dir.clone()));
        }
    }
    // single-file example projects
    let mut examples: Vec<PathBuf> = std::fs::read_dir(repo().join("examples")).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();
    examples.sort();
    for ex in examples {
        let Ok(expected) = std::fs::read_to_string(ex.join("expected.txt")) else { continue };
        if files(&ex.join("src"), "oxi").len() != 1 || !ex.join("src/main.oxi").exists() {
            continue;
        }
        let manifest = std::fs::read_to_string(ex.join("Cargo.toml")).unwrap();
        let mut rust: String = manifest
            .split("[dependencies]")
            .nth(1)
            .unwrap_or("")
            .lines()
            .take_while(|l| !l.starts_with('['))
            .filter(|l| l.contains('='))
            .map(|l| format!("// dep: {}\n", l.trim()))
            .collect();
        let emitted = oxie().arg("emit").arg(ex.join("src/main.oxi")).output().unwrap();
        assert!(emitted.status.success(), "oxie emit {} failed", ex.display());
        rust.push_str(&String::from_utf8_lossy(&emitted.stdout));
        let stem = format!("examples_{}", ex.file_name().unwrap().to_string_lossy());
        let rs = dir.join(format!("{}.rs", stem));
        std::fs::write(&rs, rust).unwrap();
        // examples read their input files relative to the project
        jobs.push((ex.clone(), expected, rs, stem, ex));
    }
    let count = jobs.len();
    let results = parallel(&jobs, |(p, expected, rs, stem, cwd), cache| match translate_and_run(rs, &dir, stem, cwd, cache) {
        Ok(actual) if actual == *expected => None,
        Ok(actual) => Some(format!("{}:\n--- expected\n{}\n--- actual\n{}", p.display(), expected, actual)),
        Err(e) => Some(e),
    });
    let failures: Vec<String> = results.into_iter().flatten().collect();
    assert!(failures.is_empty(), "{} of {} program(s) failed:\n{}", failures.len(), count, failures.join("\n\n"));
}

/// Translates a crate's Rust sources into an Oxie project at `dir` with the given manifest.
fn port_crate(src: &Path, dir: &Path, root: &str, manifest: &str) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("Cargo.toml"), manifest).unwrap();
    std::fs::write(dir.join("build.rs"), "fn main() {\n    oxie_build::compile();\n}\n").unwrap();
    std::fs::write(dir.join("src").join(format!("{}.rs", root)), format!("include!(concat!(env!(\"OUT_DIR\"), \"/oxie/{}.rs\"));\n", root)).unwrap();
    for rs in files(src, "rs") {
        let oxi = dir.join("src").join(rs.file_name().unwrap()).with_extension("oxi");
        let out = oxie().arg("from-rust").arg(&rs).arg("-o").arg(&oxi).output().unwrap();
        assert!(out.status.success(), "{}: {}", rs.display(), String::from_utf8_lossy(&out.stderr));
        // `mod name` declares a Rust file; the translated `name.oxi` is found by itself
        let text = std::fs::read_to_string(&oxi).unwrap();
        let mut lines: Vec<&str> = Vec::new();
        let mut skip_mod = false;
        for l in text.lines() {
            if l.starts_with("// TODO(from-rust): remove this line") {
                skip_mod = true;
                continue;
            }
            if skip_mod && (l.starts_with("mod ") || l.starts_with("pub mod ")) {
                skip_mod = false;
                continue;
            }
            lines.push(l);
        }
        std::fs::write(&oxi, lines.join("\n") + "\n").unwrap();
    }
}

/// The `oxie` command and this translator, both translated to Oxie, must behave exactly like
/// the originals. Slow (builds both crates and their dependencies): `cargo test -- --ignored`.
#[test]
#[ignore]
fn translated_cli_behaves_the_same() {
    let r = repo();
    let dir = work_dir("self_port");
    let deps = |extra: &str| {
        format!(
            "[package]\nname = \"{{}}\"\nversion = \"0.1.0\"\nedition = \"2021\"\nbuild = \"build.rs\"\n\n[workspace]\n\n[dependencies]\noxie-compiler = {{ path = \"{}\" }}\n{}\n[build-dependencies]\noxie-build = {{ path = \"{}\" }}\n",
            r.join("crates/oxie-compiler").display(),
            extra,
            r.join("crates/oxie-build").display()
        )
    };
    let translator = deps("syn = { version = \"2\", features = [\"full\", \"visit\", \"extra-traits\"] }\nproc-macro2 = { version = \"1\", features = [\"span-locations\"] }\nquote = \"1\"\nprettyplease = \"0.2\"\n").replacen("{}", "translator_port", 1);
    port_crate(&r.join("crates/oxie-from-rust/src"), &dir.join("translator"), "lib", &translator);
    let cli = deps(&format!("oxie-from-rust = {{ path = \"{}\", package = \"translator_port\" }}\nserde_json = \"1\"\n", dir.join("translator").display())).replacen("{}", "cli_port", 1);
    port_crate(&r.join("crates/oxie-cli/src"), &dir.join("cli"), "main", &cli);
    let build = oxie().arg("build").current_dir(dir.join("cli")).output().unwrap();
    assert!(build.status.success(), "the translated CLI does not build:\n{}", String::from_utf8_lossy(&build.stderr));
    let ported = dir.join("cli/target/debug/cli_port");

    let mut runs: Vec<Vec<String>> = vec![vec!["version".into()], vec!["help".into()], vec!["no-such-command".into()]];
    for sub in ["tests/e2e", "tests/coverage", "tests/diagnostics"] {
        for p in files(&r.join(sub), "oxi") {
            runs.push(vec!["emit".into(), p.display().to_string()]);
        }
    }
    for p in files(&r.join("tests/from_rust"), "rs") {
        runs.push(vec!["from-rust".into(), p.display().to_string()]);
    }
    runs.push(vec!["fmt".into(), "--check".into(), r.join("tests/e2e").display().to_string()]);
    let mut failures = Vec::new();
    for args in &runs {
        let a = oxie().args(args).current_dir(&r).output().unwrap();
        let mut c = Command::new(&ported);
        c.env("NO_COLOR", "1").env("OXIE_HOME", &r);
        let b = c.args(args).current_dir(&r).output().unwrap();
        if a.stdout != b.stdout || a.stderr != b.stderr || a.status.code() != b.status.code() {
            failures.push(format!("oxie {}:\n--- original\n{}{}\n--- translated\n{}{}", args.join(" "), String::from_utf8_lossy(&a.stdout), String::from_utf8_lossy(&a.stderr), String::from_utf8_lossy(&b.stdout), String::from_utf8_lossy(&b.stderr)));
        }
    }
    assert!(failures.is_empty(), "{} of {} invocations differ:\n{}", failures.len(), runs.len(), failures.join("\n\n"));
}
