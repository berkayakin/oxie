Write a 2D vector type as a data class `Vec2(x, y)` with `f64` fields, and give it the
operators `+` and `-` between vectors, `*` by a number, and unary `-`. Add a `length()` method
and a `rotated(degrees)` method, and make it print as `(x, y)` with one decimal. Write the
degrees-to-radians conversion as an extension function on `f64` named `deg_to_rad`.

With a = (3, 4) and b = (1, -2), the program must print exactly:

```
a + b = (4.0, 2.0)
a - b = (2.0, 6.0)
a * 2 = (6.0, 8.0)
-b = (-1.0, 2.0)
|a| = 5
a rotated 90 = (-4.0, 3.0)
sum = (4.5, 2.5), a is still (3.0, 4.0)
```

The last line adds up a list of a, b and (0.5, 0.5) in a loop, and shows that `a` can still be
used afterwards.
