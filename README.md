# brs — Better Rust

**Kotlin/TypeScript-style syntax that compiles to plain Rust.**
Every crate works, programs run exactly as fast as hand-written Rust, and you never write
`let mut`, `impl` blocks, `Ok(...)`, `.to_string()` on literals or `#[derive(...)]` boilerplate
unless you want to.

```kotlin
import std::collections::HashMap

data class User(name: String, age: u32, email: String? = null)

fun oldest(users: &[User]): &User? = users.iter().max_by_key { it.age }

fun parse_user(line: &str): User throws {
    val (name, age) = line.split_once(',') ?: throw "bad line: $line"
    User(name.trim(), age.trim().parse()?)
}

fun main() throws {
    var users = listOf(User("Ada", 36), User("Linus", 28, "linus@example.com"))
    users.push(parse_user("Grace, 45")?)
    println("oldest: ${oldest(&users)?.name ?: "nobody"}")

    var by_age = HashMap()
    for (u in &users) by_age.insert(u.age, u.name.clone())

    when (users.len()) {
        0 -> println("no users")
        in 1..=10 -> println("${users.len()} users")
        else -> println("many users")
    }
}
```

It compiles to the Rust you would write yourself (`brs emit` shows it):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct User {
    pub(crate) name: String,
    pub(crate) age: u32,
    pub(crate) email: Option<String>,
}

pub(crate) fn oldest(users: &[User]) -> Option<&User> {
    users.iter().max_by_key(|it| it.age)
}

pub(crate) fn parse_user(line: &str) -> Result<User, AnyError> {
    let (name, age) = match line.split_once(',') {
        Some(v) => v,
        None => return Err(format!("bad line: {line}").into()),
    };
    Ok(User::new(name.trim().into(), age.trim().parse()?, None))
}
```

## Why

Rust is fast, safe and has a fantastic ecosystem, but its syntax is dense. brs keeps Rust's
semantics — ownership, borrowing, traits, zero-cost abstractions — and gives it the syntax
Kotlin and TypeScript developers already know:

| You write (brs) | brs generates (Rust) |
|---|---|
| `val x = 1` / `var y = 2` | `let x = 1;` / `let mut y = 2;` |
| `fun area(): f64 = w * h` | `fn area(&self) -> f64 { self.w * self.h }` |
| `data class Point(x: f64, y: f64)` | struct + constructor + `#[derive(Debug, Clone, Copy, PartialEq)]` |
| `"Hello, $name! ${x + 1}"` | `format!("Hello, {name}! {}", x + 1)` |
| `String?`, `null`, `a?.b ?: c` | `Option<String>`, `None`, `map`/`and_then`/`unwrap_or` |
| `fun f(): T throws`, `throw e` | `-> Result<T, AnyError>`, `return Err(e.into())` |
| `try { ... } catch (e) { ... }` | a labeled block with `?` turned into `break` |
| `error enum E { NotFound(p: String) = "no $p" }` | enum + `Display` + `Error` + `From` impls |
| `when (x) { 1, 2 -> a; in 3..=9 -> b; else -> c }` | `match` |
| `xs.map { it * 2 }` | `xs.map(\|it\| it * 2)` |
| `interface Shape { fun area(): f64 }` | `trait Shape { fn area(&self) -> f64; }` |
| `fun String.shout() = ...` | an extension trait, imported everywhere automatically |
| `operator fun plus(o: V): V` | `impl Add for V` |

Method receivers are inferred: a method that changes a field takes `&mut self`, one that only
reads takes `&self`, a function that returns `Self` without touching `self` is static.

## Install

Requirements: Rust 1.80+ (`rustup`).

```bash
git clone <this repository> better-rust && cd better-rust
cargo install --path crates/brs-cli
```

This installs the `brs` command (compiler driver, formatter and language server).

## Quick start

```bash
brs new hello && cd hello
brs run
```

Or run a single file as a script, with crates declared in its header:

```kotlin
// dep: rand = "0.8"
fun main() = println("dice: ${rand.random::<u8>() % 6 + 1}")
```

```bash
brs run dice.brs
```

