// Less common features: unions, raw pointers, macro_rules, blanket impls, Drop, Deref,
// PhantomData, Any downcasts, labeled blocks, std::mem helpers and more.
use std::any::Any;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

macro_rules! square {
    ($x:expr) => {
        $x * $x
    };
}

#[repr(C)]
union Bits {
    f: f32,
    u: u32,
}

const fn cube(x: u64) -> u64 {
    x * x * x
}

static mut COUNTER: u32 = 0;

fn bump() -> u32 {
    unsafe {
        COUNTER += 1;
        COUNTER
    }
}

trait Loud {
    fn loud(&self) -> String;
}

impl<T: std::fmt::Display> Loud for T {
    fn loud(&self) -> String {
        format!("{}!", self).to_uppercase()
    }
}

struct Meters<T> {
    value: f64,
    _unit: PhantomData<T>,
}

struct Metric;

impl<T> Meters<T> {
    fn new(value: f64) -> Self {
        Meters { value, _unit: PhantomData }
    }
}

struct Guard {
    name: &'static str,
}

impl Drop for Guard {
    fn drop(&mut self) {
        println!("drop {}", self.name);
    }
}

struct Stack {
    inner: Vec<i32>,
}

impl Deref for Stack {
    type Target = Vec<i32>;
    fn deref(&self) -> &Vec<i32> {
        &self.inner
    }
}

impl DerefMut for Stack {
    fn deref_mut(&mut self) -> &mut Vec<i32> {
        &mut self.inner
    }
}

#[derive(Debug, Default)]
struct Config {
    retries: u32,
    name: Option<String>,
}

fn describe(v: &dyn Any) -> String {
    if let Some(i) = v.downcast_ref::<i32>() {
        format!("i32 {}", i)
    } else if let Some(s) = v.downcast_ref::<&str>() {
        format!("str {}", s)
    } else {
        "unknown".to_string()
    }
}

fn evens(limit: u32) -> impl Iterator<Item = u32> {
    (0..limit).filter(|n| n % 2 == 0)
}

fn first_negative(rows: &[Vec<i32>]) -> Option<(usize, usize)> {
    let found = 'search: {
        for (r, row) in rows.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                if *v < 0 {
                    break 'search Some((r, c));
                }
            }
        }
        None
    };
    found
}

fn main() {
    println!("{} {}", square!(7), cube(3));
    let b = Bits { f: 1.0 };
    println!("{:#x}", unsafe { b.u });
    bump();
    println!("counter {}", bump());

    let x = 42;
    let p = &x as *const i32;
    println!("{}", unsafe { *p });

    println!("{} {}", 5.loud(), "hey".loud());
    let m: Meters<Metric> = Meters::new(2.5);
    println!("{}", m.value);

    {
        let _a = Guard { name: "a" };
        let _b = Guard { name: "b" };
        println!("scope end");
    }

    let mut s = Stack { inner: vec![3, 1, 2] };
    s.push(0);
    s.sort();
    println!("{:?} len {}", *s, s.len());

    let c = Config { retries: 3, ..Default::default() };
    println!("{:?}", c);

    println!("{} {} {}", describe(&5i32), describe(&"x"), describe(&1.5f64));
    println!("{:?}", evens(9).collect::<Vec<_>>());
    println!("{:?}", first_negative(&[vec![1, 2], vec![3, -4]]));

    let mut a = vec![1, 2];
    let mut bb = vec![9];
    std::mem::swap(&mut a, &mut bb);
    let old = std::mem::replace(&mut a, vec![7, 7]);
    let taken = std::mem::take(&mut bb);
    println!("{:?} {:?} {:?} {:?}", a, bb, old, taken);

    let ch = (b'a' + 2) as char;
    let code = 'Z' as u32;
    println!("{} {} {:?}", ch, code, char::from_u32(0x41));
    let v: Vec<u8> = "hi".bytes().collect();
    println!("{:?}", v);

    let mut it = [10, 20, 30].iter();
    while let Some(n) = it.next() {
        if *n == 20 {
            continue;
        }
        print!("{} ", n);
    }
    println!();

    let value: Result<u32, String> = Err("bad".into());
    let n = if let Ok(v) = value { v } else if let Some(d) = Some(5) { d } else { 0 };
    println!("{}", n);
    let nested = |a: i32| move |b: i32| a * b;
    println!("{}", nested(6)(7));
    let words = vec!["a", "bb", "ccc"];
    let lens: Vec<usize> = words.iter().map(|w| w.len()).rev().collect();
    println!("{:?}", lens);
}
