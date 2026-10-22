# Tooling

## Installing

```bash
cargo install --path crates/oxie-cli
```

installs `oxie` into `~/.cargo/bin`. New projects depend on `crates/oxie-build` from this
repository by path; set `OXIE_HOME` to the repository root if you move it.

## The `oxie` command

| Command | Description |
|---|---|
| `oxie new <name>` | New program: `Cargo.toml`, `build.rs`, `src/main.rs`, `src/main.oxi` |
| `oxie new <name> --lib` | New library (`src/lib.oxi`) |
| `oxie init [--lib]` | Make the current directory an Oxie project; in an existing Rust crate, add Oxie support (`build.rs` + build-dependency) |
| `oxie run [--release] [-- args]` | Build and run the project |
| `oxie run file.oxi [-- args]` | Run a single file. Inside a project's `src/` this runs the project |
| `oxie build [--release]` | Build |
| `oxie check` | Type-check without producing binaries (fast) |
| `oxie test [name] [--release]` | Run `test` blocks; `name` may be the test's description (`oxie test "adds numbers"`) or part of it |
| `oxie fmt [--check] [paths]` | Format `.oxi` files (the project's `src/` by default) |
| `oxie emit file.oxi` | Print the Rust generated for a file |
| `oxie lsp` | Start the language server on stdin/stdout |
| `oxie clean` | `cargo clean` |
| `oxie version` | Print the version |

Arguments after `--` go to the program (`run`) or to cargo (`build`, `check`, `test`).

### Diagnostics

`oxie` first checks the `.oxi` files itself (syntax and Oxie-level errors), then runs cargo and
rewrites rustc's messages so they point at `.oxi` lines, including warnings:

```
error[E0382]: borrow of moved value: `v`
 --> src/main.oxi:6:13
  |
6 |     println(v.len())
  |             ^ value borrowed here after move
  = note: move occurs because `v` has type `Vec<i32>`, which does not implement the `Copy` trait
```

Each generated file has a source map next to it (`main.rs.map`) used for the translation.

### Scripts

A `.oxi` file outside a project runs as a script. Its dependencies go in header comments:

```oxie
// dep: rand = "0.8"
// dep: serde = { version = "1", features = ["derive"] }
fun main() = println(rand.random::<u32>())
```

Scripts are built in `~/.cache/Oxie` (set `OXIE_CACHE` to change it) with a shared target
directory, so only the first run of a script compiles its dependencies.

## Project layout

```
Cargo.toml          [build-dependencies] oxie-build = { path = ".../crates/oxie-build" }
build.rs            fn main() { oxie_build::compile(); }
src/main.rs         include!(concat!(env!("OUT_DIR"), "/oxie/main.rs"));
src/main.oxi        crate root (src/lib.oxi for libraries)
src/*.oxi           modules; directories are nested modules
```

`oxie_build::compile()` compiles `src/`; `oxie_build::compile_dir("other")` compiles another
directory. In a Rust crate, `include!(concat!(env!("OUT_DIR"), "/oxie/mod.rs"))` declares the Oxie
modules without an Oxie crate root.

## Formatter

`oxie fmt` re-indents (4 spaces, one level per open bracket and for continuation lines such as
`.method()` chains), normalizes spaces around commas, colons and operators like `=`, `==`,
`->`, removes trailing whitespace and repeated blank lines. It never changes strings or
comments and leaves files with syntax errors untouched. `oxie fmt --check` exits with status 1
if a file would change (useful in CI).

## Language server

`oxie lsp` implements the Language Server Protocol over stdio:

| Feature | Details |
|---|---|
| Diagnostics | Oxie errors on every keystroke; rustc errors and warnings (mapped to `.oxi`) after `cargo check` on open and save |
| Completion | locals, members after `.` (using declared types), project declarations, keywords, snippets, std types, builtins; module names in `import` lines |
| Hover | Oxie signature, doc comment, and the generated Rust signature |
| Go to definition / declaration | functions, classes, enums, variants, fields, methods, locals and parameters, across files |
| References, document highlight | whole-word occurrences in the project (locals: within their function) |
| Rename | project-wide for declarations; function-local for locals |
| Signature help | parameter list while typing a call |
| Formatting | same as `oxie fmt` |
| Document and workspace symbols | classes, enums, interfaces, functions, fields, variants, tests |
| Inlay hints | how each method receives its object (`&self`, `&mut self`, static), which Oxie infers |
| `oxie.showRust` command | returns the generated Rust for a file |

Initialization option `{"cargoCheck": false}` (or environment variable `OXIE_LSP_NO_CHECK=1`)
turns off `cargo check` on save.

### Kuana

The extension in [`editors/kuana-oxie`](../editors/kuana-oxie) adds:

* syntax highlighting, outline / breadcrumbs / sticky scroll, bracket matching, indentation,
  text objects (tree-sitter grammar from `editors/tree-sitter-oxie`);
* the language server (needs `oxie` on your `PATH`);
* ▶ buttons on `fun main` (run, run --release, show generated Rust) and on each `test` block;
* snippets (`main`, `fun`, `class`, `data`, `when`, `test`, `errenum`, ...).

Install: Settings → Extensions → **Install from Folder…** → choose `editors/kuana-oxie`. After
editing the grammar, run `editors/build-kuana-extension.sh` and **Reload Extensions**.

Kuana's own source (branch `oxie-language` of the editor) additionally shows a `.oxi` file icon
and adds `oxie run`, `oxie test`, `oxie build` and `oxie fmt` to the Run… list of Oxie projects.

### Other editors

Any editor with LSP support can use `oxie lsp` for `*.oxi` files. For syntax highlighting, use
the tree-sitter grammar in `editors/tree-sitter-oxie` (Neovim, Helix, Zed), or Kotlin
highlighting as a fallback.
