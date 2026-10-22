//! Compiler for the Oxie programming language, which compiles to plain Rust.
//!
//! The compiler is a source-to-source translator. Every construct lowers to the Rust you
//! would write by hand, so programs keep Rust's performance and use any crate directly.

pub mod ast;
pub mod compile;
pub mod diag;
pub mod emit;
pub mod format;
pub mod infer;
pub mod lexer;
pub mod lower;
pub mod lower_expr;
pub mod parser;
pub mod runtime;
pub mod sema;
pub mod symbols;
pub mod visit;

pub use compile::{compile, compile_one, discover, Input, Options, Output, SourceMap};
pub use diag::{Diag, LineIndex, Severity};
