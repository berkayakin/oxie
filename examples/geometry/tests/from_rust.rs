//! The Oxie library used from plain Rust: generated items are ordinary Rust items.
use geometry::{total_area, Circle, Point, Polygon, Shape};

#[test]
fn rust_can_use_oxie_types() {
    let c = Circle::new(Point::new(0.0, 0.0), 1.0);
    let tri = Polygon::new(vec![Point::new(0.0, 0.0), Point::new(4.0, 0.0), Point::new(0.0, 3.0)]);
    assert_eq!(tri.area(), 6.0);
    assert!((total_area(&[&c, &tri]) - (std::f64::consts::PI + 6.0)).abs() < 1e-9);
    let moved = Point::new(1.0, 1.0) + Point::ORIGIN;
    assert_eq!(moved, Point { x: 1.0, y: 1.0 });
}
