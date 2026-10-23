# oxie-compiler

The compiler of [Oxie](https://github.com/berkayakin/oxie), a programming language for the Rust ecosystem: it turns `.oxi`
source into plain Rust source code, with source maps back to the `.oxi` lines. It has no
dependencies.

Most users want the [`oxie`](https://crates.io/crates/oxie) command instead; Oxie projects
compile through [`oxie-build`](https://crates.io/crates/oxie-build). How the compiler works is
described in [compiler internals](https://github.com/berkayakin/oxie/blob/main/docs/internals.md).

Licensed under either of [Apache License, Version 2.0](https://github.com/berkayakin/oxie/blob/main/LICENSE-APACHE) or
[MIT license](https://github.com/berkayakin/oxie/blob/main/LICENSE-MIT) at your option.
