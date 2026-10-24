Model bank customers who can share accounts. Write `Account` as a shared class: an account has
a name and a balance, with `deposit(amount)` and `withdraw(amount)`, where `withdraw` returns
false and changes nothing when the balance is too low. A `Customer` class holds a name and a
list of accounts, and can print a summary of them.

Alice has a "joint" account and a "savings" account; Bob has the same joint account. Then:

1. Alice deposits 100 into the joint account, through her list of accounts.
2. Bob deposits 50 into it, through his.
3. Bob tries to withdraw 200 (refused), then withdraws 20.
4. 30 is deposited into the savings account.

Print what happened and each customer's accounts with their balances. The last line checks
that the joint account in Alice's list and the one in Bob's list are the same object, not just
equal.

The program must print exactly:

```
bob: withdraw 200 refused
bob: withdrew 20
alice: joint=130 savings=30
bob: joint=130
same account: true
```
