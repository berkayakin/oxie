//! Plain-language explanations and quick fixes for the rustc errors Oxie programmers hit most.
//!
//! rustc reports errors against the generated Rust. After they are mapped back to .oxi lines,
//! this module rewrites the common ones in Oxie terms (`val`/`var`, lambdas, `throws`), relabels
//! the related spans ("moved here", "borrowed here") and computes fixes as edits to the .oxi
//! source. Fixes come from two places: rustc's own suggestions whose spans map exactly onto the
//! source, and Oxie-specific edits found by looking at the source text.

use crate::cargo::{Edit, Fix, Label, MappedDiag};

pub fn explain(d: &mut MappedDiag, src: &str) {
    let lines: Vec<&str> = src.lines().collect();
    let names = ticks(&d.message);
    let first = names.first().cloned().unwrap_or_default();
    let code = d.code.clone().unwrap_or_default();
    match code.as_str() {
        "E0382" => moved_value(d, &lines, &first),
        "E0384" => assign_twice(d, &lines, &first),
        "E0596" => borrow_mut(d, &lines, &first),
        "E0499" | "E0502" | "E0505" | "E0506" => borrow_conflict(d, &lines, &first, &code),
        "E0507" => {
            rewrite(d, format!("can't take {} out of a value that is only borrowed", describe(&first)));
            d.notes.retain(|n| !n.contains("`mut "));
            d.notes.push(help("the value belongs to something you only borrowed, so it can't be moved out. Take a copy with `.clone()`, or borrow it with `&`."));
        }
        "E0373" => lambda_outlives(d, &lines, &names),
        "E0597" => {
            rewrite(d, format!("`{}` is dropped while something still refers to it", first));
            d.notes.push(help(&format!("a reference to `{}` is used after `{}` goes out of scope (the end of its block). Declare `{}` in an outer block, or store a copy instead of a reference.", first, first, first)));
        }
        "E0716" => {
            rewrite(d, "a temporary value is dropped while something still refers to it".to_string());
            d.notes.push(help("store the value in a `val` first, then borrow from that variable."));
        }
        "E0106" => {
            d.notes.push(help("Oxie fills in lifetimes for functions and for classes that hold references, but not here; write them yourself (`fun f<'a>(x: &'a str): &'a str`), or hold owned values (`String` instead of `&str`) to avoid lifetimes altogether."));
        }
        "E0599" => no_method(d, &lines, &first),
        "E0433" | "E0412" | "E0425" | "E0422" | "E0423" | "E0531" | "E0532" => {
            if d.message.contains("undeclared") || d.message.starts_with("cannot find") {
                rewrite(d, format!("`{}` isn't defined or imported here", last_segment(&first)));
            }
            imports(d, &lines);
        }
        "E0277" => trait_missing(d, &lines, &names),
        "E0308" => mismatched(d),
        "E0615" => getter_call(d, &lines, &first),
        // reported without a code
        _ if d.message.starts_with("future cannot be sent between threads safely") => thread_safety(d, &lines, ""),
        _ => {}
    }
    match d.code.as_deref() {
        Some("unused_mut") => unused_mut(d, &lines),
        Some("unused_variables") => unused_variable(d, &lines, &first),
        _ => {}
    }
    rustc_fixes(d, &lines);
    d.secondary.retain(|s| s.exact && !s.text.is_empty());
    d.notes.retain(|n| !n.contains("in format strings you may be able to use"));
}

// ---------------------------------------------------------------------------------------------
// individual errors

fn moved_value(d: &mut MappedDiag, lines: &[&str], name: &str) {
    refine(d, lines, name);
    let in_loop = d.label.as_deref().is_some_and(|l| l.contains("previous iteration")) || d.secondary.iter().any(|s| s.text.contains("previous iteration"));
    let mut move_site: Option<(u32, u32)> = None;
    let mut callee: Option<(String, usize)> = None;
    if in_loop && d.label.as_deref().is_some_and(|l| l.contains("previous iteration")) {
        move_site = Some((d.line, d.col));
        callee = call_around(lines.get(d.line as usize).copied().unwrap_or(""), d.col as usize);
        d.label = Some(format!("`{}` is moved here in every iteration of the loop", name));
    }
    for s in d.secondary.iter_mut() {
        if s.text.contains("moved here") || s.text.contains("value moved") {
            let line = lines.get(s.line as usize).copied().unwrap_or("");
            if let Some(c) = find_word(line, name, s.col as usize) {
                s.col = c as u32;
                s.end_col = (c + name.chars().count()) as u32;
            }
            callee = call_around(line, s.col as usize);
            s.text = match &callee {
                Some((f, _)) => format!("`{}` moved into `{}()` here", name, f),
                None if line.contains(" in ") && line.trim_start().starts_with("for") => format!("the loop takes `{}` here", name),
                None => format!("`{}` moved here", name),
            };
            move_site = Some((s.line, s.col));
            s.exact = true;
        } else if s.text.contains("move occurs because") || s.text.contains("inside of this loop") {
            s.text = String::new();
        }
    }
    d.secondary.retain(|s| !s.text.is_empty());
    if !in_loop {
        d.label = Some("used here after the move".into());
    }
    rewrite(
        d,
        if in_loop { format!("`{}` was already moved in an earlier loop iteration", name) } else { format!("`{}` was moved, so it can't be used anymore", name) },
    );
    d.notes.retain(|n| !n.starts_with("note: move occurs") && !n.contains("consider changing this parameter type") && !n.contains("consider cloning"));
    let callee_text = callee.as_ref().map(|(f, _)| format!("`{}`", f)).unwrap_or_else(|| "the new owner".into());
    let param = callee.as_ref().and_then(|(f, index)| fun_param(lines, f, *index));
    // Oxie already lends `String` and `Vec` parameters that a top-level function only reads; one
    // that stayed owned is kept by the function (stored or returned), so `&` wouldn't compile
    let kept = param.as_ref().is_some_and(|(pl, _, _, ty)| {
        let decl = lines.get(*pl as usize).copied().unwrap_or("");
        ["fun ", "private fun ", "internal fun "].iter().any(|p| decl.starts_with(p)) && (ty == "String" || ty.starts_with("Vec<"))
    });
    if kept {
        d.notes.push(help(&format!(
            "giving a value to a function moves it: {} keeps it, and `{}` can't be used afterwards. Pass a copy with `{}.clone()`.",
            callee_text, name, name
        )));
    } else {
        d.notes.push(help(&format!(
            "giving a value to a function or another variable moves it: {} owns it now, and `{}` can't be used afterwards. Pass a copy with `{}.clone()`, or let {} borrow the value instead.",
            callee_text, name, name, callee_text
        )));
    }
    if let Some((l, c)) = move_site {
        let line = lines.get(l as usize).copied().unwrap_or("");
        if word_at(line, c as usize) == Some(name) {
            let end = c + name.chars().count() as u32;
            d.fixes.push(Fix { title: format!("pass a copy of `{}`", name), edits: vec![edit(l, end, end, ".clone()")] });
        }
    }
    if let (Some((f, _)), Some((pl, pc, pe, ty))) = (callee, param) {
        if !ty.starts_with('&') && !is_copy_type(&ty) && !kept {
            let borrowed = borrowed_type(&ty);
            d.fixes.push(Fix { title: format!("let `{}` borrow it (parameter type `{}`)", f, borrowed), edits: vec![edit(pl, pc, pe, &borrowed)] });
        }
    }
}

