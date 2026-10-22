// Traits, trait objects, default methods and generic functions.
use std::f64::consts::PI;

trait Shape {
    fn area(&self) -> f64;
    fn name(&self) -> String;
    fn describe(&self) -> String {
        format!("{} with area {:.2}", self.name(), self.area())
    }
}

struct Circle {
    radius: f64,
}

struct Rect {
    w: f64,
    h: f64,
}

impl Rect {
    fn square(side: f64) -> Rect {
        Rect { w: side, h: side }
    }

    fn scale(&mut self, k: f64) {
        self.w *= k;
        self.h *= k;
    }
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        PI * self.radius * self.radius
    }
    fn name(&self) -> String {
        "circle".to_string()
    }
}

impl Shape for Rect {
    fn area(&self) -> f64 {
        self.w * self.h
    }
    fn name(&self) -> String {
        if self.w == self.h {
            String::from("square")
        } else {
            String::from("rectangle")
        }
    }
    fn describe(&self) -> String {
        format!("{}x{} {}", self.w, self.h, self.name())
    }
}

fn total_area(shapes: &[Box<dyn Shape>]) -> f64 {
    shapes.iter().map(|s| s.area()).sum()
}

fn largest<'a>(shapes: &'a [Box<dyn Shape>]) -> Option<&'a Box<dyn Shape>> {
    let mut best: Option<&Box<dyn Shape>> = None;
    for s in shapes {
        match best {
            None => best = Some(s),
            Some(b) if s.area() > b.area() => best = Some(s),
            _ => {}
        }
    }
    best
}

fn print_it<T: Shape>(shape: &T) {
    println!("generic: {}", shape.describe());
}

fn main() {
    let mut r = Rect { w: 2.0, h: 3.0 };
    r.scale(2.0);
    print_it(&r);
    print_it(&Circle { radius: 1.5 });

    let shapes: Vec<Box<dyn Shape>> = vec![
        Box::new(Circle { radius: 1.0 }),
        Box::new(r),
        Box::new(Rect::square(2.5)),
    ];
    for s in shapes.iter() {
        println!("{}", s.describe());
    }
    println!("total {:.3}", total_area(&shapes));
    if let Some(big) = largest(&shapes) {
        println!("largest is a {}", big.name());
    }
    let names: Vec<String> = shapes.iter().map(|s| s.name()).collect();
    println!("{}", names.join(", "));
}
