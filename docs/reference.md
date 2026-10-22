# brs language reference

brs is a syntax for Rust. Every construct below lowers to ordinary Rust; the semantics
(ownership, borrowing, traits, generics, `Option`/`Result`, iterators, macros) are Rust's.
`brs emit file.brs` prints the generated Rust for any file.

Contents: [Lexical structure](#lexical-structure) · [Declarations](#declarations) ·
[Functions](#functions) · [Types](#types) · [Classes](#classes) · [Enums](#enums) ·
[Interfaces](#interfaces-and-impl-blocks) · [Extensions and operators](#extension-functions) ·
[Statements](#statements) · [Expressions](#expressions) · [Strings](#strings) ·
[Null safety](#null-safety) · [Errors](#errors) · [Control flow](#control-flow) ·
[Lambdas](#lambdas) · [Collections](#collections) · [Modules](#modules-and-imports) ·
[Visibility](#visibility) · [Attributes](#attributes) · [Tests](#tests) · [Async](#async) ·
[Rust escape hatches](#rust-escape-hatches) · [Keywords](#keywords)

---

## Lexical structure

* **Comments**: `// line`, `/* block (nestable) */`, `/// doc comment` (becomes a Rust doc
  comment), `//! module doc`.
* **Statements end at a newline.** `;` may separate statements on one line. A line continues
  when it ends with an operator, `,`, `.`, `(`, `[` or `{`, when the next line starts with
  `.`, `?.`, `?:`, `&&` or `||`, and everywhere inside `(...)` and `[...]`:

  ```kotlin
  val total = items
      .iter()
      .map { it.price }
      .sum::<f64>()
  ```

* **Identifiers** are Rust identifiers. Rust keywords that brs does not reserve (`type`,
  `match`, `loop`, ...) can be field and method names; `r#name` works as in Rust.
* **Numbers**: Rust literals (`1_000`, `0xff`, `1e-3`, `5u8`, `2.0f32`) plus Kotlin's `10L` (`i64`).
* **Characters**: `'a'`, `'\n'`, `'\u{1F600}'`, byte `b'a'`.
* **Strings**: see [Strings](#strings). Raw strings `r"..."`, `r#"..."#` and byte strings
  `b"..."` are Rust's.

## Declarations

| brs | Rust |
|---|---|
| `val x = 1` | `let x = 1;` |
| `var x = 1` | `let mut x = 1;` |
| `val x: T = v` | `let x: T = v;` |
| `val (a, b) = pair` | `let (a, b) = pair;` |
| `var (a, b) = pair` | `let (mut a, mut b) = pair;` |
| `let`/`let mut` (Rust style) | unchanged |
| `val Some(x) = opt else { return }` | `let Some(x) = opt else { return };` |
| top-level `const NAME = 1` / `val NAME = 1` | `const NAME: i32 = 1;` (type inferred from literals) |
| `static NAME: T = v` / `static var NAME: T = v` | `static` / `static mut` |
| `typealias Name = T` / `type Name = T` | `type Name = T;` |

## Functions

```kotlin
fun name<T: Bound>(param: Type, other: Type = default): ReturnType throws Error where T: Other {
    body
}
fun short(x: i32) = x * 2
```

* **Return type**: `: T` (or Rust's `-> T`). Omitted means `()`, except for an expression
  body whose type is obvious (a literal, a template, arithmetic on typed values, a call to a
  function with a declared type, a constructor, ...), which brs infers.
* **Block value**: the last expression of a block is its value. In a function returning `()`
  the last expression is a statement.
* **Default arguments** and **named arguments** (`f(x = 1)`) work for functions, methods and
  constructors declared in brs; defaults are filled in at the call site.
* **Generics and bounds** are Rust's: `<T: Display + Clone>`, `where T: Into<String>`,
  `<'a>`, `<const N: usize>`. Kotlin's `fun <T> name()` order is accepted too.
* **Modifiers**: `async fun`, `unsafe fun`, `const fun`, `extern "C" fun`.
* **Automatic lifetimes**: when a function returns a reference and Rust cannot infer which
  parameter it borrows from, brs ties the result to all reference parameters
  (`fun longest(a: &str, b: &str): &str` → `fn longest<'a>(a: &'a str, b: &'a str) -> &'a str`).
  With no reference parameters the result is `'static`. Types with lifetime parameters get
  `<'_>` in signatures.
* **Function-typed parameters**: `f: (i32) -> i32` becomes `impl FnMut(i32) -> i32` (or
  `impl Fn` when the function is called from inside a closure); as a field, a return value
  stored in a variable, or a generic argument it becomes `Box<dyn Fn(i32) -> i32>`, and
  lambdas passed to it are boxed automatically.
* `fun main() throws` prints the error with its causes and exits with status 1.

## Types

| brs | Rust |
|---|---|
| `i32`, `u8`, `f64`, `bool`, `char`, `str`, `String`, ... | same |
| `Int`, `Long`, `Short`, `Byte`, `UInt`, `ULong`, `Double`, `Float`, `Boolean`, `Char`, `Unit` | `i32`, `i64`, `i16`, `i8`, `u32`, `u64`, `f64`, `f32`, `bool`, `char`, `()` |
| `T?` | `Option<T>` (`&T?` is `Option<&T>`) |
| `(A, B) -> R` | `impl FnMut(A, B) -> R`, `Box<dyn Fn(A, B) -> R>` or `dyn Fn(..)` depending on position |
| `Shape` (an interface used as a type) | `impl Shape` (parameters, returns), `dyn Shape` (behind `&`, `Box`, `Rc`, `Arc`), `Box<dyn Shape>` (fields, collections) |
| `&T`, `&mut T`, `[T]`, `[T; N]`, `(A, B)`, `Vec<T>`, `dyn T`, `impl T`, `*const T` | same |
| `std.io.Error` | `std::io::Error` (dots or `::` in paths) |

The standard traits used as types the same way are `Display`, `Debug`, `Error`, `Iterator`,
`IntoIterator`, `Read`, `Write`, `BufRead`, `Future`, `Any`, `ToString` and every interface
declared in brs.

## Classes

```kotlin
class Name<T>(val a: A, var b: B = default, private c: C) : Interface, Hash {
    var d: D = default            // body property (needs a default)
    init { ... }                  // runs in the constructor after fields are set

    fun read(): A = a             // &self
    fun write(x: B) { b = x }     // &mut self (inferred)
    static fun create(): Name<T>  // no self
    companion object { fun other(): Name<T> = ... }   // also static
    override fun method() { ... } // implements `Interface`
}
```

* Generates `struct Name<T> { a, b, c, d }` and `impl` with `fn new(a, b, c) -> Self`.
* **Construction**: `Name(a, b)` → `Name::new(a, b)`; named arguments and defaults work.
  Zero-argument calls of external types (`HashMap()`, `String()`) become `Type::new()`.
* `val`/`var` before constructor parameters are accepted (Kotlin habit) but don't matter:
  mutability belongs to the variable holding the object, as in Rust.
* **Implicit `this`**: inside methods, fields and methods of the class (and default methods of
  its interfaces) can be used without `this.`; a parameter or local with the same name wins.
  `this` and `self` are the same.
* **Receivers** are inferred:
  * `&mut self` if the method assigns a field, calls a mutating method on a field (`push`,
    `insert`, `clear`, `entry`, ...), takes `&mut` of a field, or calls a `&mut self` method;
  * static if it never uses `self` and returns `Self`/the class, or is named `new`, `default`,
    `from`, `from_str`, `try_from` or `from_iter`;
  * `&self` otherwise.
  Override with `mut fun`, `static fun`, or an explicit `self`, `&self`, `&mut self`,
  `mut self` first parameter. Interface methods take the receiver their implementations need.
* **`data class`** additionally derives `Debug`, `Clone`, `PartialEq`, and `Copy`, `Eq`,
  `Hash` when every field type supports them, and supports `x.copy(field = value)`.
* **Supertypes** after `:` are traits to implement. Derivable traits (`Debug`, `Clone`,
  `Copy`, `PartialEq`, `Eq`, `Hash`, `Default`, `PartialOrd`, `Ord`) without methods in the
  body become derives. `override fun toString() = "..."` implements `Display`.
* `@derive(...)` adds derives; `@no_auto_derive` turns the automatic ones off.
* Rust-style `struct Point { x: f64, y: f64 }`, tuple structs `struct Meters(f64)` and unit
  structs `struct Marker` are supported.
* `object Name { ... }` is a namespace (a Rust module) of constants and functions:
  `Name.function()`.
* Classes, enums and interfaces declared inside a class body are placed next to the class;
  refer to them as `Inner` or `Outer.Inner`.

## Enums

```kotlin
enum Shape : Display {
    Circle(radius: f64)          // named fields → Circle { radius: f64 }
    Pair(i32, i32)               // positional fields
    Empty                        // unit variant
    Coded = 3                    // explicit discriminant (unit-only enums)

    fun area(): f64 = when (this) { Circle(r) -> 3.14 * r * r; else -> 0.0 }
}
```

* Variants are separated by newlines or commas; methods follow in the same body.
* `Shape.Circle(1.0)` or `Circle(1.0)` (when the name is unique, or declared in the same file)
  constructs; named-field variants accept positional or named arguments.
* In patterns, `Circle(r)` matches named-field variants positionally (→ `Circle { radius: r }`),
  and a bare `Circle` matches any `Circle`.
* Enums derive `Debug`, `Clone`, `PartialEq` (and `Copy`, `Eq`, `Hash` when possible).
* `error enum` — see [Errors](#errors).

## Interfaces and impl blocks

```kotlin
interface Named : Display {
    fun name(): String                 // required
    fun greet(): String = "hi ${name()}" // default
    type Id                            // associated type
    const MAX: u32 = 10                // associated constant
}
```

* `interface` (or `trait`) → `trait`. Methods take `&self` unless declared otherwise (or an
  implementation needs `&mut self`).
* A class lists interfaces after `:` and implements methods with `override fun`; the
  matching `impl Trait for Class` is generated. Methods named like the trait's (and `type`
  members) go to the right trait even without `override`.
* Rust `impl Type { }` and `impl Trait for Type { }` blocks are supported, also for types not
  declared in brs (`impl Display for Point { fun fmt(f: &mut Formatter): fmt::Result = ... }`).
  Inside, methods follow the same receiver rules and fields are available implicitly when the
  type is a brs class.

## Extension functions

```kotlin
fun String.shout(): String = self.to_uppercase()
fun <T: Clone> Vec<T>.second(): T? = self.get(1).cloned()
mut fun Vec<i32>.bump(by: i32) { for (x in self.iter_mut()) *x += by }
```

Each becomes a trait with one method plus its `impl` for the receiver type; every file of the
crate imports these traits automatically. Inside, `this`/`self` is the receiver; for brs
classes their fields can be used directly.

### Operators

`operator fun` inside a class or enum implements a `std::ops` trait:

| brs | trait | | brs | trait |
|---|---|---|---|---|
| `plus` | `Add` | | `plusAssign` | `AddAssign` |
| `minus` | `Sub` | | `minusAssign` | `SubAssign` |
| `times` | `Mul` | | `timesAssign` | `MulAssign` |
| `div` | `Div` | | `divAssign` | `DivAssign` |
| `rem` | `Rem` | | `remAssign` | `RemAssign` |
| `unaryMinus` | `Neg` | | `not` | `Not` |
| `shl`, `shr`, `and`, `or`, `xor` | `Shl`, `Shr`, `BitAnd`, `BitOr`, `BitXor` | | | |

Rust names (`add`, `sub`, `mul`, `neg`, `add_assign`, ...) work as well. The parameter type is
the right-hand side (`impl Mul<f64> for Vec2` for `operator fun times(k: f64)`).

## Statements

* Variable declarations (see [Declarations](#declarations)).
* Expressions, including assignments `a = b`, `a += b` (all compound operators).
* Local `fun`, `class`, `enum`, ... declarations.
* `return`, `break ['label] [value]`, `continue ['label]`, `throw e`.

## Expressions

| brs | Rust |
|---|---|
| `a == b`, `a < b`, `a && b`, `a + b`, ... | same (Rust precedence) |
| `x in coll` / `x !in coll` | `coll.contains(&x)` / `!coll.contains(&x)` |
| `x in a..b` | `a <= x && x < b` |
| `x is Pattern` / `x !is Pattern` | `matches!(x, Pattern)` / `!matches!(..)` |
| `a to b` | `(a, b)` |
| `a until b`, `a downTo b`, `r step n` | `a..b`, `(b..=a).rev()`, `r.step_by(n)` |
| `a..b`, `a..=b`, `a..<b` | `a..b`, `a..=b`, `a..b` |
| `x as T` | `x as T` |
| `x?` | `x?` |
| `x!!` | `x.unwrap()` |
| `a ?: b` | `a.unwrap_or(b)`-like; see [Null safety](#null-safety) |
| `a?.b` | safe navigation or error propagation; see [Null safety](#null-safety) |
| `await f()` / `f().await` | `f().await` |
| `Type.member`, `module.item` | `Type::member`, `module::item` |
| `f<T>(x)`, `mutableListOf<i32>()` | `f::<T>(x)`, `Vec::<i32>::new()` |
| `Point { x: 1, y: 2 }` | struct literal (with `..base`) |
| `x.let { it + 1 }` | `{ let it = x; it + 1 }` |
| `p.copy(x = 1)` | clone with `x` replaced |
| `if (c) a else b`, `when`, `try`, blocks | expressions with values |

**Strings and `+`**: when an operand of a `+` chain is a string literal or template, the chain
builds a `String` with `format!` (`"total: " + n` works for any `Display` value).

**Implicit conversions** happen only where Rust would otherwise reject the code, and only to
the declared type of a variable, field, parameter or return value:

* a string literal to `String` (`String::from("...")`), a `&str` value to a `String`
  parameter (`.into()`);
* a value to `T?` (`Some(...)`, or `.into()` for variables), `null` to `None`;
* a lambda to a function-typed field (`Box::new(...)`), a constructed value to an interface-typed
  field or collection (`Box::new(...)`);
* a `vec[...]` to `Vec<T>` element by element.

## Strings

```kotlin
"Hello, $name!"            // format!("Hello, {name}!")
"sum: ${a + b}"            // format!("sum: {}", a + b)
"${value:.2} ${list:?}"    // format specs after `:` (Rust's format syntax)
"cost: \$5"                // literal dollar sign
"""
    multi-line
      text with $vars
    """                    // common indentation and the first/last empty lines are removed
```

* A literal without `$` is a `&'static str`; a template is a `String`.
* Inside `println`, `print`, `eprintln`, `format`, `panic`, `assert`, `write`, `writeln`, ...
  a template is spliced directly into the format string (no extra allocation).
* A string literal passed to these with extra arguments is a Rust format string:
  `println("{} items", n)`.
* `println(a, b)` prints both separated by a space; `println(x)` prints any `Display` value.

## Null safety

* `T?` is `Option<T>`, `null` is `None`.
* `a?.b`, `a?.m()`, `a?.b?.c` — **safe navigation** when `a` is optional: the result is
  `None` if `a` is. Chains flatten like Kotlin (`?.` on a `T?` property continues the chain;
  `?.` after a non-optional property acts like `.`). Reading a field through a borrowed value
  gives a reference (`Option<&T>`).
* When the left side is a `Result` (a call to a `throws` function, `.await`, `parse()`,
  `File::open(...)`, ...), `a?.b` is Rust's `?` followed by `.b`: the error is propagated.
  For a left side of unknown type, `?.` propagates errors inside functions that return
  `Result` and navigates otherwise.
* `a ?: b` — `a` if present (`Some`/`Ok`), else `b`, evaluated only when needed. `b` may be
  `return`, `throw`, `break`, `continue` or `panic(...)`. For a literal `b`, `Option<&T> ?: 0`
  yields `T` and `Option<String> ?: "text"` yields `String`.
* `x!!` — unwrap.
* `if (val v = opt) { }` / `while (val v = opt)` — `if let Some(v)` / `while let Some(v)`;
  with a pattern (`if (val Ok(v) = r)`) the pattern is used as written.
* `x?.let { ... }` runs the block when `x` is present.

## Errors

See [Error handling](errors.md) for the full guide.

| brs | Rust |
|---|---|
| `fun f(): T throws` | `-> Result<T, AnyError>` (`Box<dyn Error + Send + Sync>`) |
| `fun f(): T throws E` | `-> Result<T, E>` |
| `return v` / tail `v` in a throwing function | `return Ok(v)` / `Ok(v)` |
| `throw e` | `return Err(e.into())` |
| calling a throwing brs function as a statement or result | adds `?` |
| `try { a()?; b } catch (e: E) { c }` | labeled block; `?`/`throw` inside break out with `Err` |
| `error enum E { V(f: T) = "msg $f" }` | enum + `Display` + `Error` (+ `source`) + `From` |
| `error class E(f: T) = "msg $f"` | struct + `Display` + `Error` |
| `x.context("msg")` / `.with_context { ... }` | wrap the error with a message |

## Control flow

```kotlin
if (cond) a else b                  // also `if cond { }` (Rust style)
if (val x = opt) { } else { }
when (x) {                          // match
    1, 2 -> "one or two"            // alternatives
    in 3..=9 -> "small"             // range (or any `contains`)
    is Some(n) if n > 0 -> "guard"  // `is` optional, `if` guard
    else -> "other"                 // `_`
}
when { a < b -> ...; else -> ... }  // if/else chain
when (val r = compute()) { ... }    // binds the subject
match x { Some(v) => v, None => 0 } // Rust match syntax works too
for (item in items) { }             // also `for item in items { }`
for ((i, x) in xs.iter().enumerate()) { }
while (cond) { }
do { } while (cond)
loop { }
'outer: for (...) { break 'outer }
```

* A statement `when` on literals doesn't need `else` (brs adds `_ => {}`); a `when` used as a
  value must be exhaustive, as in Rust.
* `if`/`for`/`while` bodies can be a single statement without braces.

## Lambdas

```kotlin
{ it * 2 }                      // one implicit parameter `it`
{ a, b -> a + b }               // Kotlin style
{ (key, value) -> key }         // destructuring
{ x: i32 -> x * 2 }             // typed
(a, b) => a + b                 // TypeScript style
x => x * 2
|a, b| a + b                    // Rust style
move { ... }                    // capture by value
f(x) { trailing lambda }        // last argument outside the parentheses
```

* A lambda without `->` has no parameters, or `it` if it uses `it`. When a standard method
  always passes a value (`map`, `filter`, `map_err`, `unwrap_or_else` on results, ...), an
  unused parameter is added for you (`map_err { MyError() }`).
* A lambda body is a block; its last expression is the result.
* A lambda stored in a `val` that assigns to captured variables is declared `mut`
  automatically.
* `run { ... }` is a block expression; `repeat(n) { ... }` loops `n` times with `it`.

## Collections

| brs | Rust |
|---|---|
| `vec[1, 2, 3]`, `listOf(1, 2, 3)`, `mutableListOf()` | `vec![1, 2, 3]`, `Vec::new()` |
| `vec[0; n]` | `vec![0; n]` |
| `[1, 2, 3]`, `arrayOf(...)` | array `[1, 2, 3]` |
| `mapOf(k to v, ...)` | `HashMap::from([(k, v), ...])` |
| `setOf(a, b)` | `HashSet::from([a, b])` |
| `m[key]`, `v[i]`, `&v[1..3]` | indexing and slicing |

All standard library types and methods are used with their Rust names.

## Modules and imports

* Each `.brs` file under `src/` is a module: `src/util.brs` → `util`, `src/net/http.brs` →
  `net::http`, `src/net/mod.brs` → `net`. `src/main.brs` (or `src/lib.brs`) is the crate root.
* `import a::b::C`, `import a.b.C`, `import a::b::{C, D as E}`, `import a::b::*`,
  `import { C, D } from a::b` (TypeScript), `use ...` (Rust). Paths that start with one of
  your modules are resolved from the crate root, so `import util::helper` works in every file.
* `mod name { ... }` declares an inline module; `mod name` (without a body) declares a
  hand-written Rust module file next to the source.
* Paths may use `.` instead of `::` when the left side is a type, module or crate name:
  `std.mem.swap(a, b)`, `Duration.from_secs(1)`, `Color.Red`.

## Visibility

| brs | Rust |
|---|---|
| (default) | `pub(crate)` — visible in the whole project |
| `pub` / `public` | `pub` — exported from a library |
| `private` | private to the file (module) |
| `internal` | `pub(crate)` |
| `pub(super)`, `pub(in path)` | same |

Members of a `pub` class or enum default to `pub`. Keeping items unexported by default lets the
optimizer treat them like private Rust functions; mark library APIs `pub`.

## Attributes

`@name` and `@name(args)` become `#[name]` / `#[name(args)]`: `@derive(Serialize)`,
`@serde(rename_all = "camelCase")`, `@inline`, `@cfg(test)`, `@tokio::main`, ... Rust's `#[...]`
syntax works too. brs-specific: `@no_auto_derive` (classes/enums), `@from` (error enum cases).
Crate-level `#![...]` attributes belong in `src/main.rs`/`src/lib.rs`.

## Tests

```kotlin
test "addition works" {
    assert_eq(1 + 1, 2)
}
test "can fail" throws { "1".parse::<i32>()? }
async test "needs tokio" { ... }      // add @tokio::test
```

Tests go into a `#[cfg(test)]` module with `#[test]` functions named after the description.
Run with `brs test` (or `cargo test`).

## Async

`async fun`, `async { }`, `async move { }`, `.await` and TypeScript-style prefix `await expr`.
Use any runtime: `@tokio::main async fun main()`. A throwing async `main` works too.

## Rust escape hatches

* `rust { ... }` — raw Rust items (at top level) or a raw Rust expression/statements.
* `name!(...)`, `name![...]`, `name!{...}` — any Rust macro, passed through unchanged.
  Common std macros can also be called without `!` (`println(...)`, `assert_eq(...)`,
  `vec[...]`, `format(...)`, `panic(...)`, `todo()`, `dbg(...)`, `matches(...)`, ...).
* `macro_rules!` definitions at top level.
* Rust syntax that brs shares (`let`, `match` with `=>`, `impl`, `fn`, `|x|` closures,
  `::<T>` turbofish, `if let`, `while let`, `'label:` loops, `#[attr]`) is accepted directly.

## Keywords

Reserved: `fun fn val var let if else when match for while do loop return break continue throw
try catch class interface trait enum struct impl import use const static typealias mod true
false null this self Self in is as where async move unsafe dyn mut ref extern pub`.

Contextual (usable as names elsewhere): `data`, `error`, `object`, `companion`, `init`,
`override`, `operator`, `private`, `internal`, `public`, `test`, `rust`, `throws`, `to`,
`until`, `downTo`, `step`, `await`, `it`.
