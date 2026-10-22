// Option chains, `?` on Option, if let / while let, let-else.
#[derive(Debug, Clone)]
struct User {
    name: String,
    email: Option<String>,
    manager: Option<Box<User>>,
}

fn find<'a>(users: &'a [User], name: &str) -> Option<&'a User> {
    users.iter().find(|u| u.name == name)
}

fn manager_email(users: &[User], name: &str) -> Option<String> {
    let user = find(users, name)?;
    let manager = user.manager.as_ref()?;
    manager.email.clone()
}

fn domain(email: &str) -> Option<&str> {
    let at = email.find('@')?;
    Some(&email[at + 1..])
}

fn parse_pair(s: &str) -> Option<(i32, i32)> {
    let (a, b) = s.split_once(',')?;
    let x = a.trim().parse().ok()?;
    let y = b.trim().parse().ok()?;
    Some((x, y))
}

fn describe(n: Option<i32>) -> &'static str {
    let Some(v) = n else {
        return "nothing";
    };
    if v > 0 {
        "positive"
    } else {
        "not positive"
    }
}

fn main() {
    let boss = User { name: "grace".into(), email: Some("grace@navy.mil".to_string()), manager: None };
    let users = vec![
        User { name: "ada".to_string(), email: None, manager: Some(Box::new(boss.clone())) },
        boss.clone(),
    ];
    println!("{:?}", manager_email(&users, "ada"));
    println!("{:?}", manager_email(&users, "grace"));
    println!("{:?}", manager_email(&users, "nobody"));

    let email = find(&users, "grace").and_then(|u| u.email.clone()).unwrap_or_default();
    println!("{} -> {:?}", email, domain(&email));
    println!("{:?}", domain("no-at-sign"));

    let len = find(&users, "ada").map(|u| u.name.len()).unwrap_or(0);
    println!("len {}", len);

    let fallback = find(&users, "zed").map(|u| u.name.clone()).unwrap_or_else(|| "anonymous".to_string());
    println!("{}", fallback);

    for s in ["1, 2", "3,x", "nope", " 10 , -4 "] {
        match parse_pair(s) {
            Some((x, y)) => println!("{} + {} = {}", x, y, x + y),
            None => println!("cannot parse {:?}", s),
        }
    }

    let mut stack = vec![1, 2, 3];
    while let Some(top) = stack.pop() {
        print!("{} ", top);
    }
    println!();

    if let Some(u) = find(&users, "ada") {
        if u.email.is_none() {
            println!("{} has no email", u.name);
        }
    }

    let values = [Some(3), None, Some(-1)];
    for v in values {
        println!("{}", describe(v));
    }
    let total: i32 = values.iter().flatten().sum();
    let firsts: Option<Vec<i32>> = values.iter().cloned().collect();
    println!("{} {:?}", total, firsts);
    let maybe: Option<&str> = None;
    println!("{}", maybe.is_some());
    let x = Some(5).filter(|v| *v > 3).map(|v| v * 10);
    println!("{:?}", x);
}
