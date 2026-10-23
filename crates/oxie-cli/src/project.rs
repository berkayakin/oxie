//! Project discovery, scaffolding and single-file scripts.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use oxie_compiler::compile::{extern_crates_from_manifest, Input, Options, Output};

/// The `oxie-build` build-dependency of new projects and scripts, as a TOML value: the
/// crates.io release matching this `oxie`, or the local checkout at `OXIE_HOME` when it is set
/// (working on Oxie itself).
pub fn oxie_build_dep() -> String {
    match std::env::var("OXIE_HOME") {
        Ok(home) if !home.is_empty() => format!("{{ path = \"{}\" }}", toml_path(&Path::new(&home).join("crates/oxie-build"))),
        _ => format!("\"{}\"", env!("CARGO_PKG_VERSION")),
    }
}

pub struct Project {
    pub root: PathBuf,
    pub manifest: String,
    pub name: String,
}

impl Project {
    pub fn src_dir(&self) -> PathBuf {
        self.root.join("src")
    }

    pub fn find(start: &Path) -> Option<Project> {
        let mut dir = Some(start);
        while let Some(d) = dir {
            let m = d.join("Cargo.toml");
            if m.exists() {
                let manifest = std::fs::read_to_string(&m).ok()?;
                if manifest.contains("[package]") {
                    let name = package_name(&manifest).unwrap_or_else(|| "main".into());
                    return Some(Project { root: d.to_path_buf(), manifest, name });
                }
            }
            dir = d.parent();
        }
        None
    }

    pub fn options(&self, out_dir: PathBuf) -> Options {
        Options { crate_name: self.name.clone(), extern_crates: extern_crates_from_manifest(&self.manifest), out_dir, script: false }
    }

    /// Compiles all sources in memory (to report Oxie errors before invoking cargo).
    pub fn check_sources(&self) -> std::io::Result<Output> {
        let inputs = oxie_compiler::discover(&self.src_dir())?;
        Ok(oxie_compiler::compile(inputs, &self.options(self.root.join("target/oxie-check"))))
    }
}

pub fn package_name(manifest: &str) -> Option<String> {
    let mut in_pkg = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_pkg = t == "[package]";
            continue;
        }
        if in_pkg {
            if let Some(rest) = t.strip_prefix("name") {
                let v = rest.trim_start().strip_prefix('=')?.trim().trim_matches('"');
                return Some(v.to_string());
            }
        }
    }
    None
}

fn toml_path(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

pub fn manifest_text(name: &str, deps: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"
build = "build.rs"

[dependencies]
{deps}
[build-dependencies]
oxie-build = {}
"#,
        oxie_build_dep()
    )
}

pub const BUILD_RS: &str = "// Compiles the .oxi files in src/ to Rust (see the Oxie documentation).\nfn main() {\n    oxie_build::compile();\n}\n";

pub fn root_stub(kind: &str) -> String {
    format!(
        "// The code of this crate is written in src/{kind}.oxi.\n// This file includes the Rust code that Oxie generates from it.\ninclude!(concat!(env!(\"OUT_DIR\"), \"/oxie/{kind}.rs\"));\n"
    )
}

const HELLO_MAIN: &str = r#"/// Entry point. Run with `oxie run`.
fun main() {
    val name = "World"
    println("Hello, $name!")
}
"#;

const HELLO_LIB: &str = r#"/// Adds two numbers.
fun add(a: i32, b: i32): i32 = a + b

test "add works" {
    assert_eq(add(2, 3), 5)
}
"#;

pub fn create(dir: &Path, name: &str, lib: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(dir.join("src"))?;
    let write_new = |p: PathBuf, text: &str| -> std::io::Result<()> {
        if !p.exists() {
            std::fs::write(p, text)?;
        }
        Ok(())
    };
    write_new(dir.join("Cargo.toml"), &manifest_text(name, ""))?;
    write_new(dir.join("build.rs"), BUILD_RS)?;
    write_new(dir.join(".gitignore"), "/target\n")?;
    if lib {
        write_new(dir.join("src/lib.rs"), &root_stub("lib"))?;
        write_new(dir.join("src/lib.oxi"), HELLO_LIB)?;
    } else {
        write_new(dir.join("src/main.rs"), &root_stub("main"))?;
        write_new(dir.join("src/main.oxi"), HELLO_MAIN)?;
    }
    Ok(())
}

