// Plain data types (data classes), collections of structs, labeled loops and shadowing.
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Sku {
    id: u32,
    size: u8,
}

impl Sku {
    fn new(id: u32, size: u8) -> Self {
        Self { id, size }
    }

    fn code(&self) -> String {
        format!("SKU-{:05}-{}", self.id, self.size)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Item {
    sku: Sku,
    name: String,
    price_cents: u64,
    qty: u32,
}

#[derive(Debug)]
struct Inventory {
    items: Vec<Item>,
}

impl Inventory {
    fn add(&mut self, sku: Sku, name: &str, price_cents: u64, qty: u32) {
        if let Some(existing) = self.items.iter_mut().find(|i| i.sku == sku) {
            existing.qty += qty;
            return;
        }
        self.items.push(Item { sku, name: name.to_string(), price_cents, qty });
    }

    fn value(&self) -> u64 {
        self.items.iter().map(|i| i.price_cents * i.qty as u64).sum()
    }

    fn take(&mut self, sku: Sku, qty: u32) -> bool {
        match self.items.iter().position(|i| i.sku == sku) {
            Some(idx) if self.items[idx].qty >= qty => {
                self.items[idx].qty -= qty;
                if self.items[idx].qty == 0 {
                    self.items.remove(idx);
                }
                true
            }
            _ => false,
        }
    }
}

fn dollars(cents: u64) -> String {
    format!("${}.{:02}", cents / 100, cents % 100)
}

fn main() {
    let mut inv = Inventory { items: Vec::new() };
    let shirt = Sku::new(42, 2);
    let mug = Sku::new(7, 0);
    inv.add(shirt, "shirt", 1999, 3);
    inv.add(mug, "mug", 850, 10);
    inv.add(shirt, "shirt", 1999, 2);
    println!("value {}", dollars(inv.value()));
    println!("{} {}", inv.take(mug, 10), inv.take(shirt, 9));
    for item in &inv.items {
        println!("{:<8}{:>4} x {:>8} [{}]", item.name, item.qty, dollars(item.price_cents), item.sku.code());
    }

    let mut by_size: HashMap<Sku, u32> = HashMap::new();
    *by_size.entry(shirt).or_default() += 1;
    *by_size.entry(shirt).or_default() += 1;
    println!("{:?}", by_size.get(&shirt));

    let grid = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
    let mut found = None;
    'outer: for (r, row) in grid.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            if v * v > 30 {
                found = Some((r, c));
                break 'outer;
            }
        }
    }
    println!("{:?}", found);

    let x = 5;
    let x = x * 2;
    let x = format!("{}!", x);
    println!("{}", x);

    let mut n = 27u64;
    let mut steps = 0;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        steps += 1;
    }
    println!("collatz steps {}", steps);
    let first_big = loop {
        steps += 7;
        if steps > 200 {
            break steps;
        }
    };
    println!("{}", first_big);
    let (a, mut b) = (1, 2);
    b += a;
    let t = (a, b, "three");
    println!("{} {} {}", t.0, t.1, t.2);
}
