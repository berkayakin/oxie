Organize this program as a project with two modules besides `main`:

- `src/units.oxi`: a `Celsius` temperature type (a data class around an `f64`) with
  `to_fahrenheit()`, which prints as, for example, `12.5C` (one decimal).
- `src/stats.oxi`: functions `mean(values)` and `spread(values)` (the largest minus the
  smallest value) over a list of numbers; both return null for an empty list.

In `src/main.oxi`, for this week's temperatures 12.5, 14.0, 9.5, 17.0 and 15.5, print each day
in Celsius and Fahrenheit, then the mean and the spread as temperatures, and then the mean of
an empty list, shown as `n/a`.

The program must print exactly:

```
day 1: 12.5C = 54.5F
day 2: 14.0C = 57.2F
day 3: 9.5C = 49.1F
day 4: 17.0C = 62.6F
day 5: 15.5C = 59.9F
mean: 13.7C
spread: 7.5C
empty mean: n/a
```
