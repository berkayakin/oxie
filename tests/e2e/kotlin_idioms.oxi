
class Parser(input: String) {
    enum State { Start, Done }
    data class Span(start: usize, end: usize)

    var state: State = State.Start

    fun spans(): Vec<Span> {
        var out = mutableListOf<Span>()
        var start = 0
        for ((i, c) in input.chars().enumerate()) {
            if (c == ' ') {
                out.push(Span(start, i))
                start = i + 1
            }
        }
        out.push(Span(start, input.len()))
        state = State.Done
        out
    }
}

fun main() {
    var p = Parser("hello big world")
    val spans = p.spans()
    println("${spans.len()} spans, last ${spans.last():?}, state ${p.state:?}")
    val counts = mapOf<String, i32>()
    val n = "42".parse<i32>().unwrap()
    println("${counts.len()} $n")
    val outer = Parser.Span(1, 2)
    println("${outer:?}")
}
