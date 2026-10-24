Write a program that uses functions as values:

1. `make_multiplier(k)` returns a function that multiplies its argument by `k`.
2. `apply_twice(x, f)` applies the function `f` to `x` twice.
3. A class `Step` holds a name and a function from `i32` to `i32`. Make a list of four steps,
   "double", "add ten", "square" and "triple" (the last one made with `make_multiplier(3)`),
   and apply them in order to 3, printing each step.
4. A lambda that subtracts 1 and counts how many times it is called, applied to 10 three times
   with `repeat`.
5. The squares of the even numbers from 1 to 10, and their sum.

The program must print exactly:

```
triple 7 = 21
twice +5 on 1 = 11
double: 3 -> 6
add ten: 6 -> 16
square: 16 -> 256
triple: 256 -> 768
n = 7 after 3 calls
even squares: [4, 16, 36, 64, 100], sum 220
```
