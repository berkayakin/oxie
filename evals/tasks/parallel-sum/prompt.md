Compute the sum of the squares of 1 to 1000 with four threads: thread i (0 to 3) sums the
squares of the numbers from 250·i + 1 to 250·i + 250. Each thread adds its partial sum to one
shared object that holds the total and a log line per thread. Use an `@sync shared class` for
that object.

After joining the threads, print the log lines in sorted order, and the total. The program
must print exactly:

```
1..=250: 5239625
251..=500: 36552125
501..=750: 99114625
751..=1000: 192927125
total: 333833500
```
