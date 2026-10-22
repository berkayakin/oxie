import std::fmt

// ---- extension functions
fun str.shout(): String = self.to_uppercase() + "!"
fun i32.is_even(): bool = this % 2 == 0
fun <T: Clone> Vec<T>.second(): T? = self.get(1).cloned()
mut fun Vec<i32>.bump_all(by: i32) {
    for (x in self.iter_mut()) *x += by
}

// ---- operators
data class Vec2(x: f64, y: f64) {
    operator fun plus(other: Vec2): Vec2 = Vec2(x + other.x, y + other.y)
    operator fun times(k: f64): Vec2 = Vec2(x * k, y * k)
    operator fun unaryMinus(): Vec2 = Vec2(-x, -y)
    fun length(): f64 = (x * x + y * y).sqrt()

    override fun toString() = "($x, $y)"
}

// ---- enums with data and methods
enum Shape {
    Circle(radius: f64)
    Square(side: f64)
    Triangle(a: f64, b: f64, c: f64)
    Point

    fun area(): f64 = when (this) {
        Circle(r) -> 3.0 * r * r
        Square(s) -> s * s
        Triangle(a, b, c) -> {
            val p = (a + b + c) / 2.0
            (p * (p - a) * (p - b) * (p - c)).sqrt()
        }
        Point -> 0.0
    }

    fun is_round(): bool = this is Circle
}

enum Direction { North, East, South, West

    fun turn_right(): Direction = when (this) {
        North -> East
        East -> South
        South -> West
        West -> North
    }
}

// ---- object (singleton namespace)
object Config {
    const VERSION: &str = "1.2.3"
    val MAX_USERS = 100
    fun banner(): String = "app v$VERSION (max $MAX_USERS)"
}

// ---- companion/static + init
class Temperature(celsius: f64) {
    var label: String = ""

    init {
        label = if (celsius > 25.0) "hot" else "mild"
    }

    companion object {
        fun from_fahrenheit(f: f64): Temperature = Temperature((f - 32.0) * 5.0 / 9.0)
    }

    static fun freezing(): Temperature = Temperature(0.0)

    fun fahrenheit(): f64 = celsius * 9.0 / 5.0 + 32.0
}

// ---- iterator implementation
class Countdown(var n: u32) : Iterator {
    type Item = u32

    override fun next(): u32? {
        if (n == 0) return null
        n -= 1
        return n + 1
    }
}

// ---- generics with bounds
fun <T: PartialOrd + Copy> largest(items: &[T]): T {
    var best = items[0]
    for (x in items) {
        if (*x > best) best = *x
    }
    best
}

fun describe_all<T: fmt::Display>(items: &[T]): String = items.iter().map { "<$it>" }.collect::<Vec<_>>().join("")

interface Animal {
    fun name(): String
    fun sound(): String
    fun speak(): String = "${name()} says ${sound()}"
}

class Dog : Animal {
    override fun name() = "Dog"
    override fun sound() = "woof"
}

class Cat(name: String) : Animal {
    override fun name(): String = name.clone()
    override fun sound() = "meow"
}

fun chorus(animals: &[Box<Animal>]): Vec<String> = animals.iter().map { it.speak() }.collect()

typealias Pair = (i32, i32)

fun swap(p: Pair): Pair = (p.1, p.0)

struct Meters(f64)

fun main() {
    println("hey".shout())
    println("4 even: ${4.is_even()}, 7 even: ${7.is_even()}")
    var nums = vec[10, 20, 30]
    println("second: ${nums.second():?}")
    nums.bump_all(1)
    println("bumped: ${nums:?}")

    val a = Vec2(1.0, 2.0)
    val b = Vec2(3.0, 4.0)
    val c = a + b * 2.0
    println("c = $c, -c = ${-c}, |b| = ${b.length()}")

    val shapes = vec[Shape.Circle(1.0), Shape.Square(2.0), Triangle(3.0, 4.0, 5.0), Point]
    for (s in &shapes) {
        println("${s:?} area=${s.area():.2} round=${s.is_round()}")
    }
    var d = Direction.North
    repeat(5) { d = d.turn_right() }
    println("direction: ${d:?}")
    println(Config.banner())
    println("version: ${Config.VERSION}")

    val t = Temperature.from_fahrenheit(100.0)
    println("temp: ${t.fahrenheit():.1}F ${t.label}")
    println("freezing: ${Temperature.freezing().label}")

    val down: Vec<u32> = Countdown(3).collect()
    println("countdown: ${down:?}")
    println("sum countdown: ${Countdown(4).sum::<u32>()}")

    println("largest: ${largest(&[3, 9, 2])} ${largest(&[1.5, 0.5])}")
    println(describe_all(&["a", "b"]))

    val animals: Vec<Box<Animal>> = vec[Box.new(Dog()), Box.new(Cat("Tom".to_string()))]
    for (line in chorus(&animals)) println(line)

    println("swap: ${swap((1, 2)):?}")
    val m = Meters(5.5)
    println("meters: ${m.0}")
}
