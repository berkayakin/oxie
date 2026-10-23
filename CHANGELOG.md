# Changelog

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
