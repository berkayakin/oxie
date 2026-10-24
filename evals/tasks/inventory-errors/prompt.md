Write an inventory program. The stock starts as apple 5, pear 2 and plum 0, kept in a
`HashMap<String, u32>`.

Define an error enum `InventoryError` with two cases, an unknown item and not enough stock,
each with its message. Write a function `take(stock, item, qty)` that removes `qty` units of
`item` and returns how many are left, and throws an `InventoryError` when the item is unknown
or there isn't enough of it.

Process these orders one at a time and print a line for each: apple 3, pear 5, kiwi 1,
apple 2, apple 1. Then, inside one try/catch, take 1 pear and then 1 plum: print "both taken"
if both work, or the error that stopped the batch. Finally print how many units are left in
stock altogether.

The program must print exactly:

```
apple x3: ok, 2 left
pear x5: error: not enough pear: wanted 5, have 2
kiwi x1: error: unknown item 'kiwi'
apple x2: ok, 0 left
apple x1: error: not enough apple: wanted 1, have 0
batch stopped: not enough plum: wanted 1, have 0
left: 1
```