fn assign_twice(d: &mut MappedDiag, lines: &[&str], name: &str) {
    refine(d, lines, name);
    if d.message.contains("immutable argument") {
        rewrite(d, format!("the parameter `{}` can't be changed", name));
        d.notes.retain(|n| !n.contains("`mut "));
        d.notes.push(help(&format!("parameters are read-only. Copy it into a variable first: `var {} = {}`.", name, name)));
        return;
    }
    rewrite(d, format!("`{}` is a `val`, so it can't be changed", name));
    d.label = Some("changed here".into());
    make_var(d, lines, name);
}

fn borrow_mut(d: &mut MappedDiag, lines: &[&str], first: &str) {
    let name = first.trim_start_matches('*').split(['.', '[']).next().unwrap_or("").to_string();
    refine(d, lines, &name);
    if d.message.contains("behind a `&` reference") {
        rewrite(d, format!("`{}` is a read-only reference (`&`), so it can't be changed through it", name));
        d.notes.retain(|n| !n.contains("`mut") && !n.contains("mutable reference"));
        if let Some((pl, pc, fname, index)) = param_decl(lines, &name, d.line) {
            let line = lines[pl as usize];
            let rest: String = line.chars().skip(pc as usize).collect();
            if rest.starts_with('&') && !rest.starts_with("&mut ") && !rest.starts_with("&'") {
                let mut edits = vec![edit(pl, pc, pc + 1, "&mut ")];
                // callers in this file that pass `&x` now need `&mut x`
                for (cl, cline) in lines.iter().enumerate() {
                    for (start, args) in calls_of(cline, &fname) {
                        if let Some((ac, atext)) = args.get(index) {
                            if atext.starts_with('&') && !atext.starts_with("&mut ") && cl as u32 != pl {
                                let c = (start + ac) as u32;
                                edits.push(edit(cl as u32, c, c + 1, "&mut "));
                            }
                        }
                    }
                }
                d.notes.push(help(&format!("to change `{}`, `{}` must ask for a mutable reference: `{}: &mut ...`.", name, fname, name)));
                d.fixes.push(Fix { title: format!("let `{}` change `{}` (`&mut`)", fname, name), edits });
            }
        }
        return;
    }
    if d.message.contains("not declared as mutable") {
        rewrite(d, format!("`{}` is a `val`, so it can't be changed", name));
        make_var(d, lines, &name);
    }
}

fn make_var(d: &mut MappedDiag, lines: &[&str], name: &str) {
    d.notes.retain(|n| !n.contains("`mut ") && !n.contains("make this binding mutable"));
    for s in d.secondary.iter_mut() {
        if s.text.contains("first assignment") || s.text.contains("help: consider") {
            s.text = "declared with `val` here".into();
            s.exact = true;
        }
    }
    if let Some((l, c)) = find_decl(lines, name, d.line, "val") {
        if !d.secondary.iter().any(|s| s.line == l) {
            d.secondary.push(Label { line: l, col: c, end_col: c + 3, text: "declared with `val` here".into(), exact: true });
        }
        d.notes.push(help(&format!("declare it with `var` to allow changes: `var {} = ...`.", name)));
        d.fixes.push(Fix { title: format!("declare `{}` with `var`", name), edits: vec![edit(l, c, c + 3, "var")] });
    }
}

