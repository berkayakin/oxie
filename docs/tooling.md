# Tooling

## Installing

Oxie needs Rust 1.82 or newer with Cargo ([rustup](https://rustup.rs)): Oxie programs are
compiled by rustc, so a prebuilt `oxie` needs them too. Crates set their own minimum Rust
version (the current `tokio` needs 1.85), and Rust features need the Rust version that
introduced them.

Each of these installs `oxie` into `~/.cargo/bin`:

```bash
# macOS and Linux
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/oxie-lang/oxie/releases/latest/download/oxie-installer.sh | sh
```

```powershell
# Windows
powershell -ExecutionPolicy Bypass -c "irm https://github.com/oxie-lang/oxie/releases/latest/download/oxie-installer.ps1 | iex"
```

```bash
# with cargo-binstall, which downloads the prebuilt binary
cargo binstall oxie

# or build it from source
cargo install oxie
```

The installers and `cargo binstall` download a prebuilt binary from the
[releases page](https://github.com/oxie-lang/oxie/releases) (macOS on Apple silicon and Intel, Linux on x86-64 and ARM, Windows on
x86-64), which also has checksums. `cargo install` compiles it, which takes a minute or two.
Running the same command again updates `oxie`.

New projects and scripts depend on the `oxie-build` release from crates.io that matches the
installed `oxie` (`oxie-build = "0.1.0"` for `oxie` 0.1.0). After updating `oxie`,
`cargo update -p oxie-build` brings a project's build helper up to date.

### Working on Oxie

```bash
git clone https://github.com/oxie-lang/oxie.git && cd oxie
cargo install --path crates/oxie-cli
export OXIE_HOME="$PWD"
```

With `OXIE_HOME` set to a checkout, new projects and scripts use its `crates/oxie-build` by
path instead, so they build with your changes. The test suite sets
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
error[E0382]: `name` was moved, so it can't be used anymore
 --> src/main.oxi:7:30
  |
6 |     register(names, name)
  |                     ---- `name` moved into `register()` here
7 |     println("${names.len()} $name")
  |                              ^^^^ used here after the move
  = help: giving a value to a function moves it: `register` keeps it, and `name` can't be used
    afterwards. Pass a copy with `name.clone()`.
  = fix: pass a copy of `name`: `register(names, name.clone())`
  = rustc: borrow of moved value: `name`
```

All labeled spans of a message are shown on their `.oxi` lines ("moved here", "borrowed
here", "still used here"). The errors newcomers meet most are rewritten in Oxie terms, with
fixes:

| rustc error | Oxie explanation and fix |
|---|---|
| use of moved value (E0382) | where it moved; pass a copy, or let the function borrow |
| assign twice / borrow as mutable (E0384, E0596) | it's a `val`; declare it with `var` |
| behind a `&` reference (E0596) | the parameter needs `&mut` (callers updated) |
| borrow conflicts (E0499, E0502, E0505, E0506) | where it's borrowed and still used; copy instead of borrowing |
| a loop over a field while the body changes `this` (E0502) | loop over a copy of the field |
| not thread-safe (E0277 `Send`/`Sync`, a future held across `await`) | declare the `shared class` `@sync` |
| lambda may outlive (E0373) | make it a `move` lambda |
| `?` without `Result` (E0277) | add `throws` to the function |
| no `Display` (E0277) | print with `:?`, or add `toString` |
| missing trait bound (E0277) | `require T: Bound` |
| no method `size`, `toUpperCase`, ... (E0599) | the Rust name (`len`, `to_uppercase`) |
| trait / type not imported (E0599, E0433, E0412) | `import` the suggested path |
| unused `var` | declare it with `val` |

rustc's own suggestions are kept as fixes when they map exactly onto the `.oxi` source, and
the original message is shown last (`rustc: ...`) so it can still be searched for. In editors
the fixes are code actions (quick fixes).

Each generated file has a source map next to it (`main.rs.map`) used for the translation.

The maps are used at run time too: when a program run with `oxie run` or `oxie test` panics,
the message, the backtrace frames (`RUST_BACKTRACE=1`) and failed assertions in tests point at
`.oxi` lines instead of the generated Rust:

```
thread 'main' panicked at src/main.oxi:12:5:
index out of bounds: the len is 3 but the index is 7
```

To do this, `oxie` reads the program's error output (and, for `oxie test`, its normal output)
as it is written and passes it on. A program run with `cargo run` writes straight to the
terminal and reports the generated file.

### Scripts

A `.oxi` file outside a project runs as a script. Its dependencies go in header comments:

```oxie
// dep: rand = "0.8"
// dep: serde = { version = "1", features = ["derive"] }
fun main() = println(rand.random::<u32>())    // output varies
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
Cargo.toml          [build-dependencies] oxie-build = "0.1.0"
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
| Diagnostics | Oxie errors on every keystroke; rustc errors and warnings (mapped to `.oxi`, explained in Oxie terms, with related locations) after `cargo check` on open and save |
| Quick fixes | code actions for the fixes listed under [Diagnostics](#diagnostics) (`var` instead of `val`, `.clone()`, `&mut` parameters, imports, `move` lambdas, `throws`, ...) |
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

### VS Code

The extension in [`editors/vscode-oxie`](../editors/vscode-oxie) adds:

* syntax highlighting (a TextMate grammar), with `rust { }` blocks highlighted as Rust and
  ` ```oxie ` blocks in Markdown files;
* the language server; if `oxie` isn't on the `PATH` or in `~/.cargo/bin`, it offers to run
  `cargo install oxie`;
* ▶ Run and Run --release above `fun main`, ▶ Run Test above each `test` block, and Run File,
  Run File (Release) and Run Tests in File in the editor's run menu;
* **Show Generated Rust**: the Rust for the current file beside it, updated as you type;
* `oxie build`, `run`, `test`, `check` and `fmt` tasks for Oxie projects (Terminal → Run Task);
* snippets, the same as Kuana's.

Settings: `oxie.path` (the `oxie` command, for example a checkout's `target/debug/oxie`),
`oxie.cargoCheck` (`cargo check` on open and save) and `oxie.trace.server`. In Restricted Mode
only highlighting and snippets work, because building runs the project's build scripts.

Install: build the package with `npm install` and `npm run package` in `editors/vscode-oxie`,
then run `code --install-extension oxie-0.1.0.vsix` (or use **Extensions: Install from VSIX…**).

### Other editors

Any editor with LSP support can use `oxie lsp` for `*.oxi` files. For syntax highlighting, use
the tree-sitter grammar in `editors/tree-sitter-oxie` (Neovim, Helix, Zed), the TextMate grammar
`editors/vscode-oxie/syntaxes/oxie.tmLanguage.json` (JetBrains IDEs through TextMate Bundles,
Shiki), or Kotlin highlighting as a fallback.
