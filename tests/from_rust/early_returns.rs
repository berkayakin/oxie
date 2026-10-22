// Early returns inside values (`let x = match .. { _ => return .. }`), casts of `if`
// expressions, and a local variable named like a module.
mod shapes {
    pub fn area(w: u32, h: u32) -> u32 {
        w * h
    }
}

fn lookup(key: &str) -> Option<u32> {
    let base = match key {
        "one" => 1,
        "two" => 2,
        _ => return None,
    };
    println!("found {}", key);
    Some(base * 10)
}

fn clamp_len(s: &str) -> usize {
    let n = if s.is_empty() {
        return 0;
    } else {
        s.len()
    };
    let wide = if n > 3 { 3 } else { n } as usize;
    wide + 100
}

fn first_even(v: &[i32]) -> Result<i32, String> {
    let x = match v.iter().find(|x| *x % 2 == 0) {
        Some(x) => *x,
        None => {
            println!("no even number in {:?}", v);
            return Err("none".to_string());
        }
    };
    Ok(x * 2)
}

fn total(items: &[(&str, u32)]) -> u32 {
    let mut sum = 0;
    for (name, n) in items {
        sum += match *n {
            0 => continue,
            n if n > 100 => return 999,
            n => n,
        };
        println!("added {}", name);
    }
    sum
}

fn main() {
    println!("{:?} {:?} {:?}", lookup("one"), lookup("two"), lookup("three"));
    println!("{} {} {}", clamp_len(""), clamp_len("ab"), clamp_len("abcdef"));
    println!("{:?} {:?}", first_even(&[1, 3, 4]), first_even(&[1, 3]));
    println!("{} {}", total(&[("a", 1), ("b", 0), ("c", 5)]), total(&[("x", 1), ("y", 500), ("z", 2)]));
    let shapes = vec![(2, 3), (4, 5)];
    let areas: Vec<u32> = shapes.iter().map(|(w, h)| shapes::area(*w, *h)).collect();
    println!("{:?}", areas);
}