fn borrow_conflict(d: &mut MappedDiag, lines: &[&str], name: &str, code: &str) {
    refine(d, lines, name);
    // `*self` in a method is `this`
    let is_this = name == "*self" || name == "self";
    let name = if is_this { "this" } else { name };
    let relabel = |t: &str| -> String {
        let t = t.to_string();
        if t.contains("immutable borrow later used") || t.contains("borrow later used") || t.contains("first borrow later used") {
            "...and is still used here".into()
        } else if t.contains("immutable borrow occurs") {
            format!("`{}` is borrowed here...", name)
        } else if t.contains("first mutable borrow occurs") {
            format!("`{}` is borrowed for changing here...", name)
        } else if t.contains("second mutable borrow occurs") {
            "...and borrowed for changing again here".into()
        } else if t.contains("mutable borrow occurs") {
            format!("`{}` is changed here", name)
        } else if t.contains("is borrowed here") || t.contains("borrow of") {
            format!("`{}` is borrowed here...", name)
        } else if t.contains("is assigned to here") {
            format!("`{}` is changed here", name)
        } else if t.contains("move out of") {
            format!("`{}` is moved here", name)
        } else {
            t
        }
    };
    if let Some(l) = d.label.clone() {
        d.label = Some(relabel(&l));
    }
    for s in d.secondary.iter_mut() {
        let t = relabel(&s.text);
        if t != s.text {
            s.text = t;
            s.exact = true;
            if s.text.contains(&format!("`{}`", name)) {
                refine_label(s, lines, name);
            }
        }
    }
    let headline = match code {
        "E0502" if d.message.contains("as mutable because it is also borrowed as immutable") => format!("`{}` can't be changed while it is borrowed", name),
        "E0502" => format!("`{}` can't be read while it is being changed", name),
        "E0499" => format!("`{}` is already borrowed for changing", name),
        "E0505" => format!("`{}` can't be moved while it is borrowed", name),
        _ => format!("`{}` can't be changed while it is borrowed", name),
    };
    rewrite(d, headline);
    // one span borrowed and still used: one label
    let used: Vec<(u32, u32)> = d.secondary.iter().filter(|s| s.text.contains("borrowed here...")).map(|s| (s.line, s.col)).collect();
    d.secondary.retain(|s| !(s.text == "...and is still used here" && used.contains(&(s.line, s.col))));
    for s in d.secondary.iter_mut().filter(|s| s.text.ends_with("borrowed here...") && used.contains(&(s.line, s.col))) {
        s.text = format!("`{}` is borrowed here, and still used", name);
    }
    // a loop over a field of `this` whose body changes `this`: loop over a copy
    if is_this {
        let found = d.secondary.iter().filter(|s| s.text.contains("borrowed")).find_map(|s| loop_field(lines.get(s.line as usize).copied().unwrap_or("")).map(|f| (s.line, f)));
        if let Some((l, (end, field))) = found {
            d.notes.retain(|n| !n.contains("index-based loop"));
            d.notes.push(help(&format!(
                "the loop goes through `{}`, which belongs to `this`, while its body changes `this` (and so maybe `{}`). Loop over a copy, or collect the changes and make them after the loop.",
                field, field
            )));
            d.fixes.push(Fix { title: format!("loop over a copy of `{}`", field), edits: vec![edit(l, end, end, ".clone()")] });
            return;
        }
    }
    d.notes.push(help(&format!(
        "a reference to `{}` is still in use. Rust doesn't let a value change (or move) while references to it exist, because the change could invalidate them. Finish using the reference first, or copy the data out (`.clone()`) instead of borrowing it.",
        name
    )));
    // `val x = &expr` -> `val x = expr.clone()`
    for s in d.secondary.iter() {
        if !s.text.contains("borrowed here") {
            continue;
        }
        let line = lines.get(s.line as usize).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if !(trimmed.starts_with("val ") || trimmed.starts_with("var ")) {
            continue;
        }
        let Some(eq) = line.find(" = &") else { continue };
        let amp = line[..eq + 3].chars().count() as u32;
        let expr = line[eq + 4..].split("//").next().unwrap_or("").trim_end();
        if expr.is_empty() || expr.starts_with("mut ") {
            continue;
        }
        let end = amp + 1 + expr.chars().count() as u32;
        d.fixes.push(Fix { title: "copy the value instead of borrowing it".into(), edits: vec![edit(s.line, amp, amp + 1, ""), edit(s.line, end, end, ".clone()")] });
        break;
    }
}

/// A value that isn't `Send`/`Sync` used from another thread: usually a `shared class` handle.
fn thread_safety(d: &mut MappedDiag, lines: &[&str], ty: &str) {
    let future = d.message.starts_with("future cannot be");
    // for a future, the type is in a note: "the trait `Send` is not implemented for `Rc<..>`"
    let noted = d.notes.iter().find_map(|n| n.split("is not implemented for `").nth(1).and_then(|t| t.rsplit_once('`')).map(|(t, _)| t.to_string()));
    let ty = if ty.is_empty() { noted.as_deref().unwrap_or("") } else { ty };
    d.notes.retain(|n| !n.contains("full name for the type") && !n.contains("--verbose") && !n.contains("{closure@") && !n.contains("required because it's used within"));
    let class = ty.strip_prefix("Rc<RefCell<").and_then(|t| t.strip_suffix("Data>>"));
    let decl = class.and_then(|c| {
        let pat = format!("shared class {}", c);
        lines.iter().enumerate().find_map(|(l, t)| t.find(&pat).filter(|i| t[i + pat.len()..].chars().next().is_none_or(|c| !is_ident_char(c))).map(|i| (l as u32, t[..i].chars().count() as u32)))
    });
    match (class, decl) {
        (Some(c), Some((l, col))) => {
            if future {
                rewrite(d, format!("this task can't move between threads: it holds a `{}` across an `await`", c));
                d.notes.retain(|n| !n.contains("is not implemented for"));
            } else {
                rewrite(d, format!("`{}` can't be used from another thread", c));
                d.label = Some(format!("a `{}` handle goes to another thread here", c));
            }
            d.notes.push(help(&format!(
                "a `shared class` handle (`Rc<RefCell<..>>`) belongs to one thread. Declare it `@sync shared class {}`, which uses `Arc<Mutex<..>>` so threads can share it.",
                c
            )));
            d.fixes.push(Fix { title: format!("make `{}` usable from several threads (`@sync`)", c), edits: vec![edit(l, col, col, "@sync ")] });
        }
        _ if future => {
            rewrite(d, "this task can't move between threads: it holds a value across an `await` that belongs to one thread".into());
            d.notes.push(help("a value that belongs to one thread (an `Rc`, a `RefCell`, or a `shared class` without `@sync`) is still alive at an `await`, and the task may continue on another thread. End its use before the `await` (put it in a block), or use `Arc`, `Mutex` or a `@sync shared class`."));
        }
        _ if ty.starts_with("Rc<") => {
            d.notes.push(help("`Rc` counts its references without synchronization, so it can't cross threads. Use `Arc` (with a `Mutex` to change the value), or a `@sync shared class`."));
        }
        _ => {}
    }
}

