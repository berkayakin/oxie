// Enums with data, match on tuples, guards, ranges and bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Light {
    Red,
    Yellow,
    Green,
}

#[derive(Debug, Clone, PartialEq)]
enum Event {
    Timer(u32),
    Emergency { reason: String },
    Reset,
}

impl Light {
    fn next(self) -> Light {
        match self {
            Light::Red => Light::Green,
            Light::Green => Light::Yellow,
            Light::Yellow => Light::Red,
        }
    }

    fn duration(&self) -> u32 {
        match *self {
            Light::Red => 30,
            Light::Yellow => 5,
            Light::Green => 25,
        }
    }
}

fn step(light: Light, event: &Event) -> Light {
    match (light, event) {
        (_, Event::Emergency { .. }) => Light::Red,
        (_, Event::Reset) => Light::Red,
        (l, Event::Timer(t)) if *t >= l.duration() => l.next(),
        (l, Event::Timer(_)) => l,
    }
}

fn classify(n: i64) -> &'static str {
    match n {
        i64::MIN..=-1 => "negative",
        0 => "zero",
        1 | 2 | 3 => "small",
        4..=99 => "medium",
        _ => "large",
    }
}

fn grade(score: u8) -> char {
    match score {
        90..=100 => 'A',
        80..=89 => 'B',
        70..=79 => 'C',
        s if s >= 60 => 'D',
        _ => 'F',
    }
}

fn main() {
    let events = vec![
        Event::Timer(10),
        Event::Timer(30),
        Event::Timer(25),
        Event::Timer(5),
        Event::Emergency { reason: String::from("ambulance") },
        Event::Timer(40),
        Event::Reset,
    ];
    let mut light = Light::Red;
    for e in &events {
        let next = step(light, e);
        if next != light {
            println!("{:?} -> {:?} on {:?}", light, next, e);
        }
        light = next;
    }
    for e in &events {
        if let Event::Emergency { reason } = e {
            println!("emergency because of {}", reason);
        }
    }
    let timers: u32 = events
        .iter()
        .map(|e| match e {
            Event::Timer(t) => *t,
            _ => 0,
        })
        .sum();
    println!("timer total {}", timers);
    for n in [-5, 0, 2, 42, 1000] {
        println!("{} is {}", n, classify(n));
    }
    let grades: String = [95u8, 85, 75, 65, 10].iter().map(|&s| grade(s)).collect();
    println!("{}", grades);
    let point = (3, -2);
    match point {
        (0, 0) => println!("origin"),
        (x, 0) | (0, x) => println!("on an axis at {}", x),
        (x, y) if x > 0 && y < 0 => println!("fourth quadrant {} {}", x, y),
        _ => println!("somewhere"),
    }
    let msg = match light {
        Light::Red => "stop",
        Light::Yellow | Light::Green => "go-ish",
    };
    println!("{}", msg);
    let arr = [1, 2, 3, 4, 5];
    match arr {
        [first, .., last] => println!("{} {}", first, last),
    }
    if let [a, b, rest @ ..] = &arr[..] {
        println!("{} {} {:?}", a, b, rest);
    }
    let n = 7;
    let kind = match n {
        x @ 1..=9 => format!("digit {}", x),
        _ => "other".to_string(),
    };
    println!("{}", kind);
    println!("{}", matches!(light, Light::Red | Light::Yellow));
}
