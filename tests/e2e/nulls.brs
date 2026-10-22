data class Address(city: String, zip: String?)
data class Person(name: String, age: i32, address: Address?)

fun find(people: &[Person], name: &str): &Person? = people.iter().find { it.name == name }

fun first_even(xs: &[i32]): i32? {
    for (x in xs) {
        if (x % 2 == 0) return *x
    }
    return null
}

fun maybe_name(flag: bool): String? = if (flag) "brs" else null

fun describe(p: &Person?): String = when (p) {
    null -> "nobody"
    else -> "someone"
}

fun main() {
    val people = vec[
        Person("Ada", 36, Address("London", "NW1")),
        Person("Bob", 20, null),
    ]
    val ada = find(&people, "Ada")
    println("city: ${ada?.address?.city:?}")
    val bob = find(&people, "Bob")
    println("bob city: ${bob?.address?.city:?}")
    val age = find(&people, "Zed")?.age ?: -1
    println("zed age: $age")
    val len = maybe_name(true)?.len() ?: 0
    println("len: $len")
    val name = maybe_name(false) ?: "anonymous"
    println("name: $name")
    println("first even: ${first_even(&[1, 3, 4, 5]):?}")
    println("none even: ${first_even(&[1, 3]):?}")
    ada?.let { println("found ${it.name}") }
    find(&people, "Nope")?.let { println("should not print") }
    if (val p = find(&people, "Bob")) {
        println("bob is ${p.age}")
    }
    val x: i32? = 5
    val y: i32? = null
    println("x=${x:?} y=${y:?} sum=${x!! + (y ?: 10)}")
    var count: i32? = null
    println("count: ${count:?}")
    count = 3
    println("count: ${count:?}")
    println(describe(null))
    val zip = ada?.address?.zip.clone() ?: "no zip"
    println("zip: $zip")
    val words = vec["10", "x", "30"]
    val nums: Vec<i32> = words.iter().filter_map { it.parse().ok() }.collect()
    println("nums: ${nums:?}")
    if (y is None) println("y is none")
    if (x is Some(5)) println("x is five")
}
