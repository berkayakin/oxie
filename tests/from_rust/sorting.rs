// Ordering: manual Ord impls, sort_by_key, binary search, dedup, retain, BinaryHeap.
use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Task {
    priority: u8,
    name: String,
}

impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.cmp(&other.priority).then_with(|| other.name.cmp(&self.name))
    }
}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn task(priority: u8, name: &str) -> Task {
    Task { priority, name: name.to_string() }
}

fn main() {
    let mut heap = BinaryHeap::new();
    heap.push(task(2, "write"));
    heap.push(task(5, "deploy"));
    heap.push(task(1, "rest"));
    heap.push(task(5, "build"));
    while let Some(t) = heap.pop() {
        print!("{}({}) ", t.name, t.priority);
    }
    println!();

    let mut nums = vec![5, 3, 8, 1, 9, 3, 5, 5, 2];
    nums.sort_unstable();
    nums.dedup();
    println!("{:?}", nums);
    match nums.binary_search(&8) {
        Ok(i) => println!("found 8 at {}", i),
        Err(i) => println!("insert 8 at {}", i),
    }
    println!("{:?}", nums.binary_search(&4));
    nums.retain(|&n| n % 2 == 1);
    println!("{:?}", nums);

    let mut words = vec!["banana", "Apple", "cherry", "apple", "date"];
    words.sort_by_key(|w| w.to_lowercase());
    println!("{:?}", words);
    words.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    println!("{:?}", words);

    let mut min_heap = BinaryHeap::new();
    for n in [7, 2, 9, 4] {
        min_heap.push(Reverse(n));
    }
    let Reverse(smallest) = min_heap.pop().unwrap();
    println!("smallest {}", smallest);

    let seen: HashSet<char> = "mississippi".chars().collect();
    let mut letters: Vec<char> = seen.into_iter().collect();
    letters.sort();
    println!("{:?}", letters);

    let mut queue: VecDeque<i32> = VecDeque::new();
    queue.push_back(1);
    queue.push_back(2);
    queue.push_front(0);
    while let Some(x) = queue.pop_front() {
        if x < 2 {
            queue.push_back(x + 10);
        }
        print!("{} ", x);
    }
    println!();
    let a = task(1, "a");
    let b = task(1, "b");
    println!("{:?} {}", a.cmp(&b), a < b);
    println!("{:?}", [3.5, 1.25, 2.0].iter().cloned().fold(f64::NAN, f64::min));
}
