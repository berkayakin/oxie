// Operator overloading, Copy data types, Index and Display with a loop.
use std::fmt;
use std::ops::{Add, AddAssign, Index, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Vec2 {
    x: f64,
    y: f64,
}

impl Vec2 {
    fn new(x: f64, y: f64) -> Self {
        Vec2 { x, y }
    }

    fn length(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    fn dot(self, other: Vec2) -> f64 {
        self.x * other.x + self.y * other.y
    }

    fn zero() -> Vec2 {
        Vec2::new(0.0, 0.0)
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f64) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }
}

impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2::new(-self.x, -self.y)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Vec2) {
        self.x += o.x;
        self.y += o.y;
    }
}

impl fmt::Display for Vec2 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({:.1}, {:.1})", self.x, self.y)
    }
}

struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<f64>,
}

impl Matrix {
    fn identity(n: usize) -> Matrix {
        let mut data = vec![0.0; n * n];
        for i in 0..n {
            data[i * n + i] = 1.0;
        }
        Matrix { rows: n, cols: n, data }
    }

    fn set(&mut self, r: usize, c: usize, v: f64) {
        let i = r * self.cols + c;
        self.data[i] = v;
    }
}

impl Index<(usize, usize)> for Matrix {
    type Output = f64;
    fn index(&self, (r, c): (usize, usize)) -> &f64 {
        &self.data[r * self.cols + c]
    }
}

impl fmt::Display for Matrix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for r in 0..self.rows {
            let row: Vec<String> = (0..self.cols).map(|c| format!("{}", self[(r, c)])).collect();
            writeln!(f, "[{}]", row.join(" "))?;
        }
        Ok(())
    }
}

fn main() {
    let a = Vec2::new(1.0, 2.0);
    let b = Vec2 { x: 3.0, y: -1.0 };
    println!("{} {} {}", a + b, a - b, a * 2.0);
    println!("{} {:.3} {}", -a, (a + b).length(), a.dot(b));
    let mut acc = Vec2::zero();
    for v in [a, b, a] {
        acc += v;
    }
    println!("{:?} {}", acc, acc == Vec2::new(5.0, 3.0));
    let mut m = Matrix::identity(3);
    m.set(0, 2, 7.5);
    print!("{}", m);
    println!("{}", m[(0, 2)] + m[(1, 1)]);
}
