Model an order's life cycle with enums. The status is one of Pending, Paid (with the amount),
Shipped (with a tracking code), Delivered, or Cancelled (with a reason). The events are
Pay(amount), Ship(tracking), Deliver and Cancel(reason).

Write a function that takes the current status and an event and returns the next status, or
null when the event isn't allowed in that status. The allowed moves are:

- Pending + Pay → Paid
- Pending + Cancel → Cancelled with the given reason
- Paid + Ship → Shipped
- Paid + Cancel → Cancelled with the reason "refund: " followed by the given reason
- Shipped + Deliver → Delivered

Give the status a `label()` method (see the output for the format; amounts have two decimals)
and an `is_final()` method that is true for Delivered and Cancelled.

Starting from Pending, apply these events in order: Ship("X1"), Pay(25.5), Ship("TRK-42"),
Cancel("changed my mind"), Deliver, Pay(1.0). Print the new status after each event that is
allowed, and the event in its debug form for each one that isn't.

The program must print exactly:

```
start: pending
ignored Ship { tracking: "X1" } while pending
-> paid 25.50
-> shipped (TRK-42)
ignored Cancel { reason: "changed my mind" } while shipped (TRK-42)
-> delivered
ignored Pay { amount: 1.0 } while delivered
final: true
```
