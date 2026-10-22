# Tooling

## Installing

```bash
cargo install --git https://github.com/berkayakin/oxie oxie-cli
```

installs `oxie` into `~/.cargo/bin`. New projects and scripts take the `oxie-build` crate from
the repository on GitHub, so their first build needs network access. Cargo pins the commit in
the project's `Cargo.lock`; `cargo update -p oxie-build` moves it to the latest.

### Working on Oxie

```bash
git clone https://github.com/berkayakin/oxie.git && cd oxie
cargo install --path crates/oxie-cli
export OXIE_HOME="$PWD"
```

With `OXIE_HOME` set to a checkout, new projects and scripts use its `crates/oxie-build` by
path instead, so they build with your changes and without network access. The test suite sets
it to the checkout it runs in.

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
| `oxie from-rust file.rs [-o out.oxi]` | Translate Rust to Oxie (`-` reads stdin); see [Translating Rust](#translating-rust) |
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

Scripts are built in `~/.cache/oxie` (set `OXIE_CACHE` to change it) with a shared target
directory, so only the first run of a script compiles its dependencies.

### Translating Rust

`oxie from-rust` turns Rust (a file from your crate, an example from a crate's docs, an answer
from Stack Overflow) into idiomatic Oxie, printed to stdout or written with `-o`:

```bash
oxie from-rust src/point.rs -o src/point.oxi
pbpaste | oxie from-rust -
```

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
struct Point { x: f64, y: f64 }

impl Point {
    fn new(x: f64, y: f64) -> Self { Point { x, y } }
    fn scale(&mut self, k: f64) { self.x *= k; self.y *= k; }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "({}, {})", self.x, self.y) }
}

fn parse_point(s: &str) -> Result<Point, Box<dyn std::error::Error>> {
    let (a, b) = s.split_once(',').ok_or("missing comma")?;
    Ok(Point::new(a.trim().parse()?, b.trim().parse()?))
}
```

becomes

```oxie
data class Point(x: f64, y: f64) {
    fun scale(k: f64) {
        x *= k
        y *= k
    }

    override fun toString() = "($x, $y)"
}

fun parse_point(s: &str): Point throws {
    val (a, b) = s.split_once(',').ok_or("missing comma")?
    Point(a.trim().parse()?, b.trim().parse()?)
}
```

* Structs and their `impl` blocks become classes (a `data class` when the derives are exactly
  the ones Oxie adds and `new` takes every field); traits become interfaces; enums get their
  methods in the enum body. `Display` impls that only `write!` become `toString()`, `impl Add`
  and friends become `operator fun`s, and `#[cfg(test)]` modules become `test` blocks.
* `Result` returns become `throws` (`Ok(v)` → `v`, `return Err(e)` → `throw e`); `Option<T>` is
  `T?` and `None` is `null`; `match` is `when`; `if let Some(x) = e` is `if (val x = e)`.
* `format!`, `println!` and friends use string templates; closures become lambdas (`{ it * 2 }`
  for simple one-parameter bodies) and move outside the parentheses when last; `vec![..]`,
  `HashMap::from([(k, v)])` and `HashSet::from([..])` become `vec[..]`, `mapOf(k to v)` and `setOf(..)`.
* Receivers, `self.`, `Type::new` and literal `.to_string()`s are dropped where Oxie restores
  them. The translation is compiled with the Oxie compiler before it is printed: receivers and
  derives Oxie would infer differently are written out, and items Oxie can't express are kept as
  Rust in `rust { }` blocks with a `// TODO(from-rust)` comment (also reported on stderr).
* Comments are kept. A `Box<dyn Error>` becomes `AnyError` (`Box<dyn Error + Send + Sync>`), so a
  failing `main` prints `error: ...` instead of Rust's `Error: ...` debug output.

## Project layout

```
Cargo.toml          [build-dependencies] oxie-build = { git = "https://github.com/berkayakin/oxie" }
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