/// `for (x in items)` / `for (x in this.items)`: the column after the iterable and its name.
fn loop_field(line: &str) -> Option<(u32, String)> {
    let start = line.find("for (")?;
    let rest = &line[start..];
    let in_at = start + rest.find(" in ")? + 4;
    let iter = &line[in_at..];
    let name_at = in_at + if iter.starts_with("this.") { 5 } else { 0 };
    let name: String = line[name_at..].chars().take_while(|c| is_ident_char(*c)).collect();
    if name.is_empty() || !line[name_at + name.len()..].starts_with(')') {
        return None;
    }
    Some((line[..name_at + name.len()].chars().count() as u32, name))
}

fn lambda_outlives(d: &mut MappedDiag, lines: &[&str], names: &[String]) {
    let name = names.iter().find(|n| *n != "'static").cloned().unwrap_or_default();
    rewrite(d, format!("the lambda may run after `{}` is gone", name));
    d.label = Some(format!("this lambda borrows `{}`", name));
    for s in d.secondary.iter_mut() {
        if s.text.contains("is borrowed here") {
            s.text = format!("`{}` is used here", name);
            s.exact = true;
            refine_label(s, lines, &name);
        }
    }
    d.notes.retain(|n| !n.contains("`move ") && !n.contains("move` keyword") && !n.contains("outlive `'static`"));
    d.notes.push(help(&format!(
        "the lambda can run later (for example on another thread), after this function has returned and `{}` is gone. A `move` lambda takes `{}` with it.",
        name, name
    )));
    let line = lines.get(d.line as usize).copied().unwrap_or("");
    let rest: String = line.chars().skip(d.col as usize).collect();
    if rest.starts_with('{') || rest.starts_with('|') || rest.starts_with('(') {
        d.fixes.push(Fix { title: "make it a `move` lambda".into(), edits: vec![edit(d.line, d.col, d.col, "move ")] });
    }
}

/// Kotlin/Java method names and their Rust equivalents.
const KOTLIN_METHODS: &[(&str, &str)] = &[
    ("size", "len"),
    ("length", "len"),
    ("count", "len"),
    ("isEmpty", "is_empty"),
    ("isNotEmpty", "!is_empty"),
    ("isBlank", "trim().is_empty"),
    ("add", "push"),
    ("addAll", "extend"),
    ("put", "insert"),
    ("containsKey", "contains_key"),
    ("forEach", "for_each"),
    ("toUpperCase", "to_uppercase"),
    ("toLowerCase", "to_lowercase"),
    ("uppercase", "to_uppercase"),
    ("lowercase", "to_lowercase"),
    ("startsWith", "starts_with"),
    ("endsWith", "ends_with"),
    ("removeAt", "remove"),
    ("indexOf", "find"),
    ("toList", "to_vec"),
    ("toMutableList", "to_vec"),
    ("toString", "to_string"),
    ("equals", "eq"),
    ("getOrDefault", "get(..).unwrap_or"),
    ("keySet", "keys"),
    ("toInt", "parse::<i32>"),
    ("toLong", "parse::<i64>"),
    ("toDouble", "parse::<f64>"),
    ("charAt", "chars().nth"),
    ("substring", "get(a..b)"),
    ("joinToString", "join"),
    ("reversed", "rev"),
    ("first", "first"),
];

fn no_method(d: &mut MappedDiag, lines: &[&str], method: &str) {
    if !d.message.starts_with("no method named") && !d.message.starts_with("no function or associated item named") {
        return;
    }
    refine(d, lines, method);
    let rust = KOTLIN_METHODS.iter().find(|(k, _)| *k == method).map(|(_, r)| r.to_string()).or_else(|| {
        // only when rustc knows a method by the snake_case name
        let snake = snake_case(method);
        (snake != method && d.notes.iter().any(|n| n.contains(&format!("`{}`", snake)))).then_some(snake)
    });
    if let Some(r) = rust.filter(|r| r != method) {
        let plain = r.chars().all(|c| c.is_alphanumeric() || c == '_');
        d.notes.retain(|n| !n.contains("similar name") && !n.contains("you might have meant"));
        d.notes.push(help(&format!("Rust names this `{}`; standard library methods use Rust's snake_case names.", r)));
        let line = lines.get(d.line as usize).copied().unwrap_or("");
        if plain && word_at(line, d.col as usize) == Some(method) {
            let end = d.col + method.chars().count() as u32;
            d.fixes.push(Fix { title: format!("use `{}`", r), edits: vec![edit(d.line, d.col, end, &r)] });
        }
    }
    imports(d, lines);
}

/// `use a::b::C;` suggestions become `import a::b::C` fixes.
fn imports(d: &mut MappedDiag, lines: &[&str]) {
    let mut paths: Vec<String> = Vec::new();
    for s in &d.raw_suggestions {
        for l in s.lines() {
            let l = l.trim();
            if let Some(p) = l.strip_prefix("use ").and_then(|p| p.strip_suffix(';')) {
                let p = p.trim_start_matches("crate::").to_string();
                if !p.contains("__oxie") && !paths.contains(&p) {
                    paths.push(p);
                }
            }
        }
    }
    if paths.is_empty() {
        return;
    }
    d.notes.retain(|n| !n.contains("consider importing") && !n.contains("perhaps you want to import") && !n.contains("similar name"));
    let at = import_line(lines);
    let blank = if at == 0 && lines.first().is_some_and(|l| !l.trim().is_empty() && !l.starts_with("import ")) { "\n" } else { "" };
    for p in paths.iter().take(3) {
        d.fixes.push(Fix { title: format!("import `{}`", p), edits: vec![edit(at, 0, 0, &format!("import {}\n{}", p, blank))] });
    }
}

/// The line where a new import goes: after the last top-level import, or at the top.
fn import_line(lines: &[&str]) -> u32 {
    lines.iter().rposition(|l| l.starts_with("import ")).map(|i| i as u32 + 1).unwrap_or(0)
}

