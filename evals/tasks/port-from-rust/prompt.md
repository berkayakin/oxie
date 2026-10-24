Here is a Rust program. Write the same program in Oxie, in src/main.oxi, so that it prints the
same output. Write it the Oxie way: no `fn`, `let`, `impl` blocks or `match`, and no macro calls
with `!` (`println!`, `format!`, `write!`, `vec!`, ...).

```rust
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f64),
    Op(char),
}

struct Calculator {
    stack: Vec<f64>,
    history: Vec<String>,
}

impl Calculator {
    fn new() -> Self {
        Calculator { stack: Vec::new(), history: Vec::new() }
    }

    fn eval(&mut self, input: &str) -> Result<f64, String> {
        self.stack.clear();
        for tok in tokenize(input)? {
            match tok {
                Token::Num(n) => self.stack.push(n),
                Token::Op(op) => {
                    let b = self.stack.pop().ok_or("stack underflow")?;
                    let a = self.stack.pop().ok_or("stack underflow")?;
                    let r = match op {
                        '+' => a + b,
                        '-' => a - b,
                        '*' => a * b,
                        '/' => {
                            if b == 0.0 {
                                return Err("division by zero".to_string());
                            }
                            a / b
                        }
                        _ => unreachable!(),
                    };
                    self.stack.push(r);
                }
            }
        }
        let result = self.stack.pop().ok_or("empty expression")?;
        if !self.stack.is_empty() {
            return Err(format!("{} values left over", self.stack.len()));
        }
        self.history.push(format!("{} = {}", input, result));
        Ok(result)
    }
}

impl fmt::Display for Calculator {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} calculations: {}", self.history.len(), self.history.join("; "))
    }
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    input
        .split_whitespace()
        .map(|w| match w {
            "+" | "-" | "*" | "/" => Ok(Token::Op(w.chars().next().unwrap())),
            _ => w.parse::<f64>().map(Token::Num).map_err(|_| format!("bad token '{}'", w)),
        })
        .collect()
}

fn main() {
    let mut calc = Calculator::new();
    for input in ["3 4 +", "5 1 2 + 4 * + 3 -", "2 0 /", "1 +", "2 x *", "1 2", "10 4 /"] {
        match calc.eval(input) {
            Ok(v) => println!("{} => {}", input, v),
            Err(e) => println!("{} => error: {}", input, e),
        }
    }
    println!("{}", calc);
}
```

It prints:

```
3 4 + => 7
5 1 2 + 4 * + 3 - => 14
2 0 / => error: division by zero
1 + => error: stack underflow
2 x * => error: bad token 'x'
1 2 => error: 1 values left over
10 4 / => 2.5
3 calculations: 3 4 + = 7; 5 1 2 + 4 * + 3 - = 14; 10 4 / = 2.5
```
