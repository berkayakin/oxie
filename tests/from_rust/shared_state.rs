// Rc<RefCell<..>> shared ownership, interior mutability and Weak references.
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

#[derive(Debug)]
struct Account {
    name: String,
    balance: RefCell<i64>,
    touches: Cell<u32>,
}

impl Account {
    fn new(name: &str, balance: i64) -> Rc<Account> {
        Rc::new(Account { name: name.to_string(), balance: RefCell::new(balance), touches: Cell::new(0) })
    }

    fn adjust(&self, by: i64) {
        *self.balance.borrow_mut() += by;
        self.touches.set(self.touches.get() + 1);
    }
}

struct TreeNode {
    label: String,
    parent: RefCell<Weak<TreeNode>>,
    children: RefCell<Vec<Rc<TreeNode>>>,
}

fn node(label: &str) -> Rc<TreeNode> {
    Rc::new(TreeNode { label: label.to_string(), parent: RefCell::new(Weak::new()), children: RefCell::new(vec![]) })
}

fn add_child(parent: &Rc<TreeNode>, child: Rc<TreeNode>) {
    *child.parent.borrow_mut() = Rc::downgrade(parent);
    parent.children.borrow_mut().push(child);
}

fn path(n: &Rc<TreeNode>) -> String {
    match n.parent.borrow().upgrade() {
        Some(p) => format!("{}/{}", path(&p), n.label),
        None => n.label.clone(),
    }
}

fn main() {
    let a = Account::new("shared", 100);
    let b = Rc::clone(&a);
    let c = a.clone();
    b.adjust(-30);
    c.adjust(5);
    println!("{} {} touched {} times, rc={}", a.name, a.balance.borrow(), a.touches.get(), Rc::strong_count(&a));

    let log: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let writer = {
        let log = Rc::clone(&log);
        move |msg: &str| log.borrow_mut().push(msg.to_uppercase())
    };
    writer("start");
    writer("stop");
    println!("{:?}", log.borrow());

    let root = node("root");
    let usr = node("usr");
    let bin = node("bin");
    add_child(&root, usr.clone());
    add_child(&usr, bin.clone());
    println!("{}", path(&bin));
    println!("children of root: {}", root.children.borrow().len());

    let counts: RefCell<BTreeMap<char, usize>> = RefCell::new(BTreeMap::new());
    for ch in "hello world".chars().filter(|c| c.is_alphabetic()) {
        *counts.borrow_mut().entry(ch).or_insert(0) += 1;
    }
    let summary: Vec<String> = counts.borrow().iter().map(|(k, v)| format!("{}{}", k, v)).collect();
    println!("{}", summary.join(" "));
    drop(b);
    println!("rc now {}", Rc::strong_count(&a));
}