fn trait_missing(d: &mut MappedDiag, lines: &[&str], names: &[String]) {
    let msg = d.message.clone();
    if msg.contains("cannot be sent between threads safely") || msg.contains("cannot be shared between threads safely") {
        thread_safety(d, lines, names.first().map(String::as_str).unwrap_or(""));
        return;
    }
    if msg.contains("the `?` operator can only be used") {
        rewrite(d, "`?` passes errors on, but this function doesn't declare `throws`".into());
        d.notes.retain(|n| !n.contains("FromResidual") && !n.contains("consider adding return type"));
        d.notes.push(help("add `throws` to the function so errors can be passed on: `fun main() throws { ... }`."));
        if let Some((l, c)) = enclosing_fun_brace(lines, d.line) {
            d.fixes.push(Fix { title: "add `throws` to the function".into(), edits: vec![edit(l, c, c, " throws")] });
        }
        return;
    }
    if msg.contains("`?` couldn't convert the error") {
        d.notes.push(help("the error type doesn't convert into the one this function throws. Declare plain `throws` to accept any error, or add a case for it to your `error enum` marked `@from`."));
        return;
    }
    let Some(ty) = names.first() else { return };
    let display = msg.ends_with("doesn't implement `std::fmt::Display`");
    let debug = msg.ends_with("doesn't implement `Debug`") || msg.ends_with("doesn't implement `std::fmt::Debug`");
    if !display && !debug {
        bound_fix(d, lines);
        return;
    }
    if is_type_param(ty) {
        bound_fix(d, lines);
        return;
    }
    let base = ty.split('<').next().unwrap_or(ty).trim_start_matches('&').to_string();
    let is_std_collection = matches!(base.as_str(), "Vec" | "Option" | "HashMap" | "HashSet" | "BTreeMap" | "BTreeSet" | "VecDeque" | "Result") || base.starts_with('(') || base.starts_with('[');
    let user_kind = user_type_kind(lines, &base);
    d.notes.retain(|n| !n.contains("in format strings you may be able to use") && !n.contains("is not implemented for"));
    if display {
        rewrite(d, format!("`{}` has no text form for `$` templates", ty));
        let printable_debug = is_std_collection || matches!(user_kind, Some("data class") | Some("enum"));
        if printable_debug {
            d.notes.push(help(&format!("print its debug form with a `:?` spec: `${{value:?}}`.{}", if user_kind.is_some() { format!(" To choose the text yourself, add `override fun toString() = \"...\"` to `{}`.", base) } else { String::new() })));
            if let Some(f) = debug_spec_fix(d, lines) {
                d.fixes.push(f);
            }
        } else if user_kind.is_some() {
            d.notes.push(help(&format!("add `override fun toString() = \"...\"` to `{}`, or make it a `data class` and print it with `${{value:?}}`.", base)));
        }
    } else if user_kind.is_some() {
        d.notes.push(help(&format!("`data class` and `enum` get a debug form automatically; for a plain `class`, add `@derive(Debug)` to `{}`.", base)));
    }
}

/// Adds `:?` to the template placeholder that rustc pointed at.
fn debug_spec_fix(d: &MappedDiag, lines: &[&str]) -> Option<Fix> {
    let line = lines.get(d.line as usize).copied()?;
    let chars: Vec<char> = line.chars().collect();
    // `{name}` inline arguments in the generated format string
    if let Some(gen) = d.gen_text.as_deref().and_then(|g| g.strip_prefix('{')).and_then(|g| g.strip_suffix('}')) {
        if !gen.is_empty() && gen.chars().all(|c| c.is_alphanumeric() || c == '_') {
            let simple = format!("${}", gen);
            let braced = format!("${{{}}}", gen);
            let s: String = chars.iter().collect();
            if let Some(i) = s.find(&braced) {
                let c = s[..i].chars().count() as u32 + braced.chars().count() as u32 - 1;
                return Some(Fix { title: "print the debug form (`:?`)".into(), edits: vec![edit(d.line, c, c, ":?")] });
            }
            let mut from = 0;
            while let Some(i) = s[from..].find(&simple) {
                let i = from + i;
                let after = s[i + simple.len()..].chars().next();
                if !after.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    let c = s[..i].chars().count() as u32;
                    let end = c + simple.chars().count() as u32;
                    return Some(Fix { title: "print the debug form (`:?`)".into(), edits: vec![edit(d.line, c, end, &format!("${{{}:?}}", gen))] });
                }
                from = i + simple.len();
            }
            return None;
        }
    }
    // an expression inside `${...}`: find the closing brace after the span
    let start = d.col as usize;
    let open = (0..start.min(chars.len())).rev().find(|&i| chars[i] == '{' && i > 0 && chars[i - 1] == '$')?;
    let mut depth = 0;
    for (i, &ch) in chars.iter().enumerate().skip(open) {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let inner: String = chars[open + 1..i].iter().collect();
                    if inner.contains(':') {
                        return None;
                    }
                    return Some(Fix { title: "print the debug form (`:?`)".into(), edits: vec![edit(d.line, i as u32, i as u32, ":?")] });
                }
            }
            _ => {}
        }
    }
    None
}

/// "consider restricting type parameter `T` with trait `X`: `: path::X`" -> add the bound.
fn bound_fix(d: &mut MappedDiag, lines: &[&str]) {
    let Some(n) = d.notes.iter().find(|n| n.contains("consider restricting type parameter")).cloned() else { return };
    let param = ticks(&n).first().cloned().unwrap_or_default();
    let Some(bound) = d.raw_suggestions.iter().find(|s| s.starts_with(": ") || s.starts_with(" + ")).cloned() else { return };
    let Some((l, line)) = enclosing_fun(lines, d.line) else { return };
    let Some(lt) = line.find('<') else { return };
    let Some(gt) = line[lt..].find('>').map(|g| lt + g) else { return };
    let generics = &line[lt + 1..gt];
    let mut offset = lt + 1;
    for part in generics.split(',') {
        let p = part.trim();
        let lead = part.len() - part.trim_start().len();
        if p == param {
            let c = line[..offset + lead + p.len()].chars().count() as u32;
            let bound = bound.trim_start_matches(" + ").trim_start_matches(": ");
            d.notes.retain(|x| !x.contains("consider restricting type parameter"));
            d.notes.push(help(&format!("generic code can only use what the type parameter promises; `{}: {}` promises it.", param, bound)));
            d.fixes.push(Fix { title: format!("require `{}: {}`", param, bound), edits: vec![edit(l, c, c, &format!(": {}", bound))] });
            return;
        }
        offset += part.len() + 1;
    }
}

