// Every shape of Result-returning function: Ok/Err in tails, branches, matches, loops and
// early returns, `?` on calls, `Err(e)?`, Result<(), E>, nested Results and trait methods.
use std::fmt;

#[derive(Debug, PartialEq)]
enum MathError {
    DivByZero,
    Negative(i64),
    Overflow,
}

impl fmt::Display for MathError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MathError::DivByZero => write!(f, "division by zero"),
            MathError::Negative(n) => write!(f, "negative input {}", n),
            MathError::Overflow => write!(f, "overflow"),
        }
    }
}

fn div(a: i64, b: i64) -> Result<i64, MathError> {
    if b == 0 {
        Err(MathError::DivByZero)
    } else {
        Ok(a / b)
    }
}

fn sqrt(n: i64) -> Result<i64, MathError> {
    match n {
        n if n < 0 => Err(MathError::Negative(n)),
        0 | 1 => Ok(n),
        _ => {
            let mut r = 1;
            while (r + 1) * (r + 1) <= n {
                r += 1;
            }
            Ok(r)
        }
    }
}

fn checked_sum(items: &[i64]) -> Result<i64, MathError> {
    let mut total: i64 = 0;
    for &i in items {
        total = match total.checked_add(i) {
            Some(t) => t,
            None => return Err(MathError::Overflow),
        };
    }
    Ok(total)
}

fn pipeline(a: i64, b: i64) -> Result<i64, MathError> {
    let q = div(a, b)?;
    sqrt(q)
}

fn validate(v: i64) -> Result<(), MathError> {
    if v < 0 {
        return Err(MathError::Negative(v));
    }
    if v > 1000 {
        Err(MathError::Overflow)?;
    }
    Ok(())
}

fn validate_all(vs: &[i64]) -> Result<(), MathError> {
    for v in vs {
        validate(*v)?;
    }
    Ok(())
}

fn maybe_div(a: i64, b: Option<i64>) -> Result<Option<i64>, MathError> {
    match b {
        Some(b) => Ok(Some(div(a, b)?)),
        None => Ok(None),
    }
}

fn parse_all(items: &[&str]) -> Result<Vec<i32>, String> {
    items.iter().map(|s| s.parse::<i32>().map_err(|e| format!("{}: {}", s, e))).collect()
}

fn first_word_len(s: &str) -> Result<usize, String> {
    let w = s.split_whitespace().next().ok_or("empty input")?;
    if w.len() > 5 {
        return Err(format!("'{}' is too long", w));
    }
    Ok(w.len())
}

trait Validator {
    fn check(&self, v: i64) -> Result<i64, MathError>;
}

struct Range {
    lo: i64,
    hi: i64,
}

impl Validator for Range {
    fn check(&self, v: i64) -> Result<i64, MathError> {
        if v < self.lo {
            Err(MathError::Negative(v))
        } else if v > self.hi {
            Err(MathError::Overflow)
        } else {
            Ok(v)
        }
    }
}

fn find_index(items: &[i64], target: i64) -> Result<usize, String> {
    for (i, x) in items.iter().enumerate() {
        if *x == target {
            return Ok(i);
        }
    }
    Err(format!("{} not found", target))
}

fn main() {
    println!("{:?} {:?}", div(7, 2), div(1, 0));
    for n in [-4, 0, 1, 15, 16, 99] {
        match sqrt(n) {
            Ok(r) => println!("sqrt({}) = {}", n, r),
            Err(e) => println!("sqrt({}) failed: {}", n, e),
        }
    }
    println!("{:?} {:?}", checked_sum(&[1, 2, 3]), checked_sum(&[i64::MAX, 1]));
    println!("{:?} {:?} {:?}", pipeline(100, 4), pipeline(1, 0), pipeline(-50, 2));
    println!("{:?} {:?} {:?}", validate(5), validate(-5), validate(5000));
    println!("{:?} {:?}", validate_all(&[1, 2, 3]), validate_all(&[1, 2000, -1]));
    println!("{:?} {:?} {:?}", maybe_div(9, Some(3)), maybe_div(9, None), maybe_div(9, Some(0)));
    println!("{:?} {:?}", parse_all(&["1", "2"]), parse_all(&["1", "x"]));
    println!("{:?} {:?} {:?}", first_word_len("hi there"), first_word_len("   "), first_word_len("enormous"));
    let r = Range { lo: 0, hi: 10 };
    println!("{:?} {:?} {:?}", r.check(5), r.check(-1), r.check(11));
    println!("{:?} {:?}", find_index(&[4, 5, 6], 6), find_index(&[4], 9));
    let total: Result<i64, MathError> = [10, 20].iter().map(|v| div(*v, 2)).sum();
    println!("{:?}", total);
}
