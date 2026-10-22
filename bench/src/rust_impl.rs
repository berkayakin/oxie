//! Hand-written idiomatic Rust versions of the same workloads.
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub struct Body {
    x: f64,
    y: f64,
    z: f64,
    vx: f64,
    vy: f64,
    vz: f64,
    mass: f64,
}

const SOLAR_MASS: f64 = 4.0 * std::f64::consts::PI * std::f64::consts::PI;
const DAYS_PER_YEAR: f64 = 365.24;

fn bodies() -> Vec<Body> {
    let b = |x, y, z, vx: f64, vy: f64, vz: f64, m: f64| Body { x, y, z, vx: vx * DAYS_PER_YEAR, vy: vy * DAYS_PER_YEAR, vz: vz * DAYS_PER_YEAR, mass: m * SOLAR_MASS };
    vec![
        Body { x: 0.0, y: 0.0, z: 0.0, vx: 0.0, vy: 0.0, vz: 0.0, mass: SOLAR_MASS },
        b(4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01, 1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05, 9.54791938424326609e-04),
        b(8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01, -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05, 2.85885980666130812e-04),
        b(1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01, 2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05, 4.36624404335156298e-05),
        b(1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01, 2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05, 5.15138902046611451e-05),
    ]
}

fn advance(bodies: &mut [Body], dt: f64) {
    let n = bodies.len();
    for i in 0..n {
        for j in i + 1..n {
            let dx = bodies[i].x - bodies[j].x;
            let dy = bodies[i].y - bodies[j].y;
            let dz = bodies[i].z - bodies[j].z;
            let d2 = dx * dx + dy * dy + dz * dz;
            let mag = dt / (d2 * d2.sqrt());
            let mj = bodies[j].mass * mag;
            let mi = bodies[i].mass * mag;
            bodies[i].vx -= dx * mj;
            bodies[i].vy -= dy * mj;
            bodies[i].vz -= dz * mj;
            bodies[j].vx += dx * mi;
            bodies[j].vy += dy * mi;
            bodies[j].vz += dz * mi;
        }
    }
    for b in bodies.iter_mut() {
        b.x += dt * b.vx;
        b.y += dt * b.vy;
        b.z += dt * b.vz;
    }
}

fn energy(bodies: &[Body]) -> f64 {
    let mut e = 0.0;
    for (i, b) in bodies.iter().enumerate() {
        e += 0.5 * b.mass * (b.vx * b.vx + b.vy * b.vy + b.vz * b.vz);
        for other in &bodies[i + 1..] {
            let dx = b.x - other.x;
            let dy = b.y - other.y;
            let dz = b.z - other.z;
            e -= b.mass * other.mass / (dx * dx + dy * dy + dz * dz).sqrt();
        }
    }
    e
}

pub fn nbody(steps: usize) -> f64 {
    let mut bs = bodies();
    for _ in 0..steps {
        advance(&mut bs, 0.01);
    }
    energy(&bs)
}

pub fn word_frequency(words: usize) -> usize {
    let vocab = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa", "lambda", "mu"];
    let mut seed = 42u64;
    let mut text = String::with_capacity(words * 6);
    for _ in 0..words {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        text.push_str(vocab[(seed >> 33) as usize % vocab.len()]);
        text.push(' ');
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for w in text.split_whitespace() {
        *counts.entry(w).or_insert(0) += 1;
    }
    counts.values().max().copied().unwrap_or(0)
}

pub fn count_primes(limit: usize) -> usize {
    let mut sieve = vec![true; limit + 1];
    sieve[0] = false;
    sieve[1] = false;
    let mut i = 2;
    while i * i <= limit {
        if sieve[i] {
            let mut j = i * i;
            while j <= limit {
                sieve[j] = false;
                j += i;
            }
        }
        i += 1;
    }
    sieve.iter().filter(|b| **b).count()
}

pub trait Shape {
    fn area(&self) -> f64;
}

struct Circle {
    r: f64,
}

struct Rect {
    w: f64,
    h: f64,
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        3.141592653589793 * self.r * self.r
    }
}

impl Shape for Rect {
    fn area(&self) -> f64 {
        self.w * self.h
    }
}

pub fn shapes_total(count: usize) -> f64 {
    let mut shapes: Vec<Box<dyn Shape>> = Vec::with_capacity(count);
    for i in 0..count {
        let f = (i % 100) as f64;
        if i % 2 == 0 {
            shapes.push(Box::new(Circle { r: f }));
        } else {
            shapes.push(Box::new(Rect { w: f, h: f + 1.0 }));
        }
    }
    shapes.iter().map(|s| s.area()).sum()
}

#[derive(Debug)]
pub struct BadNumber(String);

impl std::fmt::Display for BadNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bad number: {}", self.0)
    }
}

impl std::error::Error for BadNumber {}

fn parse_value(text: &str) -> Result<i64, BadNumber> {
    let v: i64 = text.parse().map_err(|_| BadNumber(text.to_string()))?;
    if v < 0 {
        return Err(BadNumber(text.to_string()));
    }
    Ok(v)
}

pub fn sum_valid(count: usize) -> i64 {
    let mut inputs = Vec::with_capacity(count);
    for i in 0..count {
        inputs.push(match i % 10 {
            0 => "oops".to_string(),
            1 => "-5".to_string(),
            _ => i.to_string(),
        });
    }
    let mut total = 0i64;
    for s in &inputs {
        total += parse_value(s).unwrap_or(0);
    }
    total
}

pub fn fib(n: u32) -> u64 {
    if n < 2 {
        n as u64
    } else {
        fib(n - 1) + fib(n - 2)
    }
}

#[derive(Clone, Copy)]
struct Point {
    x: i64,
    y: i64,
}

pub fn sort_points(count: usize) -> i64 {
    let mut seed = 7u64;
    let mut points = Vec::with_capacity(count);
    for _ in 0..count {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        points.push(Point { x: (seed >> 40) as i64 % 1000, y: (seed >> 20) as i64 % 1000 });
    }
    points.sort_by_key(|p| p.x * p.x + p.y * p.y);
    points[count / 2].x + points[count / 2].y
}
