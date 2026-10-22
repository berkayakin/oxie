//! Workloads implemented twice: in brs (src/*.brs, compiled by build.rs) and in Rust (rust_impl.rs).
include!(concat!(env!("OUT_DIR"), "/brs/lib.rs"));

pub mod rust_impl;
