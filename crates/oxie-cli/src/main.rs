//! The `oxie` command line tool.

mod cargo;
mod explain;
mod from_rust;
mod lsp;
mod project;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use oxie_compiler::{diag, Severity};
use project::Project;

const HELP: &str = "oxie - compiler and tools for the Oxie programming language, which compiles to plain Rust

USAGE:
    oxie <command> [options]

COMMANDS:
    new <name> [--lib]     Create a new project
    init [--lib]           Turn the current directory into a oxie project
    run [file.oxi] [--release] [-- args]
                           Build and run the project, or a single .oxi script
    build [--release]      Build the project
    check                  Type-check the project without producing binaries
    test [filter]          Run `test \"...\" { }` blocks
    fmt [--check] [paths]  Format .oxi files
    emit <file.oxi>        Print the Rust code generated for a file
    from-rust <file.rs> [-o out.oxi]
                           Translate Rust to Oxie (`-` reads stdin)
    lsp                    Start the language server (stdio)
    clean                  Remove build artifacts
    version                Print version

Any extra arguments after `--` are passed to the program (run) or cargo (build/check/test).";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("help");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    let r = match cmd {
        "new" => cmd_new(rest),
        "init" => cmd_init(rest),
        "run" => cmd_run(rest),
        "build" => cmd_build(rest, "build"),
        "check" => cmd_build(rest, "check"),
        "test" => cmd_test(rest),
        "fmt" | "format" => cmd_fmt(rest),
        "emit" => cmd_emit(rest),
        "from-rust" => from_rust::run(rest),
        "lsp" => {
            lsp::run();
            Ok(ExitCode::SUCCESS)
        }
        "clean" => cmd_clean(),
        "version" | "--version" | "-V" => {
            println!("oxie {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        "help" | "--help" | "-h" => {
            println!("{}", HELP);
            Ok(ExitCode::SUCCESS)
        }
        f if f.ends_with(".oxi") => cmd_run(&args),
        other => {
            eprintln!("error: unknown command `{}`\n\n{}", other, HELP);
            Ok(ExitCode::from(2))
        }
    };
    match r {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

type R = Result<ExitCode, String>;

fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn find_project() -> Result<Project, String> {
    Project::find(&cwd()).ok_or_else(|| "no Cargo.toml found (run `oxie new <name>` to create a project, or `oxie run file.oxi` for a script)".to_string())
}

fn split_dashdash(args: &[String]) -> (Vec<String>, Vec<String>) {
    match args.iter().position(|a| a == "--") {
        Some(i) => (args[..i].to_vec(), args[i + 1..].to_vec()),
        None => (args.to_vec(), Vec::new()),
    }
}

fn cmd_new(args: &[String]) -> R {
    let lib = args.iter().any(|a| a == "--lib");
    let name = args.iter().find(|a| !a.starts_with("--")).ok_or("usage: oxie new <name> [--lib]")?;
    let dir = cwd().join(name);
    if dir.join("Cargo.toml").exists() {
        return Err(format!("{} already contains a Cargo.toml", dir.display()));
    }
    let pkg = Path::new(name).file_name().unwrap().to_string_lossy().replace(' ', "_");
    project::create(&dir, &pkg, lib).map_err(|e| e.to_string())?;
    println!("Created {} `{}`", if lib { "library" } else { "program" }, pkg);
    println!("\n    cd {}\n    oxie {}\n", name, if lib { "test" } else { "run" });
    Ok(ExitCode::SUCCESS)
}

fn cmd_init(args: &[String]) -> R {
    let lib = args.iter().any(|a| a == "--lib");
    let dir = cwd();
    let pkg = dir.file_name().map(|n| n.to_string_lossy().replace(' ', "_")).unwrap_or_else(|| "app".into());
    if dir.join("Cargo.toml").exists() {
        // existing Rust project: add build script support
        let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).map_err(|e| e.to_string())?;
        if !manifest.contains("oxie-build") {
            let m = project::add_build_dependency(&manifest, &project::oxie_build_dep());
            std::fs::write(dir.join("Cargo.toml"), m).map_err(|e| e.to_string())?;
        }
        if !dir.join("build.rs").exists() {
            std::fs::write(dir.join("build.rs"), project::BUILD_RS).map_err(|e| e.to_string())?;
        }
        println!("Added Oxie to the existing crate. Write modules as src/<name>.oxi and add this to src/main.rs or src/lib.rs:\n\n    include!(concat!(env!(\"OUT_DIR\"), \"/oxie/mod.rs\"));\n");
        return Ok(ExitCode::SUCCESS);
    }
    project::create(&dir, &pkg, lib).map_err(|e| e.to_string())?;
    println!("Initialized Oxie project `{}`", pkg);
    Ok(ExitCode::SUCCESS)
}

