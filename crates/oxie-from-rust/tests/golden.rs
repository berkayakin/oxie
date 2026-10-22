//! The exact Oxie text produced for small inputs, pinning down the output style.

fn check(rust: &str, oxie: &str) {
    let t = oxie_from_rust::translate(rust).unwrap();
    assert_eq!(t.oxie, oxie, "\n--- got\n{}\n--- expected\n{}", t.oxie, oxie);
}

/// The example in docs/tooling.md ("Translating Rust") shows the real output.
#[test]
fn tooling_doc_example_is_current() {
    let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/tooling.md")).unwrap();
    let section = doc.split("### Translating Rust").nth(1).unwrap();
    let block = |lang: &str| -> String {
        let start = section.find(&format!("```{}\n", lang)).unwrap() + lang.len() + 4;
        let end = start + section[start..].find("```").unwrap();
        section[start..end].to_string()
    };
    check(&block("rust"), &block("oxie"));
}

#[test]
fn variables_and_functions() {
    check(
        r#"
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn main() {
    let x = add(1, 2);
    let mut total = 0;
    for i in 0..x {
        total += i;
    }
    println!("total = {}", total);
}
"#,
        r#"fun add(a: i32, b: i32): i32 = a + b

fun main() {
    val x = add(1, 2)
    var total = 0
    for (i in 0..x) {
        total += i
    }
    println("total = $total")
}
"#,
    );
}

#[test]
fn struct_with_new_becomes_data_class() {
    check(
        r#"
#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    fn dist(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    fn shift(&mut self, dx: f64) {
        self.x += dx;
    }
}

fn main() {
    let mut p = Point::new(3.0, 4.0);
    p.shift(1.0);
    println!("{:?} {:.1}", p, p.dist());
}
"#,
        r#"data class Point(x: f64, y: f64) {
    fun dist(): f64 = (x * x + y * y).sqrt()

    fun shift(dx: f64) {
        x += dx
    }
}

fun main() {
    var p = Point(3.0, 4.0)
    p.shift(1.0)
    println("${p:?} ${p.dist():.1}")
}
"#,
    );
}

#[test]
fn other_derives_stay_explicit() {
    check(
        r#"
#[derive(Debug, Clone)]
struct User {
    name: String,
    age: u32,
}

impl User {
    fn greet(&self) -> String {
        format!("hi {}", self.name)
    }
}

fn main() {
    let u = User { name: String::from("ada"), age: 36 };
    println!("{}", u.greet());
}
"#,
        r#"@derive(Debug, Clone)
class User(name: String, age: u32) {
    fun greet(): String = "hi $name"
}

fun main() {
    val u = User { name: "ada", age: 36 }
    println(u.greet())
}
"#,
    );
}

#[test]
fn enums_match_and_display() {
    check(
        r#"
use std::fmt;

enum Shape {
    Circle { radius: f64 },
    Square(f64),
    Empty,
}

impl Shape {
    fn area(&self) -> f64 {
        match self {
            Shape::Circle { radius } => 3.14 * radius * radius,
            Shape::Square(s) => s * s,
            Shape::Empty => 0.0,
        }
    }
}

impl fmt::Display for Shape {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Shape::Circle { radius } => write!(f, "circle({})", radius),
            Shape::Square(s) => write!(f, "square({s})"),
            Shape::Empty => write!(f, "empty"),
        }
    }
}
"#,
        r#"@no_auto_derive
enum Shape {
    Circle(radius: f64)
    Square(f64)
    Empty

    fun area(): f64 = when (self) {
        Shape.Circle(radius) -> 3.14 * radius * radius
        Shape.Square(s) -> s * s
        Shape.Empty -> 0.0
    }

    override fun toString() = when (self) {
        Shape.Circle(radius) -> "circle($radius)"
        Shape.Square(s) -> "square($s)"
        Shape.Empty -> "empty"
    }
}
"#,
    );
}

#[test]
fn results_become_throws() {
    check(
        r#"
use std::error::Error;

fn parse_port(s: &str) -> Result<u16, Box<dyn Error>> {
    let n: u16 = s.trim().parse()?;
    if n == 0 {
        return Err("port 0 is reserved".into());
    }
    Ok(n)
}

fn check(v: i32) -> Result<(), String> {
    if v < 0 {
        Err(format!("{} is negative", v))
    } else {
        Ok(())
    }
}

fn main() {
    match parse_port("80") {
        Ok(p) => println!("port {}", p),
        Err(e) => println!("error: {}", e),
    }
    if let Err(e) = check(-1) {
        println!("{e}");
    }
}
"#,
        r#"import std::error::Error

fun parse_port(s: &str): u16 throws {
    val n: u16 = s.trim().parse()?
    if (n == 0) throw "port 0 is reserved"
    n
}

fun check(v: i32) throws String {
    if (v < 0) throw "$v is negative"
}

fun main() {
    when (parse_port("80")) {
        Ok(p) -> println("port $p")
        Err(e) -> println("error: $e")
    }
    if (val Err(e) = check(-1)) println(e)
}
"#,
    );
}

