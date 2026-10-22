//! Workloads implemented twice: in Oxie (src/*.oxi, compiled by build.rs) and in Rust (rust_impl.rs).
include!(concat!(env!("OUT_DIR"), "/oxie/lib.rs"));

pub mod rust_impl;