/// `node.next?.value` on a shared class: fields of shared objects are read through getters.
fn getter_call(d: &mut MappedDiag, lines: &[&str], name: &str) {
    refine(d, lines, name);
    let ty = ticks(&d.message).get(1).cloned().unwrap_or_default();
    let owner = ty.trim_start_matches('&').to_string();
    rewrite(d, format!("`{}` is read with `{}()` here", name, name));
    d.label = Some(format!("`{}` is a shared class; its fields are read through methods", owner));
    d.notes.retain(|n| !n.contains("use parentheses"));
    d.notes.push(help(&format!(
        "Oxie reads `x.{}` as `x.{}()` when it knows `x` is a `{}`; here it couldn't tell (for example after `?.`), so write the call.",
        name, name, owner
    )));
    d.notes.retain(|n| !n.contains("methods are immutable"));
    let line = lines.get(d.line as usize).copied().unwrap_or("");
    if word_at(line, d.col as usize) == Some(name) {
        let end = d.col + name.chars().count() as u32;
        let after: String = line.chars().skip(end as usize).collect();
        let after = after.trim_start();
        let assigned = after.starts_with('=') && !after.starts_with("==") || ["+=", "-=", "*=", "/=", "%="].iter().any(|op| after.starts_with(op));
        if assigned {
            d.notes.push(help(&format!("to change it, call the setter: `x.set_{}(value)`.", name)));
        } else {
            d.fixes.push(Fix { title: format!("call `{}()`", name), edits: vec![edit(d.line, end, end, "()")] });
        }
    }
}

fn mismatched(d: &mut MappedDiag) {
    if let Some(l) = d.label.clone() {
        let l = l.replace("found `&str`", "found `&str` (borrowed text)").replace("found `String`", "found `String` (owned text)");
        d.label = Some(oxie_types(&l));
    }
}

fn unused_mut(d: &mut MappedDiag, lines: &[&str]) {
    let line = lines.get(d.line as usize).copied().unwrap_or("");
    let rest: String = line.chars().skip(d.col as usize).collect();
    let col = if rest.starts_with("var ") { Some(d.col) } else { find_word(line, "var", d.col as usize).map(|c| c as u32) };
    let Some(c) = col else { return };
    let after: String = line.chars().skip(c as usize + 4).collect();
    let name: String = after.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    rewrite(d, format!("`{}` never changes, so it can be a `val`", name));
    d.col = c;
    d.end_col = c + 3;
    d.label = None;
    d.notes.retain(|n| !n.contains("remove this `mut`") && !n.contains("#[warn(unused_mut)]"));
    d.fixes.push(Fix { title: format!("declare `{}` with `val`", name), edits: vec![edit(d.line, c, c + 3, "val")] });
}

fn unused_variable(d: &mut MappedDiag, lines: &[&str], name: &str) {
    refine(d, lines, name);
    let line = lines.get(d.line as usize).copied().unwrap_or("");
    d.notes.retain(|n| !n.contains("#[warn("));
    if word_at(line, d.col as usize) == Some(name) {
        let end = d.col + name.chars().count() as u32;
        d.notes.retain(|n| !n.contains("prefix it with an underscore"));
        d.notes.push(help(&format!("if that's intended, name it `_{}`.", name)));
        d.fixes.push(Fix { title: format!("rename to `_{}`", name), edits: vec![edit(d.line, d.col, end, &format!("_{}", name))] });
    }
}

/// rustc suggestions that map exactly onto the source and read the same in Oxie.
fn rustc_fixes(d: &mut MappedDiag, lines: &[&str]) {
    let suggestions = d.suggestions.clone();
    for s in suggestions {
        let r = s.replacement.as_str();
        let title = match r {
            ".clone()" => "use a copy (`.clone()`)".to_string(),
            "&" => "pass a reference (`&`)".to_string(),
            "&mut " => "pass a mutable reference (`&mut`)".to_string(),
            "move " => "make it a `move` lambda".to_string(),
            "*" => "use the value (`*`)".to_string(),
            "?" => "pass the error on (`?`)".to_string(),
            _ if r.starts_with('.') && r.ends_with(')') && !r.contains("::") && !r.contains('|') => format!("use `{}`", r),
            _ if !r.is_empty() && r.chars().all(|c| c.is_alphanumeric() || c == '_') && s.line == s.end_line && d.fixes.is_empty() => {
                let line = lines.get(s.line as usize).copied().unwrap_or("");
                let old: String = line.chars().skip(s.col as usize).take((s.end_col - s.col) as usize).collect();
                if old.is_empty() || !old.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                format!("use `{}`", r)
            }
            _ => continue,
        };
        let e = Edit { line: s.line, col: s.col, end_line: s.end_line, end_col: s.end_col, text: r.to_string() };
        if d.fixes.iter().any(|f| f.edits.iter().any(|x| x.line == e.line && x.col == e.col)) {
            continue;
        }
        d.fixes.push(Fix { title, edits: vec![e] });
    }
}

// ---------------------------------------------------------------------------------------------
// helpers

fn rewrite(d: &mut MappedDiag, message: String) {
    if d.rust_message.is_none() {
        d.rust_message = Some(d.message.clone());
    }
    d.message = message;
}

fn help(s: &str) -> String {
    format!("help: {}", s)
}

