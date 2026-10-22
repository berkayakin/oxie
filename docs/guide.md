# A tour of Oxie

This tour introduces the Oxie programming language by example. Every program here runs as-is: save it as `tour.oxi` and run
`oxie run tour.oxi`. If you know Kotlin or TypeScript you can read most Oxie code already; the
notes point out where Oxie follows Rust instead.

## Hello, world

```oxie
fun main() {
    val name = "World"
    println("Hello, $name!")
}
```

`fun main` is the entry point. Statements end at the end of the line; `;` is optional.

## Variables and types

```oxie
fun main() {
    val answer = 42              // immutable (Rust `let`)
    var count = 0                // mutable (Rust `let mut`)
    count += 1

    val pi: f64 = 3.14159        // explicit type
    val big: Long = 10_000_000L  // Kotlin names work too: Int, Long, Double, Boolean, Char
    val flag = true
    val letter = 'x'
    val (q, r) = (17 / 5, 17 % 5)  // destructuring

    println("$answer $count $pi $big $flag $letter $q $r")
}
```

The Rust primitive types (`i32`, `u64`, `f64`, `bool`, `char`, `usize`, ...) are the native
names; `Int`, `Long`, `Double`, `Float`, `Boolean`, `Char`, `Unit` are accepted as aliases.
Integer literals default to `i32`, floats to `f64`, as in Rust.

## Functions

```oxie
fun add(a: i32, b: i32): i32 {
    return a + b
}

// expression body; the return type is inferred when it is obvious
fun square(x: i32) = x * x

// the last expression of a block is its value, like Rust
fun max(a: i32, b: i32): i32 {
    if (a > b) a else b
}

// default and named arguments
fun greet(name: &str, greeting: &str = "Hello"): String = "$greeting, $name!"

fun main() {
    println(add(2, 3))
    println(square(7))
    println(max(4, 9))
    println(greet("Ada"))
    println(greet(name = "Bob", greeting = "Hi"))
}
```

