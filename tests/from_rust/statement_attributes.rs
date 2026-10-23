// Attributes on statements and `let`s: `#[cfg]` decides what runs (the program is not built
// for tests), lint attributes stay on their statement, and a doc comment on a statement
// becomes a comment. A `#[cfg]` on a block's value or on a `vec!` element is kept as Rust.
fn mode() -> &'static str {
    #[cfg(test)]
    return "test";
    #[cfg(not(test))]
    "normal"
}

fn record(log: &mut Vec<String>, n: i32) {
    #[cfg(test)]
    log.push(format!("test {}", n));
    #[cfg(not(test))]
    log.push(format!("run {}", n));
    if n > 1 {
        #[cfg(test)]
        log.push("big".to_string());
    }
    match n {
        0 => {
            #[cfg(not(test))]
            log.push("zero".to_string());
        }
        _ => {}
    }
}

fn main() {
    let mut log = Vec::new();
    #[cfg(test)]
    log.push("only in tests".to_string());
    #[cfg(test)]
    println!("never printed");
    #[cfg(test)]
    {
        log.push("block".to_string());
    }
    #[cfg(test)]
    for i in 0..3 {
        log.push(format!("loop {}", i));
    }
    for n in 0..3 {
        record(&mut log, n);
    }
    #[allow(unused_variables)]
    let unused = 5;
    #[allow(unused_mut)]
    let mut count = log.len();
    #[cfg(test)]
    let count = 0;
    /// A doc comment documents nothing here.
    let doubled = count * 2;
    #[rustfmt::skip]
    let grid = [1, 2,
                3, 4];
    #[allow(unused_must_use)]
    "7".parse::<i32>();
    let next = |x: i32| {
        #[cfg(test)]
        println!("never {}", x);
        x + 1
    };
    let mut seen = Vec::new();
    let mut see = |x: i32| {
        seen.push(x);
        #[cfg(test)]
        seen.push(x * 10);
    };
    see(1);
    see(2);
    let picked = vec![1, #[cfg(test)] 2, 3];
    println!("{} {:?}", mode(), log);
    println!("{} {} {:?} {} {:?} {:?}", count, doubled, grid, next(1), seen, picked);
}
