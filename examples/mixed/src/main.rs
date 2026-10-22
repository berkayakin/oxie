//! A Rust program where some modules are written in Oxie.
//! `mod.rs` declares every src/*.oxi file as a module of this crate.
include!(concat!(env!("OUT_DIR"), "/oxie/mod.rs"));

mod legacy;

fn main() {
    // call Oxie code from Rust
    let report = pricing::Invoice::new("ACME".to_string(), vec![
        pricing::Line::new("widget".to_string(), 3, 250),
        pricing::Line::new("gadget".to_string(), 1, 1200),
    ]);
    println!("{}", report);
    println!("total with tax: {}", pricing::total_with_tax(&report, legacy::TAX_PERCENT));
    match pricing::parse_line("bolt x 10 @ 15") {
        Ok(line) => println!("parsed: {:?}", line),
        Err(e) => println!("error: {}", e),
    }
    if let Err(e) = pricing::parse_line("nonsense") {
        println!("error: {}", e);
    }
}