Parameters are written `name: Type`, the return type after `:` (Rust's `->` also works).

## Strings

```oxie
fun main() {
    val name = "Oxie"
    val version = 1.25
    println("$name ${version:.1}")       // format spec after `:` (Rust format syntax)
    println("next: ${version + 0.75}")
    println("debug: ${listOf(1, 2):?}")  // `:?` is Debug formatting

    val greeting = "Hello, " + name + "!"  // `+` with strings builds a String
    println(greeting)

    val poem = """
        Roses are red,
          $name is new.
        """                                // indentation is trimmed automatically
    println(poem)
    println("a dollar: \$5")
}
```

String literals are `&str` (borrowed, free). Templates and `+` produce `String`. Where a
`String` is expected, a literal or `&str` is converted for you.

## Control flow

`if` and `when` are expressions.

```oxie
fun describe(n: i32): &str = when {
    n < 0 -> "negative"
    n == 0 -> "zero"
    n < 10 -> "small"
    else -> "large"
}

fun main() {
    val n = 7
    val parity = if (n % 2 == 0) "even" else "odd"
    println("$n is $parity and ${describe(n)}")

    val kind = when (n) {
        0 -> "zero"
        1, 2, 3 -> "tiny"
        in 4..=9 -> "single digit"
        x if x < 0 -> "negative"
        else -> "big"
    }
    println(kind)

    for (i in 0 until 3) print("$i ")          // 0 1 2
    for (i in 10 downTo 0 step 5) print("$i ")  // 10 5 0
    for (i in 1..=3) print("$i ")               // Rust ranges: `..` excludes the end, `..=` includes it
    println()

    var i = 0
    while (i < 3) i += 1
    do {
        i -= 1
    } while (i > 0)

    val found = loop {
        i += 7
        if (i > 20) break i
    }
    println("found $found")
}
```

`when` with a subject becomes a Rust `match`; patterns are Rust patterns (`Some(x)`,
`(a, b)`, `1..=9`, `x if cond`). Without a subject it is an if/else-if chain. Conditions are
written in parentheses as in Kotlin; Rust's style without them works too.

## Collections and lambdas

```oxie
import std::collections::HashMap

fun main() {
    val nums = listOf(5, 3, 8, 1)              // Vec<i32>; `vec[5, 3, 8, 1]` is the same
    val doubled: Vec<i32> = nums.iter().map { it * 2 }.collect()
    val big = nums.iter().filter { **it > 3 }.count()
    val sum = nums.iter().fold(0) { acc, x -> acc + x }
    println("${doubled:?} $big $sum")

    val ages = mapOf("ada" to 36, "bob" to 25)  // HashMap
    println("ada is ${ages["ada"]}")

    var counts = HashMap()
    for (word in "a b a c a".split(' ')) {
        *counts.entry(word).or_insert(0) += 1
    }
    println("a: ${counts["a"]}")

    val add = (a: i32, b: i32) => a + b        // TypeScript-style arrow function
    val twice = { x: i32 -> x * 2 }            // Kotlin-style lambda
    println("${add(2, 3)} ${twice(4)}")
}
```

A lambda without parameters can use `it`. A lambda as the last argument goes outside the
parentheses (`xs.fold(0) { acc, x -> ... }`). All Rust iterator methods are available, with
their Rust names.

## Null safety

`T?` is `Option<T>`; `null` is `None`.

```oxie
data class Address(city: String)
data class Person(name: String, address: Address?)

fun find(people: &[Person], name: &str): &Person? = people.iter().find { it.name == name }

fun main() {
    val people = listOf(Person("Ada", Address("London")), Person("Bob", null))

    val city = find(&people, "Ada")?.address?.city   // safe navigation
    println("${city:?}")

    val len = find(&people, "Zed")?.name?.len() ?: 0  // elvis: default when null
    println(len)

    if (val p = find(&people, "Bob")) {             // unwraps (Rust `if let Some(p)`)
        println("found ${p.name}")
    }
    find(&people, "Ada")?.let { println("hello ${it.name}") }

    var x: i32? = null
    println(x.is_none())
    x = 5                                           // wrapped in Some automatically
    println("${x!! + 1}")                           // `!!` unwraps (panics on null)
}
```

`a ?: b` evaluates `b` only when `a` is null (or an error). The right side can also be
`return`, `throw`, `break` or `continue`.

## Classes

```oxie
class Counter(var count: i32 = 0, step: i32 = 1) {
    var history: Vec<i32> = vec[]

    fun increment() {              // changes fields, so it takes `&mut self` automatically
        count += step
        history.push(count)
    }

    fun current(): i32 = count     // only reads: `&self`

    companion object {
        fun starting_at(n: i32): Counter = Counter(n)   // static
    }
}

data class Point(x: f64, y: f64) {
    fun distance_to(other: Point): f64 = ((x - other.x).powi(2) + (y - other.y).powi(2)).sqrt()
}

fun main() {
    var c = Counter(step = 5)
    c.increment()
    c.increment()
    println("${c.current()} ${c.history:?}")
    println(Counter.starting_at(10).current())

    val p = Point(0.0, 0.0)
    val q = p.copy(x = 3.0, y = 4.0)
    println("${q:?} ${p.distance_to(q)} ${p == Point(0.0, 0.0)}")
}
```

* Constructor parameters are fields. Call the constructor like a function: `Counter()`.
* Inside methods, fields and methods are used without `this.` (write `this.x` when a parameter
  has the same name).
* Oxie decides how each method receives the object: `&self` if it only reads, `&mut self` if it
  assigns a field or calls something that does, and static if it never uses the object and
  returns `Self`. Write `mut fun`, `static fun` or an explicit `self` parameter to choose.
* `data class` derives `Debug`, `Clone`, `PartialEq` — plus `Copy`, `Eq` and `Hash` when all
  fields allow it — and has `copy(field = value)`.
* `init { }` runs after the fields are set; body properties need a default value.

## Enums

```oxie
enum Shape {
    Circle(radius: f64)
    Rect(w: f64, h: f64)
    Empty

    fun area(): f64 = when (this) {
        Circle(r) -> 3.14159 * r * r      // variants can be used without `Shape.`
        Rect(w, h) -> w * h
        Empty -> 0.0
    }
}

enum Direction { North, East, South, West }

fun main() {
    val shapes = listOf(Shape.Circle(1.0), Shape.Rect(2.0, 3.0), Shape.Empty)
    for (s in &shapes) println("${s:?} has area ${s.area():.2}")
    println(Direction.North == Direction.East)
}
```

Variants can carry named fields `Circle(radius: f64)`, positional fields `Pair(i32, i32)`, or
nothing. Enums derive `Debug`, `Clone`, `PartialEq` (and `Copy`/`Eq`/`Hash` when possible).

## Interfaces

```oxie
interface Animal {
    fun name(): String
    fun sound(): String
    fun speak(): String = "${name()} says ${sound()}"    // default method
}

class Dog : Animal {
    override fun name() = "Dog"
    override fun sound() = "woof"
}

class Cat(name: String) : Animal {
    override fun name() = name.clone()
    override fun sound() = "meow"
}

fun introduce(a: &Animal) = println(a.speak())           // `&dyn Animal`

fun main() {
    val zoo: Vec<Box<Animal>> = vec[Box.new(Dog()), Box.new(Cat("Tom".to_string()))]
    for (a in &zoo) introduce(a.as_ref())
}
```

An interface is a Rust trait. Used as a type it becomes a trait object behind a reference or
`Box` (`&Animal` → `&dyn Animal`), or `impl Animal` for a parameter passed by value. `: Display`
or any other trait works the same way; for derivable traits (`: Hash`, `: Ord`, ...) Oxie adds
a derive. Rust's `impl Trait for Type { }` blocks work too.

## Extension functions and operators

```oxie
fun str.shout(): String = self.to_uppercase() + "!"
fun i32.is_even(): bool = this % 2 == 0

data class Vec2(x: f64, y: f64) {
    operator fun plus(o: Vec2): Vec2 = Vec2(x + o.x, y + o.y)
    operator fun times(k: f64): Vec2 = Vec2(x * k, y * k)
    override fun toString() = "($x, $y)"                  // implements Display
}

fun main() {
    println("hey".shout())
    println(4.is_even())
    println(Vec2(1.0, 2.0) + Vec2(3.0, 4.0) * 2.0)
}
```

Operator names follow Kotlin: `plus`, `minus`, `times`, `div`, `rem`, `unaryMinus`, `not`,
`plusAssign`, ... (Rust names like `add` work too).

## Errors

```oxie
error enum ConfigError {
    Missing(key: String) = "missing key: $key"
    Invalid(key: String, value: String) = "invalid value '$value' for $key"
}

fun port(text: &str): u16 throws ConfigError {
    if (text.is_empty()) throw ConfigError.Missing("port".to_string())
    text.parse().map_err { ConfigError.Invalid("port".to_string(), text.to_string()) }?
}

fun main() {
    println(port("8080").unwrap())
    when (val r = port("eighty")) {
        Ok(p) -> println("port $p")
        Err(e) -> println("error: $e")
    }
    val fallback = port("") ?: 80
    println(fallback)
}
```

`throws` turns the return type into a `Result`; returned values are wrapped in `Ok`, `throw`
returns an error, and `?` propagates one. Read [Error handling](errors.md) for `try`/`catch`,
`context`, `AnyError` and `main() throws`.

## Modules and visibility

Every `.oxi` file in `src/` is a module named after the file (`src/net/http.oxi` is
`net::http`). Import with `import`:

```oxie
import std::collections::{HashMap, HashSet}
import std.fs                     // dots work in paths too
import { Deserialize, Serialize } from serde
import model::user::User          // your own module
```

Declarations are visible in the whole project by default. Mark them `private` to keep them in
their file, or `pub` to export them from a library. Members of a `pub` class are `pub` too.

## Ownership

Oxie keeps Rust's ownership model, so `&`, `&mut`, moves and `clone()` work exactly as in Rust:

```oxie
fun total(xs: &[i32]): i32 = xs.iter().sum()
fun add_one(xs: &mut Vec<i32>) = xs.push(1)

fun main() {
    var v = vec[1, 2, 3]
    add_one(&mut v)
    println(total(&v))
    val w = v.clone()     // explicit copy
    val moved = v         // `v` can't be used after this
    println("${w:?} ${moved:?}")
}
```

What Oxie removes is the bookkeeping around it: lifetimes are filled in when Rust can't infer
them (a function returning a reference gets one lifetime tied to its reference parameters),
`&self`/`&mut self` are inferred, and closures that modify captured variables are declared
`mut` for you.

## Tests

```oxie
fun add(a: i32, b: i32) = a + b

test "adds numbers" {
    assert_eq(add(2, 2), 4)
}

test "parsing fails on letters" throws {
    val r = "x".parse::<i32>()
    assert(r.is_err())
}

fun main() = println(add(1, 1))
```

`oxie test` runs them (`oxie test "adds numbers"` runs one).

## Async

```oxie
// dep: tokio = { version = "1", features = ["full"] }
import std::time::Duration

async fun fetch(id: u32): String {
    tokio.time.sleep(Duration.from_millis(10)).await
    "item $id"
}

@tokio::main
async fun main() {
    val a = fetch(1)
    val b = fetch(2)
    val (x, y) = tokio::join!(a, b)
    println("$x, $y")
    println(await fetch(3))                       // TypeScript-style `await` works too
}
```

## Rust inside Oxie

Any Rust syntax Oxie doesn't cover is still available:

```oxie
macro_rules! square {
    ($x:expr) => { $x * $x };
}

rust {
    fn rust_only(x: u32) -> u32 {
        x.rotate_left(3)
    }
}

fun main() {
    println("${square!(5)} ${rust_only(1)}")
    println!("plain Rust macro: {}", 1 + 1)
}
```

Next: the [language reference](reference.md) lists every construct with the Rust it compiles to.
