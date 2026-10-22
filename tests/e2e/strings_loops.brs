const GREETING = "Hello"
const MAX: usize = 3

fun shout(s: &str): String = s.to_uppercase() + "!"

fun main() {
    val name = "World"
    val n = 42
    val pi = 3.14159
    println("$GREETING, $name! n=$n pi=${pi:.2} next=${n + 1}")
    println("escaped: \$name {braces} \"quoted\"")
    val multi = """
        Dear $name,
          indented line
        Bye
        """
    println(multi)
    println(shout("hey"))
    val s = "a" + "b" + n + "c"
    println(s)
    val total = 1 + 2 + " items"
    println(total)
    var i = 0
    while (i < MAX) {
        i += 1
    }
    println("i=$i")
    do {
        i -= 1
        if (i == 1) continue
    } while (i > 0)
    println("after do-while i=$i")
    for (k in 0..=2) print("$k ")
    println()
    for ((idx, ch) in "abc".chars().enumerate()) {
        if (ch == 'b') continue
        print("$idx$ch ")
    }
    println()
    var found = -1
    'outer: for (a in 0..5) {
        for (b in 0..5) {
            if (a * b == 6) {
                found = a * 10 + b
                break 'outer
            }
        }
    }
    println("found $found")
    val grade = { score: i32 ->
        when {
            score >= 90 -> 'A'
            score >= 80 -> 'B'
            else -> 'C'
        }
    }
    println("grades: ${grade(95)} ${grade(85)} ${grade(10)}")
    val c = 'x'
    val kind = when (c) {
        'a', 'e', 'i', 'o', 'u' -> "vowel"
        in 'a'..='z' -> "consonant"
        else -> "other"
    }
    println("$c is a $kind")
    val word = "racecar"
    val is_pal = word.chars().eq(word.chars().rev())
    println("$word palindrome: $is_pal")
    println("contains: ${"ell" in "hello"} ${3 in vec[1, 2, 3]} ${10 !in 0..5}")
    val loop_result = loop {
        i += 7
        if (i > 20) break i * 2
    }
    println("loop result: $loop_result")
}