#[test]
fn options_and_closures() {
    check(
        r#"
fn first_even(v: &[i32]) -> Option<i32> {
    let x = v.iter().find(|n| *n % 2 == 0)?;
    Some(*x * 10)
}

fn main() {
    let v = vec![1, 3, 4, 6];
    let doubled: Vec<i32> = v.iter().map(|x| x * 2).collect();
    let sum = v.iter().fold(0, |acc, x| acc + x);
    let none: Option<i32> = None;
    if let Some(e) = first_even(&v) {
        println!("{} {:?} {} {:?}", e, doubled, sum, none);
    }
    let mut it = v.into_iter();
    while let Some(x) = it.next() {
        print!("{x} ");
    }
}
"#,
        r#"fun first_even(v: &[i32]): i32? {
    val x = v.iter().find { *it % 2 == 0 }?
    Some(*x * 10)
}

fun main() {
    val v = vec[1, 3, 4, 6]
    val doubled: Vec<i32> = v.iter().map { it * 2 }.collect()
    val sum = v.iter().fold(0) { acc, x -> acc + x }
    val none: i32? = null
    if (val e = first_even(&v)) println("$e ${doubled:?} $sum ${none:?}")
    var it = v.into_iter()
    while (val x = it.next()) {
        print("$x ")
    }
}
"#,
    );
}

#[test]
fn traits_become_interfaces() {
    check(
        r#"
trait Animal {
    fn name(&self) -> String;
    fn speak(&self) -> String {
        format!("{} speaks", self.name())
    }
}

struct Dog {
    age: u8,
}

impl Animal for Dog {
    fn name(&self) -> String {
        "dog".to_string()
    }
}

fn main() {
    let pets: Vec<Box<dyn Animal>> = vec![Box::new(Dog { age: 3 })];
    for p in &pets {
        println!("{}", p.speak());
    }
}
"#,
        r#"interface Animal {
    fun name(): String
    fun speak(): String = "${self.name()} speaks"
}

class Dog(age: u8) : Animal {
    override fun name(): String = "dog"
}

fun main() {
    val pets: Vec<Box<dyn Animal>> = vec[Box.new(Dog { age: 3 })]
    for (p in &pets) {
        println(p.speak())
    }
}
"#,
    );
}

#[test]
fn receivers_oxie_cannot_infer_stay_explicit() {
    check(
        r#"
struct Counter {
    n: u32,
}

impl Counter {
    fn helper(x: u32) -> u32 {
        x + 1
    }

    fn into_inner(self) -> u32 {
        self.n
    }

    fn peek(&mut self) -> u32 {
        self.n
    }
}
"#,
        r#"class Counter(n: u32) {
    static fun helper(x: u32): u32 = x + 1

    fun into_inner(self): u32 = n

    fun peek(&mut self): u32 = n
}
"#,
    );
}

#[test]
fn comments_are_kept() {
    check(
        r#"
// Entry point.
fn main() {
    // say hello
    let name = "world"; // the name
    println!("hello {}", name);
}
"#,
        r#"// Entry point.
fun main() {
    // say hello
    val name = "world" // the name
    println("hello $name")
}
"#,
    );
}

#[test]
fn untranslatable_expressions_stay_rust_with_a_todo() {
    check(
        r#"
struct Button {
    on_click: Box<dyn Fn() -> i32>,
}

impl Button {
    fn click(&self) -> i32 {
        let v = (self.on_click)();
        v + 1
    }
}
"#,
        r#"class Button(on_click: Box<dyn Fn() -> i32>) {
    fun click(): i32 {
        // TODO(from-rust): kept as Rust: Oxie reads `(x.f)(args)` as a method call
        val v = rust { (self.on_click)() }
        v + 1
    }
}
"#,
    );
}

#[test]
fn untranslatable_items_stay_rust_with_a_todo() {
    check(
        r#"
extern "C" {
    fn abs(x: i32) -> i32;
}

fn main() {
    println!("{}", unsafe { abs(-3) });
}
"#,
        r#"// TODO(from-rust): kept as Rust: no Oxie syntax for this item
rust {
    extern "C" {
        fn abs(x: i32) -> i32;
    }
}

fun main() {
    println(unsafe { abs(-3) })
}
"#,
    );
}