/// Prints Oxie-level diagnostics. Returns false if there are errors.
fn report(out: &oxie_compiler::Output, root: &Path) -> bool {
    let color = cargo::color();
    let mut ok = true;
    for (path, text, diags) in &out.diags {
        let shown = path.strip_prefix(root).unwrap_or(path).display().to_string();
        for d in diags {
            if d.severity == Severity::Error {
                ok = false;
            }
            eprintln!("{}", diag::render(d, &shown, text, color));
        }
    }
    ok
}

fn precheck(p: &Project) -> Result<bool, String> {
    let out = p.check_sources().map_err(|e| e.to_string())?;
    Ok(report(&out, &p.root))
}

fn cmd_build(args: &[String], sub: &str) -> R {
    let (opts, extra) = split_dashdash(args);
    let p = find_project()?;
    if !precheck(&p)? {
        return Ok(ExitCode::FAILURE);
    }
    let mut cargs = vec![sub.to_string()];
    if opts.iter().any(|a| a == "--release" || a == "-r") {
        cargs.push("--release".into());
    }
    cargs.extend(extra);
    let r = cargo::cargo_json(&p.root, &cargs, false).map_err(|e| e.to_string())?;
    Ok(if r.success { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

fn cmd_run(args: &[String]) -> R {
    let (opts, prog_args) = split_dashdash(args);
    let release = opts.iter().any(|a| a == "--release" || a == "-r");
    let file = opts.iter().find(|a| a.ends_with(".oxi")).cloned();
    let (proj, script) = match &file {
        Some(f) => {
            let path = PathBuf::from(f);
            if !path.exists() {
                return Err(format!("{} not found", f));
            }
            let abs = oxie_compiler::compile::canonicalize(&path).map_err(|e| e.to_string())?;
            // a file inside a project's src/ runs that project
            match Project::find(abs.parent().unwrap()) {
                Some(p) if abs.starts_with(p.src_dir()) && p.manifest.contains("oxie-build") => (p, None),
                _ => (project::script_project(&abs).map_err(|e| e.to_string())?, Some(abs)),
            }
        }
        None => (find_project()?, None),
    };
    // Oxie diagnostics first
    let ok = match &script {
        Some(abs) => {
            let src = std::fs::read_to_string(abs).map_err(|e| e.to_string())?;
            let inputs = project::script_inputs(abs).map_err(|e| e.to_string())?;
            let out = oxie_compiler::compile(inputs, &project::script_options(&proj.name, &project::script_deps(&src)));
            report(&out, &cwd())
        }
        None => precheck(&proj)?,
    };
    if !ok {
        return Ok(ExitCode::FAILURE);
    }
    let mut cargs = vec!["build".to_string()];
    if release {
        cargs.push("--release".into());
    }
    if script.is_some() {
        std::env::set_var("CARGO_TARGET_DIR", project::cache_dir().join("target"));
    }
    let r = cargo::cargo_json(&proj.root, &cargs, script.is_some()).map_err(|e| e.to_string())?;
    if !r.success {
        return Ok(ExitCode::FAILURE);
    }
    let exe = r
        .executables
        .iter()
        .find(|e| e.file_stem().is_some_and(|s| s.to_string_lossy() == proj.name.replace('-', "_") || s.to_string_lossy() == proj.name))
        .or(r.executables.last())
        .cloned()
        .ok_or("no executable was built (is this a library? try `oxie test`)")?;
    let status = Command::new(&exe).args(&prog_args).status().map_err(|e| format!("cannot run {}: {}", exe.display(), e))?;
    Ok(match status.code() {
        Some(0) => ExitCode::SUCCESS,
        Some(c) => ExitCode::from(c.clamp(1, 255) as u8),
        None => ExitCode::FAILURE,
    })
}

fn cmd_test(args: &[String]) -> R {
    let (opts, extra) = split_dashdash(args);
    let file = opts.iter().find(|a| a.ends_with(".oxi")).cloned();
    // `test "adds numbers"` becomes the Rust test `test_adds_numbers`; accept the readable name too
    let filter: Vec<String> = opts.iter().filter(|a| !a.starts_with('-') && !a.ends_with(".oxi")).map(|f| test_filter(f)).collect();
    let (p, script) = match &file {
        Some(f) => {
            let abs = oxie_compiler::compile::canonicalize(f).map_err(|e| e.to_string())?;
            match Project::find(abs.parent().unwrap()) {
                Some(p) if abs.starts_with(p.src_dir()) => (p, false),
                _ => (project::script_project(&abs).map_err(|e| e.to_string())?, true),
            }
        }
        None => (find_project()?, false),
    };
    if script {
        std::env::set_var("CARGO_TARGET_DIR", project::cache_dir().join("target"));
    } else if !precheck(&p)? {
        return Ok(ExitCode::FAILURE);
    }
    let mut cargs = vec!["test".to_string(), "--no-run".to_string()];
    if opts.iter().any(|a| a == "--release") {
        cargs.push("--release".into());
    }
    let r = cargo::cargo_json(&p.root, &cargs, true).map_err(|e| e.to_string())?;
    if !r.success {
        return Ok(ExitCode::FAILURE);
    }
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cmd.arg("test").arg("--quiet").current_dir(&p.root);
    if opts.iter().any(|a| a == "--release") {
        cmd.arg("--release");
    }
    if !filter.is_empty() || !extra.is_empty() {
        cmd.arg("--");
        cmd.args(&filter);
        cmd.args(&extra);
    }
    let status = cmd.status().map_err(|e| e.to_string())?;
    Ok(if status.success() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

fn test_filter(name: &str) -> String {
    if name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == ':') {
        return name.to_string();
    }
    let mut s: String = name.chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
    while s.contains("__") {
        s = s.replace("__", "_");
    }
    s.trim_matches('_').to_string()
}

fn collect_oxie(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        if path.file_name().is_some_and(|n| n == "target" || n.to_string_lossy().starts_with('.')) {
            return;
        }
        if let Ok(rd) = std::fs::read_dir(path) {
            let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            entries.sort();
            for p in entries {
                collect_oxie(&p, out);
            }
        }
    } else if path.extension().is_some_and(|e| e == "oxi") {
        out.push(path.to_path_buf());
    }
}

fn cmd_fmt(args: &[String]) -> R {
    let check = args.iter().any(|a| a == "--check");
    let paths: Vec<PathBuf> = args.iter().filter(|a| !a.starts_with("--")).map(PathBuf::from).collect();
    let mut files = Vec::new();
    if paths.is_empty() {
        let root = Project::find(&cwd()).map(|p| p.src_dir()).unwrap_or_else(cwd);
        collect_oxie(&root, &mut files);
    } else {
        for p in &paths {
            collect_oxie(p, &mut files);
        }
    }
    let mut changed = 0;
    for f in &files {
        let text = std::fs::read_to_string(f).map_err(|e| e.to_string())?;
        let formatted = oxie_compiler::format::format_source(&text);
        if formatted != text {
            changed += 1;
            if check {
                println!("would reformat {}", f.display());
            } else {
                std::fs::write(f, formatted).map_err(|e| e.to_string())?;
                println!("formatted {}", f.display());
            }
        }
    }
    if check && changed > 0 {
        return Ok(ExitCode::FAILURE);
    }
    if !check && changed == 0 {
        println!("{} file(s) already formatted", files.len());
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_emit(args: &[String]) -> R {
    let file = args.iter().find(|a| !a.starts_with("--")).ok_or("usage: oxie emit <file.oxi>")?;
    let abs = oxie_compiler::compile::canonicalize(file).map_err(|e| format!("{}: {}", file, e))?;
    let project = Project::find(abs.parent().unwrap()).filter(|p| abs.starts_with(p.src_dir()));
    let (out, root) = match &project {
        Some(p) => {
            let inputs = oxie_compiler::discover(&p.src_dir()).map_err(|e| e.to_string())?;
            (oxie_compiler::compile(inputs, &p.options(p.root.join("target/oxie"))), p.root.clone())
        }
        None => {
            let inputs = project::script_inputs(&abs).map_err(|e| e.to_string())?;
            let src = std::fs::read_to_string(&abs).unwrap_or_default();
            (oxie_compiler::compile(inputs, &project::script_options("main", &project::script_deps(&src))), cwd())
        }
    };
    report(&out, &root);
    let idx = out.diags.iter().position(|(p, _, _)| *p == abs).unwrap_or(0);
    if let Some(f) = out.files.get(idx) {
        print!("{}", f.text);
    }
    Ok(if out.has_errors() { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

fn cmd_clean() -> R {
    let p = find_project()?;
    let status = Command::new("cargo").arg("clean").current_dir(&p.root).status().map_err(|e| e.to_string())?;
    Ok(if status.success() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}
