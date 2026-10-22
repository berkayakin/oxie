# Tooling

## Installing

```bash
cargo install --path crates/brs-cli
```

installs `brs` into `~/.cargo/bin`. New projects depend on `crates/brs-build` from this
repository by path; set `BRS_HOME` to the repository root if you move it.

## The `brs` command

| Command | Description |
|---|---|
| `brs new <name>` | New program: `Cargo.toml`, `build.rs`, `src/main.rs`, `src/main.brs` |
| `brs new <name> --lib` | New library (`src/lib.brs`) |
| `brs init [--lib]` | Make the current directory a brs project; in an existing Rust crate, add brs support (`build.rs` + build-dependency) |
| `brs run [--release] [-- args]` | Build and run the project |
| `brs run file.brs [-- args]` | Run a single file. Inside a project's `src/` this runs the project |
| `brs build [--release]` | Build |
| `brs check` | Type-check without producing binaries (fast) |
| `brs test [name] [--release]` | Run `test` blocks; `name` may be the test's description (`brs test "adds numbers"`) or part of it |
| `brs fmt [--check] [paths]` | Format `.brs` files (the project's `src/` by default) |
| `brs emit file.brs` | Print the Rust generated for a file |
| `brs lsp` | Start the language server on stdin/stdout |
| `brs clean` | `cargo clean` |
| `brs version` | Print the version |

Arguments after `--` go to the program (`run`) or to cargo (`build`, `check`, `test`).

### Diagnostics

`brs` first checks the `.brs` files itself (syntax and brs-level errors), then runs cargo and
rewrites rustc's messages so they point at `.brs` lines, including warnings:

```
error[E0382]: borrow of moved value: `v`
 --> src/main.brs:6:13
  |
6 |     println(v.len())
  |             ^ value borrowed here after move
  = note: move occurs because `v` has type `Vec<i32>`, which does not implement the `Copy` trait
```

Each generated file has a source map next to it (`main.rs.map`) used for the translation.

### Scripts

A `.brs` file outside a project runs as a script. Its dependencies go in header comments:

```kotlin
// dep: rand = "0.8"
// dep: serde = { version = "1", features = ["derive"] }
fun main() = println(rand.random::<u32>())
```

Scripts are built in `~/.cache/brs` (set `BRS_CACHE` to change it) with a shared target
directory, so only the first run of a script compiles its dependencies.

## Project layout

```
Cargo.toml          [build-dependencies] brs-build = { path = ".../crates/brs-build" }
build.rs            fn main() { brs_build::compile(); }
src/main.rs         include!(concat!(env!("OUT_DIR"), "/brs/main.rs"));
src/main.brs        crate root (src/lib.brs for libraries)
src/*.brs           modules; directories are nested modules
```

`brs_build::compile()` compiles `src/`; `brs_build::compile_dir("other")` compiles another
directory. In a Rust crate, `include!(concat!(env!("OUT_DIR"), "/brs/mod.rs"))` declares the brs
modules without a brs crate root.

## Formatter

`brs fmt` re-indents (4 spaces, one level per open bracket and for continuation lines such as
`.method()` chains), normalizes spaces around commas, colons and operators like `=`, `==`,
`->`, removes trailing whitespace and repeated blank lines. It never changes strings or
comments and leaves files with syntax errors untouched. `brs fmt --check` exits with status 1
if a file would change (useful in CI).

## Language server

`brs lsp` implements the Language Server Protocol over stdio:

| Feature | Details |
|---|---|
| Diagnostics | brs errors on every keystroke; rustc errors and warnings (mapped to `.brs`) after `cargo check` on open and save |
| Completion | locals, members after `.` (using declared types), project declarations, keywords, snippets, std types, builtins; module names in `import` lines |
| Hover | brs signature, doc comment, and the generated Rust signature |
| Go to definition / declaration | functions, classes, enums, variants, fields, methods, locals and parameters, across files |
| References, document highlight | whole-word occurrences in the project (locals: within their function) |
| Rename | project-wide for declarations; function-local for locals |
| Signature help | parameter list while typing a call |
| Formatting | same as `brs fmt` |
| Document and workspace symbols | classes, enums, interfaces, functions, fields, variants, tests |
| Inlay hints | how each method receives its object (`&self`, `&mut self`, static), which brs infers |
| `brs.showRust` command | returns the generated Rust for a file |

Initialization option `{"cargoCheck": false}` (or environment variable `BRS_LSP_NO_CHECK=1`)
turns off `cargo check` on save.

### Kuana

The extension in [`editors/kuana-brs`](../editors/kuana-brs) adds:

* syntax highlighting, outline / breadcrumbs / sticky scroll, bracket matching, indentation,
  text objects (tree-sitter grammar from `editors/tree-sitter-brs`);
* the language server (needs `brs` on your `PATH`);
* ▶ buttons on `fun main` (run, run --release, show generated Rust) and on each `test` block;
* snippets (`main`, `fun`, `class`, `data`, `when`, `test`, `errenum`, ...).

Install: Settings → Extensions → **Install from Folder…** → choose `editors/kuana-brs`. After
editing the grammar, run `editors/build-kuana-extension.sh` and **Reload Extensions**.

Kuana's own source (branch `brs-language` of the editor) additionally shows a `.brs` file icon
and adds `brs run`, `brs test`, `brs build` and `brs fmt` to the Run… list of brs projects.

### Other editors

Any editor with LSP support can use `brs lsp` for `*.brs` files. For syntax highlighting, use
the tree-sitter grammar in `editors/tree-sitter-brs` (Neovim, Helix, Zed), or Kotlin
highlighting as a fallback.