| Command | What it does |
|---|---|
| `brs new <name> [--lib]` | Create a project (`Cargo.toml`, `build.rs`, `src/main.brs`) |
| `brs run [file.brs] [-- args]` | Build and run the project or a script |
| `brs build` / `brs check` | Build / type-check |
| `brs test [name]` | Run `test "name" { }` blocks |
| `brs fmt [--check]` | Format `.brs` files |
| `brs emit file.brs` | Show the generated Rust |
| `brs lsp` | Language server for editors |

Compiler errors — brs's own and rustc's — point at your `.brs` lines:

```
error[E0308]: mismatched types
 --> src/main.brs:7:21
  |
7 |     val s: String = add(1, 2)
  |                     ^^^^^^^^^ expected `String`, found `i32`
```

## 100% Rust compatible

A brs project *is* a Cargo crate. `build.rs` runs the brs compiler and `src/main.rs` includes
the generated code, so `cargo build`, `cargo test`, `cargo doc`, rust-analyzer, clippy and
every crate on crates.io work unchanged. You can:

* use any crate: `import serde::{Serialize, Deserialize}` then `@derive(Serialize, Deserialize)`;
* mix `.rs` and `.brs` files in one crate and call between them in both directions;
* write a library in brs and use it from Rust (and vice versa);
* drop down to Rust anywhere with `rust { ... }` blocks or `macro!(...)` calls.

## Performance

brs adds no runtime and no hidden allocations or clones; each construct lowers to the Rust
you would write by hand. The [`bench`](bench) crate implements the same workloads in brs and
in idiomatic Rust (`cargo run --release -p brs-bench`):

| workload | brs | Rust | ratio |
|---|---|---|---|
| n-body | 5.28 ms | 5.28 ms | 0.999 |
| word frequency (HashMap) | 11.34 ms | 11.51 ms | 0.986 |
| prime sieve | 4.97 ms | 4.99 ms | 0.997 |
| dynamic dispatch | 20.55 ms | 20.69 ms | 0.993 |
| error handling | 16.41 ms | 16.25 ms | 1.010 |
| recursion (fib) | 2.49 ms | 2.50 ms | 0.994 |
| sorting structs | 13.55 ms | 13.49 ms | 1.004 |

Differences are within timing noise. See [docs/performance.md](docs/performance.md).

## Editor support

* **Kuana**: the extension in [`editors/kuana-brs`](editors/kuana-brs) gives highlighting,
  outline, ▶ run/test buttons, snippets and the language server.
* **Any LSP editor**: run `brs lsp` for diagnostics, completion, hover (with the generated Rust),
  go to definition, references, rename, signature help, formatting, symbols and inlay hints.
* **Tree-sitter**: the grammar is in [`editors/tree-sitter-brs`](editors/tree-sitter-brs).

## Documentation

* [A tour of brs](docs/guide.md) — start here
* [Language reference](docs/reference.md) — every construct and the Rust it becomes
* [Error handling](docs/errors.md)
* [Coming from Kotlin or TypeScript](docs/cheatsheet.md)
* [Rust interop](docs/rust-interop.md) — crates, mixed projects, calling brs from Rust
* [Tooling](docs/tooling.md) — CLI, project layout, formatter, language server, editors
* [Performance](docs/performance.md)
* [Compiler internals](docs/internals.md) — for contributors

## Examples

[`examples/`](examples) has complete programs: a calculator (lexer, recursive-descent parser,
evaluator), a todo CLI with file storage, a bank with interfaces and typed errors, Game of
Life, a word counter, async tasks with tokio, a JSON API with serde, a geometry library used
from Rust, and a Rust program with brs modules.

## Repository layout

```
crates/brs-compiler   lexer, parser, semantic analysis, lowering to Rust, formatter
crates/brs-build      build.rs helper used by brs projects
crates/brs-cli        the `brs` command and language server
editors/              tree-sitter grammar and Kuana extension
examples/             example projects (also run by the test suite)
bench/                brs vs Rust benchmarks
tests/                end-to-end and diagnostics tests
docs/                 documentation
```

Run the tests with `cargo test --workspace`.