pub fn cache_dir() -> PathBuf {
    if let Ok(d) = std::env::var("OXIE_CACHE") {
        return PathBuf::from(d);
    }
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".cache/oxie")
}

fn hash(s: &str) -> u64 {
    // FNV-1a
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Dependencies declared in a script header: `// dep: rand = "0.8"`.
pub fn script_deps(src: &str) -> Vec<String> {
    let mut deps = Vec::new();
    for line in src.lines() {
        let t = line.trim();
        if let Some(d) = t.strip_prefix("// dep:").or_else(|| t.strip_prefix("//! dep:")) {
            deps.push(d.trim().to_string());
        } else if !(t.is_empty() || t.starts_with("//")) {
            break;
        }
    }
    deps
}

/// Creates (or refreshes) a hidden Cargo project for a single .oxi script.
pub fn script_project(file: &Path) -> std::io::Result<Project> {
    let abs = std::fs::canonicalize(file)?;
    let src = std::fs::read_to_string(&abs)?;
    let stem = abs.file_stem().unwrap().to_string_lossy().replace(|c: char| !c.is_alphanumeric(), "_");
    let stem = if stem.chars().next().is_none_or(|c| c.is_ascii_digit()) { format!("script_{}", stem) } else { stem };
    let h = hash(&abs.display().to_string());
    let dir = cache_dir().join("scripts").join(format!("{}-{:x}", stem, h));
    std::fs::create_dir_all(dir.join("src"))?;
    let deps = script_deps(&src).join("\n");
    // scripts share one target directory, so two `main.oxi` in different folders need
    // different package (and binary) names
    let package = format!("{}_{:08x}", stem, h as u32);
    let manifest = manifest_text(&package, &if deps.is_empty() { String::new() } else { format!("{}\n", deps) });
    let manifest = manifest.replace("build = \"build.rs\"", "build = \"build.rs\"\n\n[workspace]");
    write_if_changed(&dir.join("Cargo.toml"), &manifest)?;
    let build = format!(
        "fn main() {{\n    oxie_build::compile_script(\"{}\");\n}}\n",
        toml_path(&abs)
    );
    write_if_changed(&dir.join("build.rs"), &build)?;
    // crate attributes written in the script (`#![...]`) go at the top of the generated main.rs
    let crate_attrs: String = src.lines().filter(|l| l.trim_start().starts_with("#![")).map(|l| format!("{}\n", l.trim())).collect();
    write_if_changed(&dir.join("src/main.rs"), &format!("{}{}", crate_attrs, root_stub("main")))?;
    Ok(Project { root: dir, manifest, name: package })
}

fn write_if_changed(p: &Path, text: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(p).ok().as_deref() != Some(text) {
        std::fs::write(p, text)?;
    }
    Ok(())
}

/// Inputs for a script: the file itself as the crate root.
pub fn script_inputs(file: &Path) -> std::io::Result<Vec<Input>> {
    let abs = std::fs::canonicalize(file)?;
    let text = std::fs::read_to_string(&abs)?;
    Ok(vec![Input { rel_path: PathBuf::from("main.oxi"), abs_path: abs, text }])
}

pub fn script_options(name: &str, deps: &[String]) -> Options {
    let manifest = format!("[dependencies]\n{}\n", deps.join("\n"));
    let mut ex: HashSet<String> = extern_crates_from_manifest(&manifest);
    ex.remove("");
    Options { crate_name: name.to_string(), extern_crates: ex, out_dir: PathBuf::from("/tmp/oxie-check"), script: true }
}