fn edit(line: u32, col: u32, end_col: u32, text: &str) -> Edit {
    Edit { line, col, end_line: line, end_col, text: text.to_string() }
}

fn describe(path: &str) -> String {
    if path.is_empty() {
        "a value".into()
    } else {
        format!("`{}`", path.trim_start_matches('*'))
    }
}

fn last_segment(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

/// Rust spellings of Option in messages, in Oxie terms.
fn oxie_types(s: &str) -> String {
    let mut out = s.to_string();
    while let Some(i) = out.find("Option<") {
        let rest = &out[i + 7..];
        let mut depth = 1;
        let mut end = None;
        for (j, ch) in rest.char_indices() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(j);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(j) = end else { break };
        let inner = rest[..j].to_string();
        out = format!("{}{}?{}", &out[..i], inner, &rest[j + 1..]);
    }
    out
}

/// Contents of `backticked` spans.
fn ticks(s: &str) -> Vec<String> {
    s.split('`').skip(1).step_by(2).map(|x| x.to_string()).collect()
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The identifier starting exactly at `col`, if any.
fn word_at(line: &str, col: usize) -> Option<&str> {
    let start = line.char_indices().nth(col).map(|(i, _)| i)?;
    if start > 0 && line[..start].chars().next_back().is_some_and(is_ident_char) {
        return None;
    }
    let len = line[start..].find(|c: char| !is_ident_char(c)).unwrap_or(line.len() - start);
    (len > 0).then(|| &line[start..start + len])
}

/// Character column of the whole-word occurrence of `word` nearest to `near` (preferring later ones).
fn find_word(line: &str, word: &str, near: usize) -> Option<usize> {
    if word.is_empty() {
        return None;
    }
    let mut best: Option<usize> = None;
    let mut from = 0;
    while let Some(i) = line[from..].find(word) {
        let i = from + i;
        let before = line[..i].chars().next_back();
        let after = line[i + word.len()..].chars().next();
        if !before.is_some_and(is_ident_char) && !after.is_some_and(is_ident_char) {
            let c = line[..i].chars().count();
            let better = match best {
                None => true,
                // the first one at or after `near`, else the last one before it
                Some(b) if c >= near => b < near || c < b,
                Some(b) => b < near && c > b,
            };
            if better {
                best = Some(c);
            }
        }
        from = i + word.len();
    }
    best
}

/// Moves the primary span onto `name` when the mapped position is only nearby.
fn refine(d: &mut MappedDiag, lines: &[&str], name: &str) {
    if name.is_empty() {
        return;
    }
    let line = lines.get(d.line as usize).copied().unwrap_or("");
    if word_at(line, d.col as usize) == Some(name) {
        d.end_col = d.col + name.chars().count() as u32;
        d.end_line = d.line;
        return;
    }
    if let Some(c) = find_word(line, name, d.col as usize) {
        d.col = c as u32;
        d.end_col = (c + name.chars().count()) as u32;
        d.end_line = d.line;
    }
}

fn refine_label(s: &mut Label, lines: &[&str], name: &str) {
    let line = lines.get(s.line as usize).copied().unwrap_or("");
    if let Some(c) = find_word(line, name, s.col as usize) {
        s.col = c as u32;
        s.end_col = (c + name.chars().count()) as u32;
    }
}

/// `val name` / `var name` declaring `name` at or before `line`: (line, column of the keyword).
fn find_decl(lines: &[&str], name: &str, line: u32, keyword: &str) -> Option<(u32, u32)> {
    for l in (0..=(line as usize).min(lines.len().saturating_sub(1))).rev() {
        let text = lines[l];
        let mut from = 0;
        while let Some(i) = text[from..].find(keyword) {
            let i = from + i;
            from = i + keyword.len();
            if text[..i].chars().next_back().is_some_and(is_ident_char) {
                continue;
            }
            let rest = &text[i + keyword.len()..];
            if !rest.starts_with(' ') {
                continue;
            }
            let rest = rest.trim_start();
            if rest.starts_with(name) && !rest[name.len()..].chars().next().is_some_and(is_ident_char) {
                return Some((l as u32, text[..i].chars().count() as u32));
            }
        }
        if l + 1 < line as usize && text.trim_start().starts_with("fun ") {
            break;
        }
    }
    None
}

/// The innermost call whose argument list contains `col`: (function name, argument index).
fn call_around(line: &str, col: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let mut depth = 0i32;
    let mut commas = 0;
    let mut i = col.min(chars.len());
    while i > 0 {
        i -= 1;
        match chars[i] {
            ')' | ']' | '}' => depth += 1,
            '[' | '{' if depth > 0 => depth -= 1,
            '[' | '{' => return None,
            '(' if depth > 0 => depth -= 1,
            '(' => {
                let name: String = chars[..i].iter().rev().take_while(|c| is_ident_char(**c)).collect::<Vec<_>>().into_iter().rev().collect();
                return (!name.is_empty() && !matches!(name.as_str(), "if" | "while" | "for" | "when")).then_some((name, commas));
            }
            ',' if depth == 0 => commas += 1,
            _ => {}
        }
    }
    None
}

/// Calls of `name(` on a line: (column after the parenthesis, [(column offset, argument text)]).
fn calls_of(line: &str, name: &str) -> Vec<(usize, Vec<(usize, String)>)> {
    let mut out = Vec::new();
    let pat = format!("{}(", name);
    let mut from = 0;
    while let Some(i) = line[from..].find(&pat) {
        let i = from + i;
        from = i + pat.len();
        if line[..i].chars().next_back().is_some_and(is_ident_char) || line[..i].trim_end().ends_with("fun") {
            continue;
        }
        let open = line[..i + pat.len()].chars().count();
        let args = split_args(&line.chars().skip(open).collect::<String>());
        out.push((open, args));
    }
    out
}

/// Splits `a, b(c, d), e) rest` at top-level commas up to the closing parenthesis.
fn split_args(s: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' if depth > 0 => depth -= 1,
            ')' => {
                push_arg(&chars, start, i, &mut out);
                return out;
            }
            ',' if depth == 0 => {
                push_arg(&chars, start, i, &mut out);
                start = i + 1;
            }
            _ => {}
        }
    }
    out
}

