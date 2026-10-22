// Inline modules, visibility, constants, statics and nested use.
mod geometry {
    pub const UNIT: f64 = 1.0;

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }

    impl Point {
        pub fn origin() -> Point {
            Point { x: 0.0, y: 0.0 }
        }

        pub fn distance(&self, other: &Point) -> f64 {
            ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
        }
    }

    pub mod shapes {
        use super::Point;

        pub fn perimeter(points: &[Point]) -> f64 {
            let mut total = 0.0;
            for i in 0..points.len() {
                let next = &points[(i + 1) % points.len()];
                total += points[i].distance(next);
            }
            total
        }
    }

    fn helper() -> &'static str {
        "private helper"
    }

    pub fn describe() -> String {
        format!("geometry ({})", helper())
    }
}

mod counter {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static CALLS: AtomicUsize = AtomicUsize::new(0);

    pub fn tick() -> usize {
        CALLS.fetch_add(1, Ordering::SeqCst) + 1
    }
}

use geometry::shapes::perimeter;
use geometry::Point;

const MAX_ITEMS: usize = 3;
static GREETING: &str = "hi";

fn main() {
    let square = [
        Point { x: 0.0, y: 0.0 },
        Point { x: geometry::UNIT, y: 0.0 },
        Point { x: 1.0, y: 1.0 },
        Point { x: 0.0, y: 1.0 },
    ];
    println!("{}", perimeter(&square));
    println!("{:?}", Point::origin());
    println!("{}", geometry::describe());
    for _ in 0..MAX_ITEMS {
        counter::tick();
    }
    println!("{} {}", GREETING, counter::tick());
}
