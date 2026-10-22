// Closures as values: FnMut parameters, returned closures, boxed closures in structs.
fn make_adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| x + n
}

fn make_counter() -> impl FnMut() -> u32 {
    let mut count = 0;
    move || {
        count += 1;
        count
    }
}

fn compose<A, B, C>(f: impl Fn(A) -> B, g: impl Fn(B) -> C) -> impl Fn(A) -> C {
    move |x| g(f(x))
}

fn repeat_call<F: FnMut()>(times: usize, mut f: F) {
    for _ in 0..times {
        f();
    }
}

struct Button {
    label: String,
    on_click: Box<dyn Fn(&str) -> String>,
}

impl Button {
    fn click(&self) -> String {
        (self.on_click)(&self.label)
    }
}

struct Pipeline {
    steps: Vec<Box<dyn Fn(i64) -> i64>>,
}

impl Pipeline {
    fn new() -> Self {
        Pipeline { steps: Vec::new() }
    }

    fn add(mut self, step: impl Fn(i64) -> i64 + 'static) -> Self {
        self.steps.push(Box::new(step));
        self
    }

    fn run(&self, input: i64) -> i64 {
        self.steps.iter().fold(input, |acc, step| step(acc))
    }
}

fn apply_twice(f: fn(i32) -> i32, x: i32) -> i32 {
    f(f(x))
}

fn square(x: i32) -> i32 {
    x * x
}

fn main() {
    let add5 = make_adder(5);
    println!("{}", add5(10));
    let mut next = make_counter();
    println!("{} {} {}", next(), next(), next());
    let shout = compose(|s: &str| s.to_uppercase(), |s: String| format!("{}!", s));
    println!("{}", shout("hey"));

    let mut hits = Vec::new();
    repeat_call(3, || hits.push(hits.len() * 2));
    println!("{:?}", hits);

    let greeting = String::from("Hello");
    let b = Button { label: "ok".to_string(), on_click: Box::new(move |l| format!("{} from {}", greeting, l)) };
    println!("{}", b.click());

    let p = Pipeline::new().add(|x| x + 1).add(|x| x * 10).add(move |x| x - 3);
    println!("{}", p.run(4));
    println!("{}", apply_twice(square, 3));
    let fs: Vec<fn(i32) -> i32> = vec![square, |x| x + 1];
    println!("{:?}", fs.iter().map(|f| f(6)).collect::<Vec<_>>());

    let mut total = 0;
    (1..=4).for_each(|x| total += x);
    println!("{}", total);
    let threshold = 3;
    let above: Vec<i32> = vec![1, 5, 2, 8].into_iter().filter(|&x| x > threshold).collect();
    println!("{:?}", above);
}
