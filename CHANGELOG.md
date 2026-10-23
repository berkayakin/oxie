# Changelog

## Unreleased

* A documentation site at [oxie.bkymz.net](https://oxie.bkymz.net), generated from `docs/`: every
  example shows the Rust that `oxie emit` produces for it and its output, kept current by the tests.
* A VS Code extension in `editors/vscode-oxie`: highlighting, the language server, ▶ run and
  test buttons, the generated Rust beside the source, tasks and snippets.
* Language server: files opened through a symlinked directory (on macOS, anything under `/tmp`)
  get their diagnostics, and definitions point at the open file instead of a second copy. A
  script opened while another script's `cargo check` runs is checked next; before, its rustc
  errors appeared only after saving it.
* Clearer errors: a borrow conflict on `self` says `this`, and a loop over a field whose body
  changes `this` offers to loop over a copy; a `shared class` handle sent to another thread (or
  held across an `await` in a spawned task) offers to declare the class `@sync`.
* Borrowed parameters: a `String` or `Vec<T>` parameter of a top-level non-`pub` function that
  the body only reads becomes `&str` or `&[T]`, so `greet(name)` no longer uses up `name`.
  Arguments for `&str` and `&[T]` parameters are lent whatever they are
  (`greet(name.to_uppercase())` used to be a type error). When a function keeps the value, the
  move error no longer suggests borrowing it.
* Capture lists: `{ [counter, tx] -> ... }` gives a lambda its own clones of `counter` and `tx`
  (and `[n = expr]` a computed value) and makes it `move`, replacing Rust's
  `let tx = tx.clone(); move || ...` before every thread or callback.
* `for (x in xs)` no longer uses up `xs` when the list is used after the loop (or by the next
  round of an outer loop): the loop borrows it (`for x in &xs`, or `for &n in &nums` for `Copy`
  elements). Loops over a class's collection fields borrow them too. Before, both were errors.
* A method reading through a new iterator (`input.chars().next()`) is no longer inferred to take
  `&mut self`.
* Classes, structs and enums whose fields hold references get their lifetime parameter
  automatically: `class Parser(input: &str)` used to produce Rust that didn't compile and now
  becomes `struct Parser<'a> { input: &'a str }`. Functions taking or returning such types get
  their lifetimes filled in as for references, and `data class`es holding `&str` derive `Copy`,
  `Eq` and `Hash` when their other fields allow.
* Windows: scripts and projects whose paths Oxie resolves no longer get the `\\?\` prefix
  written into `Cargo.toml` and build scripts, which Cargo rejected ("invalid path url").
* `oxie init` in a crate that already has `[build-dependencies]` adds `oxie-build` to that
  table instead of writing a second one, which Cargo rejected as a duplicate key.
* The repository moved to [github.com/oxie-lang/oxie](https://github.com/oxie-lang/oxie); links
  to the old address redirect.
* The crates declare `rust-version = "1.82"`, the oldest Rust that builds Oxie and its
  generated code, so an older Rust gets a clear error instead of a failed build.

## 0.1.0 - 2026-10-23

The first release of Oxie, a programming language for the Rust ecosystem: a concise syntax that
Kotlin and TypeScript developers already know, compiled to plain Rust. An Oxie project is a
Cargo crate, every crate on crates.io works, and programs run exactly as fast as hand-written
Rust.

### Install

```bash
cargo install oxie
```

Prebuilt binaries for macOS, Linux and Windows are attached below, and `cargo binstall oxie`
downloads them instead of compiling.

### The language

* `val`/`var`, `fun` with expression bodies, string templates (`"$name ${x + 1}"`), default and
  named arguments.
* `data class` with derives worked out for you, `class` with inferred `&self`, `&mut self` and
  static receivers, interfaces, extension functions, operator overloading, `shared class` for
  shared mutable objects.
* Null safety over `Option`: `T?`, `null`, `?.`, `?:`, `!!` and smart checks.
* Error handling over `Result`: `throws`, `throw`, `try`/`catch`, `error enum` with messages, and
  `?` added for you on calls that can fail.
* `when`, Kotlin-style ranges (`until`, `downTo`, `step`), lambdas (`{ it * 2 }`, `x => x * 2` or
  Rust closures), collection operations (`filter`, `map`, `sum_of`, ...).
* Arguments borrowed for you at call sites, trait bounds and lifetimes inferred.
* Rust is always available: any Rust type or crate, `rust { ... }` blocks, macros, and `.rs` and
  `.oxi` files mixed in one crate.

### Tools

* `oxie new`, `init`, `run`, `build`, `check`, `test`, `fmt`, `emit` and `clean`; single `.oxi`
  files run as scripts, with dependencies declared in a header comment.
* Compiler errors and rustc errors point at `.oxi` lines; common borrow errors are explained in
  Oxie terms, with fixes.
* `oxie from-rust` translates Rust code into Oxie.
* `oxie lsp`: a language server with diagnostics, completion, hover showing the generated Rust,
  go to definition, references, rename, signature help, formatting, symbols, inlay hints and
  quick fixes.
* Editor support for Kuana and a tree-sitter grammar.

### Crates

[`oxie`](https://crates.io/crates/oxie), [`oxie-compiler`](https://crates.io/crates/oxie-compiler),
[`oxie-build`](https://crates.io/crates/oxie-build) and
[`oxie-from-rust`](https://crates.io/crates/oxie-from-rust), licensed under MIT or Apache-2.0.
