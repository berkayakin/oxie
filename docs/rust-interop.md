# Rust interop

A brs project is a Cargo crate and brs code is Rust code, so interop is not a bridge or an FFI
layer — it is the same compiler, the same types and the same crates.

## How a project is put together

```
myapp/
  Cargo.toml        normal manifest; brs-build is a build-dependency
  build.rs          brs_build::compile() — compiles src/**/*.brs to Rust in OUT_DIR
  src/main.rs       include!(concat!(env!("OUT_DIR"), "/brs/main.rs"));
  src/main.brs      your program
  src/util.brs      module `util`
```

`cargo build`, `cargo run`, `cargo test`, `cargo doc`, `cargo clippy` and rust-analyzer all work
on it. `brs run`/`brs build` call cargo and translate error locations to `.brs` lines.

## Using crates

Add dependencies to `Cargo.toml` as usual (`cargo add serde --features derive` works) and use
them:

```kotlin
// dep: serde = { version = "1", features = ["derive"] }
// dep: serde_json = "1"
import serde::{Deserialize, Serialize}

@derive(Serialize, Deserialize)
data class Config(name: String, port: u16, debug: bool = false)

fun main() throws {
    val c: Config = serde_json.from_str("""{"name": "api", "port": 8080, "debug": true}""")?
    println("${c.name} on ${c.port}")
    println(serde_json.to_string(&c.copy(port = 9090))?)
}
```

(The `// dep:` lines are only needed for single-file scripts; projects use `Cargo.toml`.)

* Attributes are written `@name(...)`; derive macros merge with brs's automatic derives.
* Crate paths can use `.` or `::`: `serde_json.from_str`, `tokio::spawn`, `rand.random()`.
* Macros from crates are called with `!` as in Rust: `json!({ "a": 1 })`, `tokio::join!(a, b)`.

## Calling Rust from brs

Hand-written Rust modules can live in the same crate. In a brs project, declare them from a
`.brs` file:

```kotlin
// not-runnable: needs src/legacy.rs next to this file
mod legacy                 // src/legacy.rs

fun main() = println(legacy::answer())
```

Any Rust function, type, trait or macro is then used directly. Rust-only syntax can be written
inline with `rust { ... }`.

## Calling brs from Rust

Everything brs declares is a normal Rust item. In a Rust crate, add brs modules with
`brs init` (inside the crate), then include the generated modules once:

```rust
// src/main.rs or src/lib.rs
include!(concat!(env!("OUT_DIR"), "/brs/mod.rs"));   // declares every src/*.brs module

fn main() {
    let invoice = pricing::Invoice::new("ACME".to_string(), vec![]);
    println!("{}", invoice);
}
```

See [`examples/mixed`](../examples/mixed): a Rust binary whose `pricing` module is written in
brs and itself calls back into Rust.

What the generated Rust looks like:

| brs | Rust you call |
|---|---|
| `data class P(x: f64)` | `struct P { pub(crate) x: f64 }`, `P::new(x)`, derives |
| `class C(a: A) { fun m() }` | `struct C`, `C::new(a)`, `c.m()` |
| `enum E { V(n: i32) }` | `enum E { V { n: i32 } }` |
| `interface I { fun f(): i32 }` | `trait I { fn f(&self) -> i32; }` |
| `fun f(): T throws` | `fn f() -> Result<T, Box<dyn Error + Send + Sync>>` |
| `fun String.ext()` | trait `__BrsExt_<module>_ext` implemented for `String` (import it to call) |
| `object O { fun f() }` | `mod O { pub(crate) fn f() }` |

## Writing a library in brs

Use `brs new mylib --lib` (root `src/lib.brs`). Mark the public API `pub`; members of a `pub`
class are public automatically:

```kotlin
// not-runnable: library code
pub data class Point(x: f64, y: f64) {
    fun distance(o: Point): f64 = ((x - o.x).powi(2) + (y - o.y).powi(2)).sqrt()
}

pub fun origin() = Point(0.0, 0.0)
```

Rust users depend on it like any crate: `geometry::Point::new(1.0, 2.0).distance(...)`.
[`examples/geometry`](../examples/geometry) has an integration test written in Rust.

## Generated code

* Generated files are in `target/<profile>/build/<crate>-*/out/brs/` (one `.rs` per `.brs`,
  plus `__brs.rs`, the tiny support module, and `.map` source maps).
* `brs emit file.brs` prints them.
* The support module contains only zero-cost helpers: the `AnyError` type alias, the `?:`
  helpers (`#[inline(always)]` trait methods on `Option`/`Result`), `.context()`, and the error
  printer for `main() throws`. There is no runtime.

## Using Rust tools

* **rust-analyzer** works on the generated code (useful for exploring types); the brs language
  server (`brs lsp`) works on `.brs` files.
* **Debuggers** (lldb, gdb) step through the generated Rust, which mirrors the `.brs` source
  closely.
* **clippy / rustfmt** run on the crate; generated code is formatted for reading but isn't meant
  to be edited.
