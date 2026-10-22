fun consume(v: Vec<i32>): usize = v.len()

fun main() {
    val v = vec[1, 2, 3]
    consume(v)
    println(v.len())
    // error: 6: borrow of moved value
    val x = 5
    x = 6
    // error: 9: cannot assign twice
}
