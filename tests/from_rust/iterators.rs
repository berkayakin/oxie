// Iterator adaptors and closures.
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct Employee {
    name: String,
    dept: String,
    salary: u32,
    years: u32,
}

fn employee(name: &str, dept: &str, salary: u32, years: u32) -> Employee {
    Employee { name: name.to_string(), dept: dept.to_string(), salary, years }
}

fn main() {
    let staff = vec![
        employee("ada", "eng", 120, 5),
        employee("bob", "eng", 95, 2),
        employee("cy", "ops", 70, 8),
        employee("di", "ops", 82, 3),
        employee("ed", "sales", 60, 1),
    ];

    let total: u32 = staff.iter().map(|e| e.salary).sum();
    println!("total payroll {}", total);

    let seniors: Vec<&str> = staff.iter().filter(|e| e.years >= 3).map(|e| e.name.as_str()).collect();
    println!("seniors: {:?}", seniors);

    let mut by_dept: HashMap<String, Vec<String>> = HashMap::new();
    for e in &staff {
        by_dept.entry(e.dept.clone()).or_insert_with(Vec::new).push(e.name.clone());
    }
    let mut depts: Vec<_> = by_dept.keys().cloned().collect();
    depts.sort();
    for d in &depts {
        println!("{}: {}", d, by_dept[d].join(" "));
    }

    let top = staff.iter().max_by_key(|e| e.salary).unwrap();
    println!("top earner {}", top.name);

    let avg = staff.iter().map(|e| e.salary as f64).sum::<f64>() / staff.len() as f64;
    println!("average {:.1}", avg);

    let (eng, other): (Vec<&Employee>, Vec<&Employee>) = staff.iter().partition(|e| e.dept == "eng");
    println!("{} in eng, {} elsewhere", eng.len(), other.len());

    let numbered: Vec<String> = staff.iter().enumerate().map(|(i, e)| format!("{}. {}", i + 1, e.name)).collect();
    println!("{}", numbered.join(" | "));

    let pairs: Vec<(u32, u32)> = staff.iter().map(|e| e.salary).zip(staff.iter().map(|e| e.years)).take(3).collect();
    println!("{:?}", pairs);

    let parsed: Vec<i32> = "4 8 x 15 16 y 23 42".split(' ').filter_map(|s| s.parse().ok()).collect();
    println!("{:?}", parsed);

    let running: Vec<i32> = parsed
        .iter()
        .scan(0, |acc, &x| {
            *acc += x;
            Some(*acc)
        })
        .collect();
    println!("running sums {:?}", running);

    let product = (1..=6).fold(1u64, |acc, x| acc * x);
    println!("6! = {product}");

    let mut sorted = staff.clone();
    sorted.sort_by(|a, b| b.salary.cmp(&a.salary).then_with(|| a.name.cmp(&b.name)));
    let order: Vec<String> = sorted.iter().map(|e| e.name.to_uppercase()).collect();
    println!("{}", order.join(","));

    let any_rich = staff.iter().any(|e| e.salary > 100);
    let all_paid = staff.iter().all(|e| e.salary > 0);
    let pos = staff.iter().position(|e| e.name == "cy");
    println!("{} {} {:?}", any_rich, all_paid, pos);

    let words = ["apple", "bob", "kayak", "level", "rust"];
    let palindromes: Vec<&&str> = words.iter().filter(|w| w.chars().rev().collect::<String>() == **w).collect();
    println!("{:?}", palindromes);

    let mut counter = 0;
    let mut bump = |by: i32| {
        counter += by;
        counter
    };
    bump(2);
    bump(5);
    println!("counter {}", counter);

    let chunks: Vec<i32> = (1..=10).collect::<Vec<_>>().chunks(3).map(|c| c.iter().sum()).collect();
    println!("{:?}", chunks);
    let windows: Vec<i32> = [1, 4, 9, 16, 25].windows(2).map(|w| w[1] - w[0]).collect();
    println!("{:?}", windows);
}
