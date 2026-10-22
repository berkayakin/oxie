// Generic structs, trait bounds, where clauses and impl blocks with bounds.
use std::fmt::{Debug, Display};

#[derive(Debug)]
struct Stack<T> {
    items: Vec<T>,
}

impl<T> Stack<T> {
    fn new() -> Self {
        Stack { items: Vec::new() }
    }

    fn push(&mut self, item: T) {
        self.items.push(item);
    }

    fn pop(&mut self) -> Option<T> {
        self.items.pop()
    }

    fn peek(&self) -> Option<&T> {
        self.items.last()
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<T: Display> Stack<T> {
    fn render(&self) -> String {
        let parts: Vec<String> = self.items.iter().map(|i| i.to_string()).collect();
        format!("[{}]", parts.join(" "))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pair<A, B> {
    first: A,
    second: B,
}

impl<A: Copy, B: Copy> Pair<A, B> {
    fn swap(self) -> Pair<B, A> {
        Pair { first: self.second, second: self.first }
    }
}

fn largest<T: PartialOrd + Copy>(items: &[T]) -> T {
    let mut best = items[0];
    for &i in items {
        if i > best {
            best = i;
        }
    }
    best
}

fn show_all<T>(items: &[T]) -> String
where
    T: Debug,
{
    items.iter().map(|i| format!("{:?}", i)).collect::<Vec<_>>().join(";")
}

fn apply<F: Fn(i32) -> i32>(f: F, times: usize, start: i32) -> i32 {
    let mut v = start;
    for _ in 0..times {
        v = f(v);
    }
    v
}

struct Wrapper<const N: usize> {
    data: [u8; N],
}

impl<const N: usize> Wrapper<N> {
    fn sum(&self) -> u32 {
        self.data.iter().map(|&b| b as u32).sum()
    }
}

fn main() {
    let mut s: Stack<i32> = Stack::new();
    for i in 1..=5 {
        s.push(i * i);
    }
    println!("{} items, top {:?}", s.len(), s.peek());
    println!("{}", s.render());
    while let Some(x) = s.pop() {
        if x < 5 {
            break;
        }
        print!("{} ", x);
    }
    println!();
    println!("empty: {}", s.is_empty());

    let mut words = Stack::new();
    words.push("hello".to_string());
    words.push("world".to_string());
    println!("{} {:?}", words.render(), words);

    let p = Pair { first: 1, second: 'x' };
    let q = p.swap();
    println!("{:?} {:?} {}", p, q, p == Pair { first: 1, second: 'x' });

    println!("{} {} {}", largest(&[3, 9, 2]), largest(&[1.5, 0.2]), largest(&['a', 'z', 'q']));
    println!("{}", show_all(&["a", "b"]));
    println!("{}", apply(|x| x * 2 + 1, 4, 1));
    let w = Wrapper { data: [1, 2, 3, 4] };
    println!("{}", w.sum());
}
