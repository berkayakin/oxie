import std::collections::HashMap

fun apply(x: i32, f: (i32) -> i32): i32 = f(x)

fun twice(f: (i32) -> i32): (i32) -> i32 = move { x -> f(f(x)) }

fun make_adder(n: i32): (i32) -> i32 = move { it + n }

fun repeat_action(times: i32, action: () -> Unit) {
    for (_ in 0..times) action()
}

fun main() {
    val nums = vec[5, 3, 8, 1, 9, 2]
    val doubled: Vec<i32> = nums.iter().map { it * 2 }.collect()
    println("doubled: ${doubled:?}")
    val evens: Vec<&i32> = nums.iter().filter { *it % 2 == 0 }.collect()
    println("evens: ${evens:?}")
    val sum = nums.iter().fold(0) { acc, x -> acc + x }
    println("sum: $sum")
    val sorted = run {
        var v = nums.clone()
        v.sort()
        v
    }
    println("sorted: ${sorted:?}")
    println("apply: ${apply(4) { it * it }}")
    println("apply arrow: ${apply(4, (x) => x + 100)}")
    val add5 = make_adder(5)
    println("add5: ${add5(10)}")
    val quad = twice { it * 2 }
    println("quad: ${quad(3)}")
    var counter = 0
    repeat_action(3) { counter += 1 }
    println("counter: $counter")
    val square: (i32) -> i32 = { it * it }
    println("square: ${square(7)}")
    val pairs = nums.iter().enumerate().map { (i, x) -> "$i:$x" }.collect::<Vec<_>>().join(", ")
    println("pairs: $pairs")
    var freq = HashMap::new()
    "hello world".chars().filter { it.is_alphabetic() }.for_each { c -> *freq.entry(c).or_insert(0) += 1 }
    val mut_keys = run {
        var keys: Vec<_> = freq.iter().filter { (_, n) -> **n > 1 }.map { (c, _) -> *c }.collect()
        keys.sort()
        keys
    }
    println("repeated: ${mut_keys:?}")
    repeat(2) { println("repeat $it") }
    val max = nums.iter().max_by_key { it.abs() }
    println("max: ${max:?}")
    val names = listOf("ada", "bob", "cy")
    val caps = names.iter().map { s -> s.to_uppercase() }.collect::<Vec<String>>()
    println("caps: ${caps:?}")
    val total_len = names.iter().map(|s| s.len()).sum::<usize>()
    println("total len: $total_len")
    val scores = mapOf("a" to 1, "b" to 2)
    println("score b: ${scores["b"]}")
    val first_big = nums.iter().find { **it > 6 }.copied() ?: 0
    println("first big: $first_big")
}
