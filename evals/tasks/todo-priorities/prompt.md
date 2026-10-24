Extend the todo list in src/main.oxi:

1. Add priorities: an enum `Priority` with Low, Medium and High. `add` takes a priority, which
   is Medium when it isn't given.
2. Make `complete(id)` throw an error instead of returning a bool: an error enum `TodoError`
   whose messages are "no task with id N" when the id doesn't exist and "task N is already
   done" when the task is already completed.
3. Add `by_priority()`, which returns the pending tasks from High to Low priority, and by title
   within the same priority.

Then change `main` to add "write report" (High), "buy milk" (Low), "call mom" (no priority
given), "fix bike" (High) and "pay rent" (High); complete tasks 2, 4, 4 and 9, printing the
error for each one that fails; and print the pending tasks by priority.

The program must print exactly:

```
error: task 4 is already done
error: no task with id 9
[High] pay rent
[High] write report
[Medium] call mom
```
