// A singly linked list with Box and Option, plus a custom Iterator.
#[derive(Debug)]
struct Node {
    value: i32,
    next: Option<Box<Node>>,
}

#[derive(Debug)]
struct List {
    head: Option<Box<Node>>,
    len: usize,
}

impl List {
    fn new() -> Self {
        List { head: None, len: 0 }
    }

    fn push(&mut self, value: i32) {
        let old = self.head.take();
        self.head = Some(Box::new(Node { value, next: old }));
        self.len += 1;
    }

    fn pop(&mut self) -> Option<i32> {
        self.head.take().map(|node| {
            self.head = node.next;
            self.len -= 1;
            node.value
        })
    }

    fn peek(&self) -> Option<&i32> {
        self.head.as_ref().map(|n| &n.value)
    }

    fn reverse(&mut self) {
        let mut prev: Option<Box<Node>> = None;
        let mut cur = self.head.take();
        while let Some(mut node) = cur {
            cur = node.next.take();
            node.next = prev;
            prev = Some(node);
        }
        self.head = prev;
    }

    fn iter(&self) -> ListIter<'_> {
        ListIter { cur: self.head.as_deref() }
    }
}

struct ListIter<'a> {
    cur: Option<&'a Node>,
}

impl<'a> Iterator for ListIter<'a> {
    type Item = i32;

    fn next(&mut self) -> Option<i32> {
        match self.cur {
            Some(node) => {
                self.cur = node.next.as_deref();
                Some(node.value)
            }
            None => None,
        }
    }
}

struct Countdown(u32);

impl Iterator for Countdown {
    type Item = u32;
    fn next(&mut self) -> Option<Self::Item> {
        if self.0 == 0 {
            None
        } else {
            self.0 -= 1;
            Some(self.0 + 1)
        }
    }
}

fn main() {
    let mut list = List::new();
    for i in 1..=5 {
        list.push(i * 10);
    }
    println!("len {} peek {:?}", list.len, list.peek());
    println!("{:?}", list.iter().collect::<Vec<_>>());
    list.reverse();
    println!("{:?}", list.iter().collect::<Vec<i32>>());
    println!("popped {:?} {:?}", list.pop(), list.pop());
    let sum: i32 = list.iter().sum();
    println!("sum {} len {}", sum, list.len);
    let evens: Vec<i32> = list.iter().filter(|v| v % 20 == 0).collect();
    println!("{:?}", evens);
    let c: Vec<u32> = Countdown(4).collect();
    println!("{:?}", c);
    for (i, v) in Countdown(3).enumerate() {
        print!("{}:{} ", i, v);
    }
    println!();
}
