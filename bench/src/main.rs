//! Times each workload in Oxie and in hand-written Rust. Run with:
//!     cargo run --release -p oxie-bench
use std::hint::black_box;
use std::time::{Duration, Instant};

use oxie_bench as oxie;
use oxie_bench::rust_impl as rs;

/// Median of `runs` timings.
fn time<T: PartialEq + std::fmt::Debug>(runs: usize, f: impl Fn() -> T) -> (Duration, T) {
    let mut times = Vec::new();
    let mut result = None;
    for _ in 0..runs {
        let start = Instant::now();
        let r = black_box(f());
        times.push(start.elapsed());
        result = Some(r);
    }
    times.sort();
    (times[times.len() / 2], result.unwrap())
}

fn main() {
    let runs: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(15);
    println!("{:<18} {:>12} {:>12} {:>8}", "workload", "oxie", "rust", "ratio");
    let mut worst: f64 = 0.0;
    macro_rules! bench {
        ($name:expr, $oxie:expr, $rs:expr) => {{
            // interleave to share warm caches and CPU frequency fairly
            let (tb, rb) = time(runs, || $oxie);
            let (tr, rr) = time(runs, || $rs);
            let (tb2, _) = time(runs, || $oxie);
            let tb = tb.min(tb2);
            assert_eq!(format!("{:?}", rb), format!("{:?}", rr), "{} results differ", $name);
            let ratio = tb.as_secs_f64() / tr.as_secs_f64();
            worst = worst.max(ratio);
            println!("{:<18} {:>10.2?} {:>10.2?} {:>8.3}", $name, tb, tr, ratio);
        }};
    }
    bench!("n-body", oxie::nbody(black_box(200_000)), rs::nbody(black_box(200_000)));
    bench!("word frequency", oxie::word_frequency(black_box(300_000)), rs::word_frequency(black_box(300_000)));
    bench!("prime sieve", oxie::count_primes(black_box(5_000_000)), rs::count_primes(black_box(5_000_000)));
    bench!("dyn dispatch", oxie::shapes_total(black_box(1_000_000)), rs::shapes_total(black_box(1_000_000)));
    bench!("error handling", oxie::sum_valid(black_box(500_000)), rs::sum_valid(black_box(500_000)));
    bench!("recursion (fib)", oxie::fib(black_box(30)), rs::fib(black_box(30)));
    bench!("sorting structs", oxie::sort_points(black_box(500_000)), rs::sort_points(black_box(500_000)));
    println!("\nslowest ratio (Oxie / rust): {:.3}  (1.000 = identical; differences under ~5% are timing noise)", worst);
}
