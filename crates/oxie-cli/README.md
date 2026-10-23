# oxie

The command line tool and language server for [Oxie](https://github.com/berkayakin/oxie), a programming language for the
Rust ecosystem: a concise, familiar syntax that compiles to plain Rust. Every crate works, and
programs run exactly as fast as hand-written Rust.

```bash
cargo install oxie
oxie new hello && cd hello
oxie run
```

`oxie` creates, builds, runs, tests and formats Oxie projects, runs single `.oxi` files as
scripts, shows the Rust generated for a file (`oxie emit`), translates Rust to Oxie
(`oxie from-rust`) and serves editors over LSP (`oxie lsp`).

* [A tour of Oxie](https://github.com/berkayakin/oxie/blob/main/docs/guide.md)
* [Tooling](https://github.com/berkayakin/oxie/blob/main/docs/tooling.md)
* [Language reference](https://github.com/berkayakin/oxie/blob/main/docs/reference.md)

Licensed under either of [Apache License, Version 2.0](https://github.com/berkayakin/oxie/blob/main/LICENSE-APACHE) or
[MIT license](https://github.com/berkayakin/oxie/blob/main/LICENSE-MIT) at your option.
