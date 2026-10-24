Write a generic class `Stack<T>` backed by a list, with `push(item)`, `pop()` (null when the
stack is empty), `peek()` (the top item, or null), `size()` and `is_empty()`. Also write a
generic function `largest(items)` that returns the largest element of a list, or null for an
empty list.

Push 10, 20, 30 and 40 onto a stack of integers and print its size and top; pop twice; print
the size again; pop twice more, and once more on the empty stack. Push "kiwi" and then "fig"
onto a stack of strings and print its top. Call `largest` on [3, 9, 2], on
["pear", "apple", "plum"] and on an empty list.

The program must print exactly:

```
size 4, top Some(40)
popped Some(40) Some(30)
size 2, empty false
pop on empty: None, empty true
top word: fig
largest: Some(9) Some("plum")
largest of nothing: None
```