fn push_arg(chars: &[char], start: usize, end: usize, out: &mut Vec<(usize, String)>) {
    let text: String = chars[start..end].iter().collect();
    let lead = text.len() - text.trim_start().len();
    if !text.trim().is_empty() {
        out.push((start + text[..lead].chars().count(), text.trim().to_string()));
    }
}

/// The `fun` line enclosing `line`.
fn enclosing_fun<'a>(lines: &[&'a str], line: u32) -> Option<(u32, &'a str)> {
    (0..=(line as usize).min(lines.len().saturating_sub(1))).rev().find(|&l| lines[l].trim_start().starts_with("fun ") || lines[l].contains(" fun ")).map(|l| (l as u32, lines[l]))
}

/// Position of the ` {` that opens the enclosing function's body (to insert ` throws`).
fn enclosing_fun_brace(lines: &[&str], line: u32) -> Option<(u32, u32)> {
    let (l, text) = enclosing_fun(lines, line)?;
    if text.contains("throws") {
        return None;
    }
    let t = text.trim_end();
    let body = t.strip_suffix('{')?;
    let col = body.trim_end().chars().count() as u32;
    Some((l, col))
}

/// The parameter `name: Type` of the function enclosing `line`:
/// (line, column of the type, function name, parameter index).
fn param_decl(lines: &[&str], name: &str, line: u32) -> Option<(u32, u32, String, usize)> {
    let (l, text) = enclosing_fun(lines, line)?;
    let fun_at = text.find("fun ")?;
    let after = &text[fun_at + 4..];
    let fname: String = after.chars().take_while(|c| is_ident_char(*c)).collect();
    let open = text[fun_at..].find('(')? + fun_at;
    let params = split_args(&text[open + 1..]);
    let base = text[..open + 1].chars().count();
    for (index, (off, p)) in params.iter().enumerate() {
        let (pname, ty) = p.split_once(':')?;
        if pname.trim().trim_start_matches("var ").trim_start_matches("val ") == name {
            let ty_lead = ty.len() - ty.trim_start().len();
            let col = base + off + pname.chars().count() + 1 + ty[..ty_lead].chars().count();
            return Some((l, col as u32, fname, index));
        }
    }
    None
}

/// Parameter `index` of `fun name(...)` declared in this file: (line, type start, type end, type).
fn fun_param(lines: &[&str], name: &str, index: usize) -> Option<(u32, u32, u32, String)> {
    let pat = format!("fun {}(", name);
    let l = lines.iter().position(|t| t.contains(&pat))?;
    let text = lines[l];
    let open = text.find(&pat)? + pat.len();
    let params = split_args(&text[open..]);
    let base = text[..open].chars().count();
    let (off, p) = params.get(index)?;
    let (pname, ty) = p.split_once(':')?;
    let ty_text = ty.trim();
    let ty_text = ty_text.split(" = ").next().unwrap_or(ty_text).trim();
    let ty_lead = ty.len() - ty.trim_start().len();
    let col = base + off + pname.chars().count() + 1 + ty[..ty_lead].chars().count();
    Some((l as u32, col as u32, (col + ty_text.chars().count()) as u32, ty_text.to_string()))
}

fn is_copy_type(ty: &str) -> bool {
    matches!(
        ty,
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "f32" | "f64" | "bool" | "char" | "Int" | "Long" | "Double" | "Float" | "Boolean" | "Char"
    )
}

/// The idiomatic borrowed form of an owned parameter type.
fn borrowed_type(ty: &str) -> String {
    if ty == "String" {
        return "&str".into();
    }
    if let Some(inner) = ty.strip_prefix("Vec<").and_then(|t| t.strip_suffix('>')) {
        return format!("&[{}]", inner);
    }
    format!("&{}", ty)
}

fn is_type_param(ty: &str) -> bool {
    let t = ty.trim_start_matches('&');
    t.len() <= 2 && t.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// How a type is declared in this file: "data class", "class", "enum".
fn user_type_kind(lines: &[&str], name: &str) -> Option<&'static str> {
    for l in lines {
        let t = l.trim_start();
        for (prefix, kind) in [("data class ", "data class"), ("error class ", "data class"), ("enum class ", "enum"), ("error enum ", "enum"), ("enum ", "enum"), ("class ", "class"), ("struct ", "class")] {
            let t = t.trim_start_matches("pub ").trim_start_matches("private ").trim_start_matches("internal ");
            if let Some(rest) = t.strip_prefix(prefix) {
                if rest.starts_with(name) && !rest[name.len()..].chars().next().is_some_and(is_ident_char) {
                    return Some(kind);
                }
            }
        }
    }
    None
}

fn snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_calls() {
        assert_eq!(find_word("    val n = consume(nums)", "nums", 0), Some(20));
        assert_eq!(word_at("consume(nums)", 8), Some("nums"));
        assert_eq!(call_around("    val n = consume(nums)", 20), Some(("consume".into(), 0)));
        assert_eq!(call_around("f(a, g(b), c)", 11), Some(("f".into(), 2)));
        assert_eq!(fun_param(&["fun consume(v: Vec<i32>): usize = v.len()"], "consume", 0), Some((0, 15, 23, "Vec<i32>".into())));
        assert_eq!(borrowed_type("Vec<i32>"), "&[i32]");
        assert_eq!(find_decl(&["fun main() {", "    val count = 0", "    count += 1"], "count", 2, "val"), Some((1, 4)));
        assert_eq!(param_decl(&["fun add_one(v: &Vec<i32>) {", "    v.push(1)"], "v", 1), Some((0, 15, "add_one".into(), 0)));
        assert_eq!(oxie_types("expected `Option<Vec<i32>>`"), "expected `Vec<i32>?`");
        assert_eq!(snake_case("toUpperCase"), "to_upper_case");
    }
}
