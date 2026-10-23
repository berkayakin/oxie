# Compiler internals

This document is for people working on Oxie itself.

## Pipeline

```
.oxi source
  │ lexer.rs      tokens with byte spans; significant newlines; string interpolation parts
  ▼
  │ parser.rs     recursive descent → ast.rs; error recovery per statement
  ▼
  │ sema.rs       crate-wide symbol table: classes, enums, traits, functions, modules,
  │               extension traits; receiver inference; automatic derives
  ▼
  │ lower.rs      items, types, patterns → Rust text
  │ lower_expr.rs statements and expressions → Rust text (with source-map marks)
  │ infer.rs      small type inference for obvious cases (return types, receivers)
  ▼
  │ compile.rs    per crate: module tree, `#[path]` module declarations, runtime module,
  │               source maps (.rs.map)
  ▼
generated .rs files → rustc
```

The compiler is a library (`oxie-compiler`) with no dependencies. `oxie-build` calls it from
`build.rs`; the `oxie` command (`crates/oxie-cli`) calls it for checking, `oxie emit`, formatting and the language server.

## Lexer

* Newlines are emitted as tokens only inside `{ }` blocks and at top level, and only when the
  previous token can end an expression and the next line doesn't start with `.`, `?.`, `?:`,
  `&&` or `||`. Inside `( )` and `[ ]` they are skipped. The parser therefore treats `Newline`
  as a statement terminator and skips it where lists continue.
* `>` is always a single token; the parser combines adjacent `>` `>` into `>>` when it is an
  operator, so generics like `Vec<Vec<i32>>` need no special casing.
* Strings are lexed into parts: literal text (already Rust-escaped) and interpolations with
  their source text and byte offset; the parser re-lexes and parses each interpolation, so
  spans inside templates are exact. `"""` strings are dedented while lexing.

## Parser

* One recursive-descent parser for Kotlin-, TypeScript- and Rust-style syntax. Ambiguities are
  settled with small lookaheads:
  * `if (` … `)` is a Kotlin condition unless the closing parenthesis is followed by a binary
    operator (then it is part of a Rust-style condition);
  * `{` in expression position is a lambda; after a lowercase name it is a trailing lambda;
    after a type name followed by `name:`/`name,`/`}` it is a struct literal;
  * `(a, b) =>` and `x =>` are arrow functions (found by scanning to the matching parenthesis);
  * `Type<Args>` in expressions is tried as generic arguments and backtracked if it isn't one.
* A brace lambda without parameters used as a statement (not the block's value) is an error,
  since it would never run; the help points to `run { }`.
* Errors are collected; after an error the parser skips to the end of the statement.
* `rust { }` blocks and `name!( )` macro calls keep their source text verbatim.

## Semantic analysis (`sema.rs`)

`collect` builds a `Crate` from all files of a crate:

* classes (fields, constructor parameters, methods, generics), enums (variants and their
  shapes), traits (methods, associated types), free functions, module names, extension traits,
  and which enum owns each variant name (for bare variant names in patterns and calls);
* **receiver inference**: a fixpoint over all methods. A method is `&mut self` if its body
  assigns into `self` (directly or through implicit fields), calls a known mutating method on
  a field, takes `&mut` of a field, or calls a `&mut self` method of the same type. Interface
  methods without an explicit receiver take `&mut self` if any implementation needs it, and
  then all implementations use it;
* **automatic derives**: a fixpoint deciding `Copy` and `Eq + Hash` for data classes and enums
  from their field types.

## Lowering

The `Lowerer` walks one file and writes Rust with an `Emitter` that tracks indentation and
records source-map marks (generated line/column → source offset) at every statement and
expression.

Contexts tracked while lowering:

* **scopes** of locals (with declared or inferred types) — needed for implicit `this`, deciding
  whether a name is a path root (`std.fs.read` → `std::fs::read`), and inline format captures;
* the **current class** (fields, methods with receivers) for implicit `this` and calls;
* the **function context** (does it throw, its value type, enclosing `try` labels) for `Ok`
  wrapping, `throw`, and `?` inside `try`;
* **loops** for `do { } while` with `continue`.

Notable translations:

* **Tail expressions** are emitted with a `Target` (expected type + optional `Ok` wrapper);
  `if`/`when`/blocks pass the target to each branch, so coercions (`Some(...)`,
  `String::from`) and `Ok(...)` are applied at the leaves.
* **`?.`** is first resolved per link (`resolve_question_dots`): links on known `Result`s become
  `Try` + member access, others are safe navigation lowered to `as_ref`/`map`/`and_then`,
  flattening where the field or method is declared optional.
* **`?:`** uses `ElvisOr::or_val` for literal defaults (handles `Option<&T> ?: 0` and
  `Option<String> ?: "x"`) and a `match` on `Elvis::opt` otherwise.
* **`try`/`catch`** becomes `match ('label: { ...; Ok::<_, E>(value) }) { ... }`; `?` and
  `throw` inside break out of the labeled block.
* **Let chains** (`val a = x && a > 0 && ...`) become nested `if`/`match` (the `else` branch is
  repeated where a part can fail), so they work in every Rust edition.
* **Crate-level lint attributes** in the root file are copied onto every top-level item and
  module declaration, because the generated root is `include!`d and can't hold `#![...]`.
* **Extension functions** become a trait per function; every file imports all of them as
  `_`.
* **Automatic borrowing** (`coerce` with a `&T`/`&mut T` parameter type): value arguments get
  `&`/`&mut`; references (`&x`, reference-typed locals, `self`, string literals, untyped
  lambda parameters) don't. Receiver inference (`sema.rs`) treats a field passed to a `&mut`
  parameter as a mutation of `self`.
* **Borrowed parameters** (`params.rs`, before lowering): a `String`/`Vec<T>` parameter of a
  non-`pub` top-level function becomes `&str`/`&[T]` when every use is a read (a method that
  exists on `str`/slices, printing, comparing, indexing, or passing it to a parameter that is
  itself borrowed). A greatest fixpoint over the crate handles forwarding and recursion; a
  `move` lambda or async block capturing the parameter keeps it owned. `kept_reason` reports the
  first use that keeps a parameter owned ("it returns `v`"), for the explanation of E0382.
* **Inferred bounds** (`bounds.rs`): before a function or method signature is emitted, its
  body is scanned for uses of values of a type parameter (parameters typed `T`/`&T`, elements of
  `&[T]`/`Vec<T>` in `for` loops, lambdas over such collections, `val y = x` aliases) and the
  matching `where` predicates are added.
* **Collection operations** are the `ListOps` trait in the runtime module, implemented for
  `[T]` and imported as `_` everywhere, so `xs.map { }` lowers to `xs.map(|it| ..)` unchanged.
  Untyped parameters of predicate lambdas (`filter`, `find`, `any`, ...) compared with a scalar
  literal are read with `crate::__oxie::val(it)` (the `Val` trait strips any number of
  references from numbers, `bool` and `char`).
* **Shared classes** (`shared.rs`): the class is lowered as `{Name}Data`, then the handle
  (`Rc<RefCell<..>>` or `Arc<Mutex<..>>`), forwarding methods, getters, setters and identity
  `PartialEq`/`Hash` are emitted. Expression lowering routes `obj.field` reads, assignments
  and calls on fields of known shared types through getters, setters or a single borrow,
  evaluating arguments before a mutable borrow; shared places used by value get `.clone()`.

## Source maps and diagnostics

Each generated file `x.rs` has `x.rs.map`:

```
oxie-sourcemap 1
/abs/path/src/x.oxi
<gen line> <gen col> <src line> <src col>
...
```

`crates/oxie-cli/src/cargo.rs` runs cargo with `--message-format=json`, maps the primary span of each
message (following macro expansions back to the call site) to the `.oxi` file, and renders it
in rustc's format. Secondary labels are mapped too; those that don't map exactly (the generated
text between the nearest mark and the span differs from the source) are dropped unless an
explanation places them. rustc suggestions whose spans map exactly are kept as edits.

`explain.rs` then rewrites the common errors (moves, `val` mutation, borrow conflicts, `&`
parameters, closures outliving their scope, missing `throws`, `Display`, bounds and imports,
Kotlin method names) in Oxie terms and computes fixes as text edits on the `.oxi` source,
found from the source text and the mapped spans. The CLI prints the fixes with a preview of
the changed line; the language server attaches them to the diagnostic (`data.fixes`) and
returns them from `textDocument/codeAction`. Errors caused by an Oxie decision say why: a move
into a function that keeps its parameter points at the use `params::kept_reason` finds, and a
borrow that ends too early because of the shared `'a` of automatic lifetimes shows the
signature with a narrower one.

Before any of that, `Mapper::compiler_bug` sets aside errors that only a bug in Oxie can cause:
one inside `__oxie.rs`, or a Rust syntax error in code generated from a file without verbatim
Rust (`rust { }` blocks and macro calls). They are reported as "Oxie generated Rust that doesn't
compile" with rustc's message and the generated line, never explained as the user's error.

The maps are used at run time too. `oxie run` pipes the program's stderr, and `oxie test`
cargo's stdout and stderr, through `locations.rs`, which rewrites every `path.rs:line:col`
whose file has a map (panic messages, backtrace frames, failed assertions) to the `.oxi`
position, line by line as the output arrives.

## Tree-sitter grammar

`editors/tree-sitter-oxie/grammar.js` is a separate, deliberately shallow grammar for editors:
declarations, calls, member access, strings with interpolation; everything else is a token
stream grouped by brackets. It parses all repository files without errors and is used for
highlighting, outline, indentation, text objects and runnables.
`editors/build-kuana-extension.sh` regenerates it and copies it into the Kuana extension.

VS Code highlights with a TextMate grammar instead,
`editors/vscode-oxie/syntaxes/oxie.tmLanguage.json`: regular expressions, edited as JSON. Its
scope names follow VS Code's Rust grammar wherever Oxie and Rust share a construct, so a `.oxi`
file and the Rust generated from it look alike in any theme. `rust { }` blocks embed the Rust
grammar; braces inside them are counted by the Oxie grammar. `npm run test:grammar` checks the
scopes asserted in `editors/vscode-oxie/test/grammar/*.oxi`.

## Tests

* `crates/oxie-compiler` unit tests: lexer and formatter.
* `crates/oxie-cli/tests/e2e.rs`:
  * `tests/e2e/*.oxi` programs run and compared with `.out` files;
  * `tests/coverage/*.oxi`: one program per Rust feature, compared with `.out` files;
  * `tests/diagnostics/*.oxi` must fail with `// error: <line>: <text>` errors at the right lines;
  * every example in `examples/` runs (compared with `expected.txt`) and its tests pass;
  * every runnable code block in `README.md` and `docs/*.md` compiles and runs.
* `crates/oxie-cli/tests/emit.rs`: the Rust generated for `tests/emit/*.oxi` must equal the
  `.rs` (and `.annotated.rs`) files next to them; `OXIE_BLESS=1` rewrites them.
* `crates/oxie-cli/tests/runtime.rs`: panics under `oxie run` and `oxie test` point at `.oxi` lines.
* `crates/oxie-cli/tests/lsp.rs` drives the language server over stdio.
* `editors/vscode-oxie`: `npm test` runs the grammar tests, then integration tests in VS Code
  (`test/suite`) with the language server on a copy of `test/fixtures`.
* `tests/run_e2e.sh` runs the e2e programs from the shell.

`e2e.rs` and `from_rust.rs` run their programs on as many threads as there are cores. Each
thread builds in its own `OXIE_CACHE` under `target/tmp/oxie-cache/<n>`, because builds that
share a target directory wait for each other; `cargo clean` removes these caches. Failures are
reported in the order of the programs' paths, as they were when the programs ran one at a time.
