//! Plain-language explanations and quick fixes for the rustc errors Oxie programmers hit most.
//!
//! rustc reports errors against the generated Rust. After they are mapped back to .oxi lines,
//! this module rewrites the common ones in Oxie terms (`val`/`var`, lambdas, `throws`), relabels
//! the related spans ("moved here", "borrowed here") and computes fixes as edits to the .oxi
//! source. Fixes come from two places: rustc's own suggestions whose spans map exactly onto the
//! source, and Oxie-specific edits found by looking at the source text. A declaration the error
//! depends on (the function a value moved into, a type) may be in another file of the crate.

use std::path::{Path, PathBuf};

use crate::cargo::{Edit, Fix, Label, MappedDiag};
use crate::sources::Sources;
use oxie_compiler::ast::{Block, Cond, Expr, ExprKind, File, FunDecl, Item, ItemKind, Pat, Stmt, Type, TypeDecl, UnOp};
use oxie_compiler::diag::Span;

pub fn explain(d: &mut MappedDiag, sources: &Sources) {
    let Some(here) = sources.file(&d.file) else { return };
    let src = here.text.as_str();
    let lines = here.lines();
    if not_optional(d, src) {
        generated_names(d);
        return;
    }
    let names = ticks(&d.message);
    let first = names.first().cloned().unwrap_or_default();
    let code = d.code.clone().unwrap_or_default();
    match code.as_str() {
        "E0382" => moved_value(d, sources, &lines, &first),
        "E0384" => assign_twice(d, &lines, &first),
        "E0596" => borrow_mut(d, sources, &lines, &first),
        "E0499" | "E0502" | "E0505" | "E0506" => borrow_conflict(d, sources, &lines, &first, &code),
        "E0507" => {
            rewrite(d, format!("can't take {} out of a value that is only borrowed", describe(&first)));
            d.notes.retain(|n| !n.contains("`mut "));
            d.notes.push(help("the value belongs to something you only borrowed, so it can't be moved out. Take a copy with `.clone()`, or borrow it with `&`."));
        }
        "E0373" => lambda_outlives(d, &lines, &names),
        "E0597" => {
            rewrite(d, format!("`{}` is dropped while something still refers to it", first));
            d.notes.push(help(&format!("a reference to `{}` is used after `{}` goes out of scope (the end of its block). Declare `{}` in an outer block, or store a copy instead of a reference.", first, first, first)));
            lifetime_hint(d, sources, &lines);
        }
        "E0716" => {
            rewrite(d, "a temporary value is dropped while something still refers to it".to_string());
            d.notes.retain(|n| !n.contains("`let` binding"));
            d.notes.push(help("store the value in a `val` first, then borrow from that variable."));
            lifetime_hint(d, sources, &lines);
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
        "E0277" => trait_missing(d, sources, &lines, &names),
        "E0308" => mismatched(d, src, &lines),
        "E0615" => getter_call(d, &lines, &first),
        // reported without a code
        _ if d.message.starts_with("future cannot be sent between threads safely") => thread_safety(d, sources, ""),
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
    generated_names(d);
}

/// `main() throws` is generated as `__oxie_main` (called from a plain `main`); messages call
/// it by its Oxie name.
fn generated_names(d: &mut MappedDiag) {
    let fix = |s: &mut String| {
        if s.contains("__oxie_main") {
            *s = s.replace("__oxie_main", "main");
        }
    };
    fix(&mut d.message);
    d.label.iter_mut().for_each(fix);
    d.notes.iter_mut().for_each(fix);
    d.secondary.iter_mut().for_each(|s| fix(&mut s.text));
    d.rust_message.iter_mut().for_each(fix);
}

// ---------------------------------------------------------------------------------------------
// individual errors

fn moved_value(d: &mut MappedDiag, sources: &Sources, lines: &[&str], name: &str) {
    refine(d, lines, name);
    // rustc suggests a copy (`.clone()`, `<T as Clone>::clone(..)`, or one taken before a lambda)
    // only when the type implements `Clone`; a plain `.clone()` is its suggestion at the move
    let copyable = d.raw_suggestions.iter().any(|s| s.contains("clone("));
    let clone_here = d.raw_suggestions.iter().any(|s| s == ".clone()");
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
    // the callee's parameter, declared in this file or another one: (file, line, type start,
    // type end, type, declaration line)
    let param = callee.as_ref().and_then(|(f, index)| {
        let file = sources.declaring(&d.file, |ls| fun_param(ls, f, *index).is_some())?;
        let lines = file.lines();
        let (pl, pc, pe, ty) = fun_param(&lines, f, *index)?;
        Some((sources.elsewhere(&file.path, &d.file), pl, pc, pe, ty, lines[pl as usize].to_string()))
    });
    // Oxie already lends `String` and `Vec` parameters that a top-level function only reads; one
    // that stayed owned is kept by the function (stored or returned), so `&` wouldn't compile
    let kept = param.as_ref().is_some_and(|(.., ty, decl)| ["fun ", "private fun ", "internal fun "].iter().any(|p| decl.starts_with(p)) && (ty == "String" || ty.starts_with("Vec<")));
    if kept {
        // say where and why the function keeps it, from the same rules that decided it
        let why = callee.as_ref().and_then(|(f, index)| kept_reason(sources, &d.file, f, *index));
        if let Some((file, l, c, e, why)) = &why {
            d.secondary.push(Label { file: file.clone(), line: *l, col: *c, end_col: *e, text: format!("{} keeps it: {}", callee_text, why), exact: true });
        }
        let because = why.map(|(.., w)| format!(" ({})", w)).unwrap_or_default();
        let instead = match (copyable, in_loop) {
            (true, _) => format!(" Pass a copy with `{}.clone()`.", name),
            (false, false) => format!(" Finish using `{}` before the call.", name),
            (false, true) => String::new(),
        };
        d.notes.push(help(&format!(
            "giving a value to a function moves it: {} keeps it{}, and `{}` can't be used afterwards.{}",
            callee_text, because, name, instead
        )));
    } else {
        let instead = if copyable { format!("Pass a copy with `{}.clone()`, or let {} borrow the value instead", name, callee_text) } else { format!("Let {} borrow the value instead", callee_text) };
        d.notes.push(help(&format!(
            "giving a value to a function or another variable moves it: {} owns it now, and `{}` can't be used afterwards. {}.",
            callee_text, name, instead
        )));
    }
    if let Some((l, c)) = move_site.filter(|_| clone_here) {
        let line = lines.get(l as usize).copied().unwrap_or("");
        if word_at(line, c as usize) == Some(name) {
            let end = c + name.chars().count() as u32;
            d.fixes.push(Fix { title: format!("pass a copy of `{}`", name), edits: vec![edit(l, end, end, ".clone()")] });
        }
    }
    if let (Some((f, _)), Some((file, pl, pc, pe, ty, _))) = (callee, param) {
        if !ty.starts_with('&') && !is_copy_type(&ty) && !kept {
            let borrowed = borrowed_type(&ty);
            d.fixes.push(Fix { title: format!("let `{}` borrow it (parameter type `{}`)", f, borrowed), edits: vec![Edit { file, ..edit(pl, pc, pe, &borrowed) }] });
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

fn borrow_mut(d: &mut MappedDiag, sources: &Sources, lines: &[&str], first: &str) {
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
                // callers that pass `&x` now need `&mut x`. For a top-level function, its calls
                // `f(..)` outside class bodies (where `f` may be a method), in every file of the
                // crate unless another file declares a top-level `f` too; for a method, the calls
                // `x.f(..)` in this file, which may also be of another type's method
                use crate::sources::{declares_top_level_fun, owner_at};
                let here = sources.file(&d.file);
                let is_method = here.is_some_and(|f| owner_at(&f.ast().items, byte_offset(&f.text, pl, pc) as u32).is_some());
                let unique = !is_method && sources.files.iter().filter(|f| declares_top_level_fun(&f.ast().items, &fname)).count() <= 1;
                for file in sources.files.iter().filter(|f| unique || sources.elsewhere(&f.path, &d.file).is_none()) {
                    let other = sources.elsewhere(&file.path, &d.file);
                    for (cl, cline) in file.lines().iter().enumerate() {
                        for (start, args) in calls_of(cline, &fname, is_method) {
                            let in_class = || owner_at(&file.ast().items, byte_offset(&file.text, cl as u32, start as u32) as u32).is_some();
                            if let Some((ac, atext)) = args.get(index).filter(|_| is_method || !in_class()) {
                                if atext.starts_with('&') && !atext.starts_with("&mut ") && (other.is_some() || cl as u32 != pl) {
                                    let c = (start + ac) as u32;
                                    edits.push(Edit { file: other.clone(), ..edit(cl as u32, c, c + 1, "&mut ") });
                                }
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
        mut_self_call(d, sources, d.line, d.col, &name);
        make_var(d, lines, &name);
    }
}

/// The class of the variable `name` used at `line`: the type its `val`/`var` declares or the
/// class its initializer constructs (`val c = Counter()`), or the type of a parameter `name`.
fn local_class(lines: &[&str], name: &str, line: u32) -> Option<String> {
    let decl = declaration(lines, name, line)?;
    let after = decl.strip_prefix(':').or_else(|| decl.strip_prefix('='))?;
    let ty: String = after.trim_start().trim_start_matches('&').trim_start_matches("mut ").chars().take_while(|c| is_ident_char(*c)).collect();
    ty.chars().next().is_some_and(char::is_uppercase).then_some(ty)
}

/// The element type of the list `name` used at `line`: from the `Vec<T>` or `[T]` its `val`/`var`
/// or parameter declares, or `i32`/`f64` for a `vec[...]` of number literals.
fn element_type(lines: &[&str], name: &str, line: u32) -> Option<String> {
    let decl = declaration(lines, name, line)?;
    if let Some(ty) = decl.strip_prefix(':') {
        let ty = ty.trim_start().trim_start_matches('&').trim_start_matches("mut ").trim_start();
        let inner = ty.strip_prefix("Vec<").or_else(|| ty.strip_prefix('['))?;
        return Some(inner.chars().take_while(|c| is_ident_char(*c)).collect());
    }
    let items: Vec<&str> = decl.strip_prefix('=')?.trim_start().strip_prefix("vec[")?.split(']').next()?.split(',').map(str::trim).collect();
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '_');
    let int = |s: &str| digits(s.strip_prefix('-').unwrap_or(s));
    let float = |s: &str| s.strip_prefix('-').unwrap_or(s).split_once('.').is_some_and(|(a, b)| digits(a) && digits(b));
    if items.iter().all(|s| int(s)) {
        Some("i32".into())
    } else {
        items.iter().all(|s| float(s)).then(|| "f64".into())
    }
}

/// What follows the name in the declaration of the variable or parameter `name` used at `line`:
/// `: Type = ...` or `= value` for a `val`/`var`, `: Type...` for a parameter.
fn declaration(lines: &[&str], name: &str, line: u32) -> Option<String> {
    match [find_decl(lines, name, line, "val"), find_decl(lines, name, line, "var")].into_iter().flatten().max() {
        Some((l, c)) => {
            let rest: String = lines[l as usize].chars().skip(c as usize + 3).collect();
            Some(rest.trim_start().strip_prefix(name)?.trim_start().to_string())
        }
        None => {
            let (pl, pc, ..) = param_decl(lines, name, line)?;
            Some(format!(": {}", lines[pl as usize].chars().skip(pc as usize).collect::<String>()))
        }
    }
}

/// `name.method(` at `col` of the line: the method's name.
fn method_called_on(line: &str, col: u32, name: &str) -> Option<String> {
    let rest: String = line.chars().skip(col as usize).collect();
    let after = rest.strip_prefix(name)?.strip_prefix('.')?;
    let m: String = after.chars().take_while(|c| is_ident_char(*c)).collect();
    (!m.is_empty() && after[m.len()..].starts_with('(')).then_some(m)
}

/// When `name.method()` at this position calls a method that Oxie made `&mut self`, says why
/// the call changes `name`: a label where the method changes its object, and a note. Only when
/// the class of `name` is known (a constructor call or a declared type), so a method of another
/// type with the same name isn't blamed.
fn mut_self_call(d: &mut MappedDiag, sources: &Sources, line: u32, col: u32, name: &str) -> bool {
    let Some(here) = sources.file(&d.file) else { return false };
    let lines = here.lines();
    let Some(m) = method_called_on(lines.get(line as usize).copied().unwrap_or(""), col, name) else { return false };
    let class = if name == "this" || name == "self" { crate::sources::owner_at(&here.ast().items, byte_offset(&here.text, line, col) as u32) } else { local_class(&lines, name, line) };
    let Some((file, cause, why)) = class.and_then(|c| sources.mut_self(&m, &c)) else { return false };
    if let Some(c) = cause {
        let idx = oxie_compiler::LineIndex::new(&file.text);
        let (l, cc) = idx.line_col(&file.text, c.start as usize);
        let (el, ec) = idx.line_col(&file.text, c.end as usize);
        let end_col = if el == l { ec as u32 } else { cc as u32 + 1 };
        d.secondary.push(Label { file: sources.elsewhere(&file.path, &d.file), line: l as u32, col: cc as u32, end_col, text: format!("`{}` changes its object here, so it takes `&mut self`", m), exact: true });
    }
    d.notes.push(help(&format!("calling `{}.{}()` changes `{}`: Oxie made `{}` take `&mut self` because {}.", name, m, name, m, why)));
    true
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
            d.secondary.push(Label { file: None, line: l, col: c, end_col: c + 3, text: "declared with `val` here".into(), exact: true });
        }
        d.notes.push(help(&format!("declare it with `var` to allow changes: `var {} = ...`.", name)));
        d.fixes.push(Fix { title: format!("declare `{}` with `var`", name), edits: vec![edit(l, c, c + 3, "var")] });
    }
}

fn borrow_conflict(d: &mut MappedDiag, sources: &Sources, lines: &[&str], name: &str, code: &str) {
    refine(d, lines, name);
    // `*self` in a method is `this`
    let is_this = name == "*self" || name == "self";
    let name = if is_this { "this" } else { name };
    // changed (borrowed for changing) while a shared borrow is still in use
    let changed_while_read = code == "E0502" && d.message.contains("as mutable because it is also borrowed as immutable");
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
        "E0502" if changed_while_read => format!("`{}` can't be changed while it is borrowed", name),
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
    // shapes with a change that costs nothing: a map's entry API, a loop over `iter_mut()`
    if changed_while_read && (entry_instead(d, sources) || iter_mut_instead(d, sources, lines)) {
        return;
    }
    // a change made by calling a method that Oxie made `&mut self`: say why it changes the value
    let changes = |t: &str| t.contains("is changed here") || t.contains("for changing");
    let mut at: Vec<(u32, u32)> = d.label.as_deref().filter(|l| changes(l)).map(|_| (d.line, d.col)).into_iter().collect();
    at.extend(d.secondary.iter().filter(|s| s.file.is_none() && changes(&s.text)).map(|s| (s.line, s.col)));
    let receivers: &[&str] = if is_this { &["this", "self"] } else { &[name] };
    'calls: for (l, c) in at {
        for r in receivers {
            if mut_self_call(d, sources, l, c, r) {
                break 'calls;
            }
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

/// `if (val v = m.get(&k)) return v` right before `m.insert(k, value)`, on a `HashMap` or
/// `BTreeMap` parameter or field: returning the reference keeps `m` borrowed after the `if` too,
/// since rustc doesn't tell the path that returned from the one that goes on. The entry API looks
/// the key up once. When the next statement is `return m.get(&k)!!`, the fix replaces the three.
fn entry_instead(d: &mut MappedDiag, sources: &Sources) -> bool {
    let Some(here) = sources.file(&d.file) else { return false };
    let src = here.text.as_str();
    let Some(borrow) = d.secondary.iter().find(|s| s.text.contains("is borrowed here")) else { return false };
    if !d.secondary.iter().any(|s| s.text.starts_with("returning this value requires")) {
        return false;
    }
    let (get_at, insert_at) = (byte_offset(src, borrow.line, borrow.col) as u32, byte_offset(src, d.line, d.col) as u32);
    let text = |s: Span| src.get(s.start as usize..s.end as usize).unwrap_or("").to_string();
    // the key a lookup or an insert names: `id` for `&id` and for `id.clone()`
    let key = |s: Span| {
        let k = text(s);
        let k = k.trim_start_matches('&');
        [".clone()", ".to_string()", ".to_owned()"].iter().fold(k, |k, m| k.strip_suffix(m).unwrap_or(k)).to_string()
    };
    // `m.get(k)`, alone or unwrapped (`!!`, `.unwrap()`): (map, key)
    let lookup = |e: &Expr| match &e.kind {
        ExprKind::MethodCall { recv, name, args, .. } if name == "get" && args.len() == 1 => Some((text(recv.span), key(args[0].value.span))),
        _ => None,
    };
    let unwrapped = |e: &Expr| match &e.kind {
        ExprKind::Unwrap(g) => lookup(g),
        ExprKind::MethodCall { recv, name, args, .. } if name == "unwrap" && args.is_empty() => lookup(recv),
        _ => None,
    };
    // the `if` and its lookup; the insert, its map, key, key text, value text and whether the value
    // can run in a lambda; the unwrapped lookups that may follow (their span, whether they return)
    let (mut first, mut insert, mut after) = (None, None, Vec::new());
    oxie_compiler::visit::walk_items(&here.ast().items, &mut |e| {
        match &e.kind {
            ExprKind::If { cond, then, else_: None } => {
                if let Cond::Let { pat: Pat::Ident { name: v, .. }, expr, implicit_some: true } = &**cond {
                    let returns_v = matches!(&then.stmts[..], [Stmt::Expr { expr: Expr { kind: ExprKind::Return(Some(r)), .. }, .. }] if r.ident() == Some(v.as_str()));
                    if expr.span.start == get_at && returns_v {
                        first = lookup(expr).map(|l| (e.span, l));
                    }
                }
            }
            ExprKind::MethodCall { recv, name, args, .. } if name == "insert" && args.len() == 2 && e.span.start == insert_at => {
                let (k, value) = (&args[0].value, &args[1].value);
                insert = Some((e.span, text(recv.span), key(k.span), text(k.span), text(value.span), lambda_body_ok(value, &text(recv.span))));
            }
            ExprKind::Return(Some(r)) => after.extend(unwrapped(r).map(|l| (e.span, true, l))),
            _ => after.extend(unwrapped(e).map(|l| (e.span, false, l))),
        }
        true
    });
    let (Some((if_span, (map, k))), Some((call, insert_map, insert_key, key_text, value, value_ok))) = (first, insert) else { return false };
    // nothing but blank space, `;` and (unless `strict`) comments between two statements
    let gap = |a: u32, b: u32, strict: bool| src.get(a as usize..b as usize).is_some_and(|t| t.lines().all(|l| (if strict { l } else { code(l) }).chars().all(|c| c.is_whitespace() || c == ';')));
    let Some(is_param) = map_declared(here.ast(), insert_at, &map) else { return false };
    if map != insert_map || k != insert_key || !gap(if_span.end, call.start, false) {
        return false;
    }
    let entry = format!("{}.entry({}).or_insert_with {{ {} }}", map, key_text, value);
    for s in d.secondary.iter_mut().filter(|s| s.text.starts_with("returning this value requires")) {
        s.text = format!("returned here, so `{}` stays borrowed for the rest of the function", map);
    }
    d.notes.push(help(&format!(
        "returning the reference from `{}.get()` keeps `{}` borrowed until the function returns, even on the path where the key is missing, so `{}.insert()` can't change it. Look the key up once with the entry API, which inserts only when the key is missing: `{}`.",
        map, map, map, entry
    )));
    // the lookup that ends the function, after the insert (the fix keeps comments, so there are
    // none in between); the value runs in a lambda while the map parameter is borrowed (a field's
    // lambda could need `this`)
    let tail = |s: Span, ret: bool| ret || src[s.end as usize..].trim_start().starts_with('}');
    let last = after.into_iter().find(|(s, ret, l)| gap(call.end, s.start, true) && tail(*s, *ret) && *l == (map.clone(), k.clone()));
    if let Some((s, ret, _)) = last.filter(|_| is_param && value_ok && !value.contains('\n') && gap(if_span.end, call.start, true)) {
        let edit = span_edit(src, Span { start: if_span.start, end: s.end }, &format!("{}{}", if ret { "return " } else { "" }, entry));
        d.fixes.push(Fix { title: format!("look `{}` up once with `{}.entry()`", key_text, map), edits: vec![edit] });
    }
    true
}

/// Can `e` be the body of a lambda that runs while `map` is borrowed: it doesn't name the map,
/// use `it` or leave the function.
fn lambda_body_ok(e: &Expr, map: &str) -> bool {
    let mut ok = true;
    oxie_compiler::visit::walk_expr(e, &mut |x| {
        let leaves = matches!(x.kind, ExprKind::Return(_) | ExprKind::Throw(_) | ExprKind::Try(_) | ExprKind::Await(_) | ExprKind::Break { .. } | ExprKind::Continue(_));
        ok &= !leaves && !matches!(x.ident(), Some(n) if n == map || n == "it");
        ok
    });
    ok
}

/// Whether `recv` (`m` or `this.m`, read at `offset`) is declared as a `HashMap` or `BTreeMap`:
/// `Some(true)` for a parameter of the function, `Some(false)` for a field of its class.
fn map_declared(file: &File, offset: u32, recv: &str) -> Option<bool> {
    let (f, class) = fun_at(&file.items, offset)?;
    let field = recv.strip_prefix("this.");
    let param = f.params.iter().find(|p| field.is_none() && p.name() == Some(recv)).map(|p| &p.ty);
    let ty = param.or_else(|| class?.fields.iter().find(|x| x.name == field.unwrap_or(recv)).map(|x| &x.ty))?;
    let ty = match ty {
        Type::Ref { inner, .. } => inner,
        t => t,
    };
    matches!(ty.last_name(), Some("HashMap" | "BTreeMap")).then_some(param.is_some())
}

/// The function or method whose declaration holds `offset`, and its class.
fn fun_at(items: &[Item], offset: u32) -> Option<(&FunDecl, Option<&TypeDecl>)> {
    let it = items.iter().find(|it| it.span.start <= offset && offset < it.span.end)?;
    match &it.kind {
        ItemKind::Fun(f) => Some((f, None)),
        ItemKind::Type(t) => fun_at(&t.members, offset).map(|(f, _)| (f, Some(t))),
        ItemKind::Impl(i) => fun_at(&i.members, offset),
        ItemKind::Mod { items, .. } | ItemKind::Object { items, .. } => fun_at(items, offset),
        _ => None,
    }
}

/// `v[i] = ...` in a loop over `v.iter().enumerate()` whose body uses `v` only as `v[i]`: the loop
/// reads `v` while the body changes it. Changing each element through `v.iter_mut()` is allowed
/// and costs nothing. The fix rewrites a body of one statement when the elements are numbers.
fn iter_mut_instead(d: &mut MappedDiag, sources: &Sources, lines: &[&str]) -> bool {
    let Some(here) = sources.file(&d.file) else { return false };
    let src = here.text.as_str();
    let Some(borrow) = d.secondary.iter().find(|s| s.text.contains("is borrowed here")) else { return false };
    let (iter_at, change_at) = (byte_offset(src, borrow.line, borrow.col) as u32, byte_offset(src, d.line, d.col) as u32);
    let within = |s: Span, at: u32| s.start <= at && at < s.end;
    let mut found = None;
    oxie_compiler::visit::walk_items(&here.ast().items, &mut |e| {
        if let ExprKind::For { pat: Pat::Tuple(ps), iter, body, .. } = &e.kind {
            if within(iter.span, iter_at) && within(body.span, change_at) {
                found = indexed_loop(src, ps, iter, body);
            }
        }
        found.is_none()
    });
    let Some(l) = found else { return false };
    d.notes.retain(|n| !n.contains("index-based loop"));
    let example = match &l.stmt {
        Some(s) => format!("for ({}) {{ {} }}", l.head, s),
        None => format!("for ({}) {{ *{} = ... }}", l.head, l.elem),
    };
    d.notes.push(help(&format!(
        "`{}.iter()` borrows `{}` for the whole loop, so the body can't change `{}[{}]`, even though it is the element the loop is at. Loop over `{}.iter_mut()` instead, which lets the body change each element through `{}`: `{}`.",
        l.list, l.list, l.list, l.index, l.list, l.elem, example
    )));
    let numbers = l.list.chars().all(is_ident_char) && element_type(lines, &l.list, d.line).is_some_and(|t| is_copy_type(&t));
    if l.stmt.is_some() && numbers {
        let edits = l.edits.iter().map(|(s, t)| span_edit(src, *s, t)).collect();
        d.fixes.push(Fix { title: format!("change the elements through `{}.iter_mut()`", l.list), edits });
    }
    true
}

/// A loop `for ((i, x) in v.iter().enumerate())` whose body changes `v[i]` (see `iter_mut_instead`).
struct IndexedLoop {
    list: String,
    index: String,
    elem: String,
    /// The loop's head over `v.iter_mut()`, enumerated when the body uses `i` apart from `v[i]`.
    head: String,
    /// The edits that make the loop change the elements through `x`.
    edits: Vec<(Span, String)>,
    /// The body's one statement after the edits, when each read of `x` became `*x` or stayed the
    /// receiver of a method call.
    stmt: Option<String>,
}

/// The loop over `iter` when it is `v.iter().enumerate()` into `(i, x)` and the body uses `v`
/// only as `v[i]`, and assigns only to `v[i]`.
fn indexed_loop(src: &str, ps: &[Pat], iter: &Expr, body: &Block) -> Option<IndexedLoop> {
    let [Pat::Ident { name: i, span: i_span, .. }, Pat::Ident { name: x, span: x_span, .. }] = ps else { return None };
    let ExprKind::MethodCall { recv: iter_call, name: enumerate, args, .. } = &iter.kind else { return None };
    let ExprKind::MethodCall { recv: list, name: iter_name, args: iter_args, .. } = &iter_call.kind else { return None };
    if enumerate != "enumerate" || iter_name != "iter" || !args.is_empty() || !iter_args.is_empty() {
        return None;
    }
    let text = |s: Span| src.get(s.start as usize..s.end as usize).unwrap_or("");
    let v = text(list.span);
    let is_elem = |e: &Expr| matches!(&e.kind, ExprKind::Index(l, ix) if text(l.span) == v && ix.ident() == Some(i.as_str()));
    let is_x = |e: &Expr| e.ident() == Some(x.as_str());
    // what becomes `*x`: each `v[i]`, and `x` read as an operand; the reads of `x` handled so
    // (or left as they are), and all of them
    let (mut derefs, mut handled, mut reads, mut writes) = (Vec::new(), 0, 0, 0);
    let (mut index_used, mut other) = (false, false);
    for s in &body.stmts {
        oxie_compiler::visit::walk_stmt(s, &mut |e| {
            match &e.kind {
                _ if is_elem(e) => {
                    derefs.push(e.span);
                    return false;
                }
                ExprKind::Assign(_, target, value) => {
                    if is_elem(target) {
                        writes += 1;
                    } else {
                        other = true;
                    }
                    if is_x(value) {
                        derefs.push(value.span);
                        handled += 1;
                    }
                }
                ExprKind::Binary(_, a, b) => {
                    for o in [a, b].into_iter().filter(|o| is_x(o)) {
                        derefs.push(o.span);
                        handled += 1;
                    }
                }
                ExprKind::Unary(UnOp::Neg, a) if is_x(a) => {
                    derefs.push(a.span);
                    handled += 1;
                }
                ExprKind::Unary(UnOp::Deref, a) | ExprKind::MethodCall { recv: a, .. } if is_x(a) => handled += 1,
                _ if is_x(e) => reads += 1,
                _ if e.ident() == Some(i.as_str()) => index_used = true,
                _ if text(e.span) == v => other = true,
                _ => {}
            }
            true
        });
    }
    // the pattern `(i, x) in ` before the iterable
    let open = src[..i_span.start as usize].rfind('(')?;
    let close = x_span.end as usize + src[x_span.end as usize..].find(')')?;
    let blank = |a: usize, b: usize, t: &str| src.get(a..b).is_some_and(|s| s.trim() == t);
    let pattern = blank(open + 1, i_span.start as usize, "") && blank(i_span.end as usize, x_span.start as usize, ",") && blank(x_span.end as usize, close, "") && blank(close + 1, iter.span.start as usize, "in");
    if other || writes == 0 || !pattern {
        return None;
    }
    let head = if index_used { format!("({}, {}) in {}.iter_mut().enumerate()", i, x, v) } else { format!("{} in {}.iter_mut()", x, v) };
    let stmt = match &body.stmts[..] {
        [Stmt::Expr { expr, .. }] if handled == reads => {
            let mut t = text(expr.span).to_string();
            let mut ds: Vec<&Span> = derefs.iter().collect();
            ds.sort_by_key(|s| std::cmp::Reverse(s.start));
            for s in ds {
                t.replace_range((s.start - expr.span.start) as usize..(s.end - expr.span.start) as usize, &format!("*{}", x));
            }
            (!t.contains('\n')).then_some(t)
        }
        _ => None,
    };
    let mut edits = vec![(Span::new(open, iter.span.end as usize), head.clone())];
    edits.extend(derefs.into_iter().map(|s| (s, format!("*{}", x))));
    Some(IndexedLoop { list: v.to_string(), index: i.clone(), elem: x.clone(), head, edits, stmt })
}

/// A value that isn't `Send`/`Sync` used from another thread: usually a `shared class` handle.
fn thread_safety(d: &mut MappedDiag, sources: &Sources, ty: &str) {
    let future = d.message.starts_with("future cannot be");
    // for a future, the type is in a note: "the trait `Send` is not implemented for `Rc<..>`"
    let noted = d.notes.iter().find_map(|n| n.split("is not implemented for `").nth(1).and_then(|t| t.rsplit_once('`')).map(|(t, _)| t.to_string()));
    let ty = if ty.is_empty() { noted.as_deref().unwrap_or("") } else { ty };
    d.notes.retain(|n| !n.contains("full name for the type") && !n.contains("--verbose") && !n.contains("{closure@") && !n.contains("required because it's used within"));
    let class = ty.strip_prefix("Rc<RefCell<").and_then(|t| t.strip_suffix("Data>>"));
    // `shared class C` in some file of the crate: (the file unless it's this one, line, column)
    let decl_in = |lines: &[&str], c: &str| -> Option<(u32, u32)> {
        let pat = format!("shared class {}", c);
        lines.iter().enumerate().find_map(|(l, t)| t.find(&pat).filter(|i| t[i + pat.len()..].chars().next().is_none_or(|c| !is_ident_char(c))).map(|i| (l as u32, t[..i].chars().count() as u32)))
    };
    let decl = class.and_then(|c| {
        let file = sources.declaring(&d.file, |ls| decl_in(ls, c).is_some())?;
        let (l, col) = decl_in(&file.lines(), c)?;
        Some((sources.elsewhere(&file.path, &d.file), l, col))
    });
    match (class, decl) {
        (Some(c), Some((file, l, col))) => {
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
            d.fixes.push(Fix { title: format!("make `{}` usable from several threads (`@sync`)", c), edits: vec![Edit { file, ..edit(l, col, col, "@sync ") }] });
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
        // `move { }` can't follow a call as a trailing lambda (`spawn { .. }`, `f(x) { .. }`),
        // so it goes inside the parentheses
        let before: String = line.chars().take(d.col as usize).collect();
        let before = before.trim_end();
        let close = if rest.starts_with('{') { matching_brace(lines, d.line, d.col) } else { None };
        let edits = match (close, before.chars().last()) {
            (Some((cl, cc)), Some(')')) => {
                let paren = before.chars().count() as u32 - 1;
                let empty = before[..before.len() - 1].trim_end().ends_with('(');
                vec![edit(d.line, paren, d.col, if empty { "move " } else { ", move " }), edit(cl, cc + 1, cc + 1, ")")]
            }
            (Some((cl, cc)), Some(c)) if is_ident_char(c) || c == '>' => vec![edit(d.line, before.chars().count() as u32, d.col, "(move "), edit(cl, cc + 1, cc + 1, ")")],
            _ => vec![edit(d.line, d.col, d.col, "move ")],
        };
        d.fixes.push(Fix { title: "make it a `move` lambda".into(), edits });
    }
}

/// The `}` that closes the `{` at (line, col), skipping string literals: (line, column).
fn matching_brace(lines: &[&str], line: u32, col: u32) -> Option<(u32, u32)> {
    let (mut depth, mut in_str, mut escaped) = (0, false, false);
    for (l, text) in lines.iter().enumerate().skip(line as usize) {
        let skip = if l == line as usize { col as usize } else { 0 };
        for (c, ch) in text.chars().enumerate().skip(skip) {
            match ch {
                _ if escaped => escaped = false,
                '\\' if in_str => escaped = true,
                '"' => in_str = !in_str,
                '{' if !in_str => depth += 1,
                '}' if !in_str => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((l as u32, c as u32));
                    }
                }
                _ => {}
            }
        }
    }
    None
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

fn trait_missing(d: &mut MappedDiag, sources: &Sources, lines: &[&str], names: &[String]) {
    let msg = d.message.clone();
    if msg.contains("cannot be sent between threads safely") || msg.contains("cannot be shared between threads safely") {
        thread_safety(d, sources, names.first().map(String::as_str).unwrap_or(""));
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
    let debug = msg.ends_with("doesn't implement `std::fmt::Debug`") || msg.ends_with("doesn't implement `Debug`");
    if !display && !debug {
        bound_fix(d, lines);
        return;
    }
    // a name read inline maps to its `$`: underline the whole `$low` / `${low}`
    if let Some(len) = placeholder_len(lines.get(d.line as usize).copied().unwrap_or(""), d.col) {
        (d.end_line, d.end_col) = (d.line, d.col + len);
    }
    if is_type_param(ty) {
        bound_fix(d, lines);
        return;
    }
    let base = ty.split('<').next().unwrap_or(ty).trim_start_matches('&').to_string();
    let is_std_collection = matches!(base.as_str(), "Vec" | "Option" | "HashMap" | "HashSet" | "BTreeMap" | "BTreeSet" | "VecDeque" | "Result") || base.starts_with('(') || base.starts_with('[');
    let user_kind = sources.declaring(&d.file, |ls| user_type_kind(ls, &base).is_some()).and_then(|f| user_type_kind(&f.lines(), &base));
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

/// The length of the template placeholder (`$name`, `${...}`) starting at `col`.
fn placeholder_len(line: &str, col: u32) -> Option<u32> {
    let rest: Vec<char> = line.chars().skip(col as usize).collect();
    match rest.get(1)? {
        '{' if rest[0] == '$' => {
            let mut depth = 0;
            for (i, c) in rest.iter().enumerate().skip(1) {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i as u32 + 1);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        c if rest[0] == '$' && (c.is_alphabetic() || *c == '_') => Some(1 + rest[1..].iter().take_while(|c| is_ident_char(**c)).count() as u32),
        _ => None,
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

fn mismatched(d: &mut MappedDiag, src: &str, lines: &[&str]) {
    let borrowed_text = d.label.as_deref().is_some_and(|l| l.contains("expected `String`, found `&str`"));
    if let Some(l) = d.label.clone() {
        let l = l.replace("found `&str`", "found `&str` (borrowed text)").replace("found `String`", "found `String` (owned text)");
        d.label = Some(oxie_types(&l));
    }
    if borrowed_text {
        elvis_borrowed_text(d, lines);
    }
    // `n ?: "none"` with an `i32?`: the default goes to a runtime helper, which rustc mentions
    if d.rust_message.is_none() {
        if let Some((value, _)) = elvis_default_at(src, d.line, d.col) {
            rewrite(d, format!("the default after `?:` doesn't have the type of the value in `{}`", value));
            d.notes.retain(|n| !n.contains("the return type of this call") && !n.contains("method defined here") && !n.contains("arguments to this"));
            d.notes.push(help(&format!("`{} ?: default` gives the value in `{}`, or the default when there is none, so both need the same type.", value, value)));
        }
    }
}

/// `u.address?.city ?: "unknown"` where a `String` is expected: the chain reads `city` through a
/// reference, so the text default makes the result a `&str`. Oxie doesn't copy the field
/// silently; the fix is `u.address?.city.clone() ?: "unknown"`.
fn elvis_borrowed_text(d: &mut MappedDiag, lines: &[&str]) {
    if d.line != d.end_line {
        return;
    }
    let line = lines.get(d.line as usize).copied().unwrap_or("");
    let span: Vec<char> = line.chars().skip(d.col as usize).take(d.end_col.saturating_sub(d.col) as usize).collect();
    let Some(op) = top_level_elvis(&span) else { return };
    let left: String = span[..op].iter().collect::<String>().trim_end().to_string();
    let right: String = span[op + 2..].iter().collect::<String>().trim().to_string();
    if !left.contains("?.") || !right.starts_with('"') || left.ends_with(".clone()") {
        return;
    }
    rewrite(d, format!("`{} ?: {}` is borrowed text (`&str`), but a `String` is expected", left, right));
    d.notes.retain(|n| !n.contains("conversion method"));
    d.notes.push(help(&format!(
        "`{}` reads the field through a reference, so `?:` with a text default gives a `&str` borrowed from it. Oxie doesn't copy the field silently: take a copy with `.clone()` before `?:`, or declare the result as `&str`.",
        left
    )));
    let end = d.col + left.chars().count() as u32;
    d.fixes.push(Fix { title: "use a copy (`.clone()`)".to_string(), edits: vec![edit(d.line, end, end, ".clone()")] });
}

/// Index of the first `?:` outside brackets and string literals.
fn top_level_elvis(s: &[char]) -> Option<usize> {
    let (mut depth, mut in_str, mut escaped) = (0i32, false, false);
    for i in 0..s.len() {
        let c = s[i];
        if in_str {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '?' if depth == 0 && s.get(i + 1) == Some(&':') => return Some(i),
            _ => {}
        }
    }
    None
}

/// `name ?: "x"` or `name?.len()` where `name` can't be null (a `String`, not a `String?`).
/// rustc complains about the runtime helpers they lower to (`ElvisOr`, `as_ref`, `map`), which
/// the program never mentions; the error is replaced by one about the operand, with a fix.
/// Both copies rustc reports for one `?:` end up the same, so only one is shown.
fn not_optional(d: &mut MappedDiag, src: &str) -> bool {
    let msg = d.message.clone();
    // the type rustc names, and whether the error is about `?:` (else about `?.`)
    let (ty, elvis) = match d.code.as_deref().unwrap_or("") {
        "E0277" if msg.contains(": ElvisOr<") || msg.contains(": Elvis`") => (between(&msg, "the trait bound `", ": Elvis"), true),
        "E0277" if msg.contains("the `?` operator can only be applied to values that implement `Try`") => (d.label.as_deref().and_then(|l| between(l, "cannot be applied to type `", "`")), false),
        "E0599" if ["as_ref", "map", "and_then"].iter().any(|m| msg.starts_with(&format!("no method named `{}` found", m))) => (msg.split_once(" found for ").and_then(|(_, r)| ticks(r).first().cloned()), false),
        "E0599" if msg.ends_with("is not an iterator") => (ticks(&msg).first().cloned(), false),
        "E0282" | "E0283" if d.notes.iter().any(|n| n.contains(" as AsRef<")) => (d.notes.iter().find_map(|n| between(n, "<", " as AsRef<")), false),
        _ => return false,
    };
    let (file, _) = oxie_compiler::parser::parse_file(src);
    let at = byte_offset(src, d.line, d.col) as u32;
    // (the value, the text the fix removes or replaces, its replacement)
    let mut found: Option<(Span, Span, &str)> = None;
    // `a ?: b ?: c` is `(a ?: b) ?: c`: both start at `a`, and rustc's error doesn't tell which
    // one has the value that isn't optional
    let (mut elvises, mut chain) = (0, None);
    oxie_compiler::visit::walk_items(&file.items, &mut |e| {
        match &e.kind {
            ExprKind::Elvis(l, r) if elvis && l.span.start == at => {
                // the outermost comes first: its left side is the whole chain before the last `?:`
                chain = chain.or(Some(l.span));
                elvises += 1;
                found = Some((l.span, Span { start: l.span.end, end: r.span.end }, ""));
            }
            // in `a?.b?.c` every link starts at `a`; the first `?.` is the one after the value
            ExprKind::Field { recv, safe: true, .. } | ExprKind::MethodCall { recv, safe: true, .. } if !elvis && recv.span.start == at && found.is_none_or(|(v, ..)| recv.span.end < v.end) => {
                if let Some(q) = src.get(recv.span.end as usize..e.span.end as usize).and_then(|t| t.find("?.")) {
                    let q = recv.span.end + q as u32;
                    found = Some((recv.span, Span { start: q, end: q + 2 }, "."));
                }
            }
            _ => {}
        }
        true
    });
    let Some((value, removed, replacement)) = found else { return false };
    let value = if elvises > 1 { chain.unwrap_or(value) } else { value };
    let shown = source_text(src, value).filter(|t| t.chars().count() <= 40).map(|t| format!("`{}`", t)).unwrap_or_else(|| "the value".into());
    let ty = ty.filter(|t| t != "_");
    if elvises > 1 {
        // a chain: say what's wrong without naming the operand
        rewrite(d, "a `?:` here has a left side that can't be null".to_string());
        d.label = Some(match ty.as_deref() {
            Some(t) if !t.starts_with('{') => format!("one of these is a `{}`, not a `{}?`", t, t),
            _ => "one of these is never null".into(),
        });
        d.notes = vec![help("`a ?: b ?: c` is `(a ?: b) ?: c`. When `a`, or `a ?: b`, always has a value, the `?:` after it is never used: remove that default, or make the value optional.")];
        d.fixes.clear();
    } else if let Some(inner) = ty.as_deref().and_then(|t| t.trim_start_matches('&').strip_prefix("Option<").or_else(|| t.trim_start_matches('&').strip_prefix("Result<"))) {
        // `opt ?: 0` with a `String?`: the default has another type than the value
        let inner = first_generic_arg(inner);
        rewrite(d, format!("the default after `?:` doesn't have the type of the value in {}", shown));
        d.label = Some(format!("holds a `{}`", inner));
        d.notes = vec![help(&format!("`a ?: default` gives the value in `a`, or the default when there is none, so the default must be a `{}` too.", inner))];
    } else {
        let what = match ty.as_deref() {
            Some("{integer}") => format!("{} is an integer", shown),
            Some("{float}") => format!("{} is a number", shown),
            Some(t) => format!("{} has type `{}`", shown, t),
            None => format!("{} is never null", shown),
        };
        let (op, does) = if elvis { ("?:", "`a ?: b` gives `b` when `a` is null (a `T?`) or an error (a `Result`)") } else { ("?.", "`a?.b` reads `b` only when `a` isn't null (a `T?`), and passes an error on (a `Result`)") };
        rewrite(d, format!("`{}` needs an optional value, but {}", op, what));
        d.label = Some(match ty.as_deref() {
            Some(t) if !t.starts_with('{') => format!("type `{}`, not `{}?`", t, t),
            _ => "never null".into(),
        });
        let removed_text = source_text(src, removed).unwrap_or_default();
        let (advice, title) = if elvis {
            let short = !removed_text.trim().is_empty() && removed_text.trim().chars().count() <= 30;
            let title = if short { format!("remove `{}`", removed_text.trim()) } else { "remove the default (`?: ...`)".to_string() };
            // a variable or field can be declared optional; a call's result can't
            let declared = source_text(src, value).filter(|t| t.chars().all(|c| is_ident_char(c) || c == '.')).zip(ty.as_deref().filter(|t| !t.starts_with('{')));
            let or = declared.map(|(v, t)| format!(", or declare `{}` as `{}?` if it can be missing", v, t)).unwrap_or_default();
            (format!("{} always has a value, so the default is never used: remove it{}.", shown, or), title)
        } else {
            (format!("{} always has a value: use `.` instead.", shown), "use `.` instead of `?.`".to_string())
        };
        d.notes = vec![help(&format!("{}. {}", does, advice))];
        let idx = oxie_compiler::LineIndex::new(src);
        let (l, c) = idx.line_col(src, removed.start as usize);
        let (el, ec) = idx.line_col(src, removed.end as usize);
        d.fixes = vec![Fix { title, edits: vec![Edit { file: None, line: l as u32, col: c as u32, end_line: el as u32, end_col: ec as u32, text: replacement.to_string() }] }];
    }
    let idx = oxie_compiler::LineIndex::new(src);
    let (l, c) = idx.line_col(src, value.start as usize);
    let (el, ec) = idx.line_col(src, value.end as usize);
    (d.line, d.col, d.end_line, d.end_col) = (l as u32, c as u32, el as u32, ec as u32);
    d.secondary.clear();
    d.suggestions.clear();
    d.raw_suggestions.clear();
    // in a `throws` function `?.` passes errors on, and rustc's follow-up errors about that
    // `?` come at the same position
    d.covers_position = true;
    true
}

/// The `?:` whose default starts at this position: (the value's text, the default's text).
fn elvis_default_at(src: &str, line: u32, col: u32) -> Option<(String, String)> {
    let (file, _) = oxie_compiler::parser::parse_file(src);
    let at = byte_offset(src, line, col) as u32;
    let mut found = None;
    oxie_compiler::visit::walk_items(&file.items, &mut |e| {
        if let ExprKind::Elvis(l, r) = &e.kind {
            if r.span.start == at {
                found = source_text(src, l.span).zip(source_text(src, r.span));
            }
        }
        true
    });
    found
}

/// The byte offset of a zero-based line and character column.
fn byte_offset(src: &str, line: u32, col: u32) -> usize {
    let start: usize = src.split_inclusive('\n').take(line as usize).map(str::len).sum();
    let rest = src.get(start..).unwrap_or("");
    start + rest.char_indices().nth(col as usize).map_or(rest.len(), |(i, _)| i)
}

/// The source text of a span on one line.
fn source_text(src: &str, span: Span) -> Option<String> {
    let t = src.get(span.start as usize..span.end as usize)?;
    (!t.contains('\n')).then(|| t.to_string())
}

/// The first type argument of the arguments that follow `Option<` or `Result<` (`Vec<i32>` in
/// `Vec<i32>, E>`).
fn first_generic_arg(args: &str) -> &str {
    let mut depth = 0;
    for (i, c) in args.char_indices() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' if depth > 0 => depth -= 1,
            ',' | '>' if depth == 0 => return args[..i].trim(),
            _ => {}
        }
    }
    args.trim()
}

/// The text between the first `start` and the next `end` after it.
fn between(s: &str, start: &str, end: &str) -> Option<String> {
    let (_, rest) = s.split_once(start)?;
    rest.split_once(end).map(|(t, _)| t.to_string())
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
        let e = Edit { file: None, line: s.line, col: s.col, end_line: s.end_line, end_col: s.end_col, text: r.to_string() };
        // an explanation's own fix of the same kind wins (`move` goes inside a call's parentheses)
        if d.fixes.iter().any(|f| f.title == title || f.edits.iter().any(|x| x.line == e.line && x.col == e.col)) {
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
    Edit { file: None, line, col, end_line: line, end_col, text: text.to_string() }
}

/// An edit that replaces the bytes `span` of `src`.
fn span_edit(src: &str, span: Span, text: &str) -> Edit {
    let idx = oxie_compiler::LineIndex::new(src);
    let ((l, c), (el, ec)) = (idx.line_col(src, span.start as usize), idx.line_col(src, span.end as usize));
    Edit { file: None, line: l as u32, col: c as u32, end_line: el as u32, end_col: ec as u32, text: text.to_string() }
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

/// Calls of `name(` on a line, method calls `x.name(` too when `methods`: (column after the
/// parenthesis, [(column offset, argument text)]).
fn calls_of(line: &str, name: &str, methods: bool) -> Vec<(usize, Vec<(usize, String)>)> {
    let mut out = Vec::new();
    let pat = format!("{}(", name);
    let mut from = 0;
    while let Some(i) = line[from..].find(&pat) {
        let i = from + i;
        from = i + pat.len();
        let before = line[..i].chars().next_back();
        if before.is_some_and(is_ident_char) || (!methods && before == Some('.')) || line[..i].trim_end().ends_with("fun") {
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

/// The line without a `//` comment at its end.
fn code(line: &str) -> &str {
    let (mut in_str, mut escaped) = (false, false);
    for (i, c) in line.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if in_str => escaped = true,
            '"' => in_str = !in_str,
            '/' if !in_str && line[i + 1..].starts_with('/') => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Does this line declare `fun name(` (outside a comment)?
fn declares_fun(line: &str, name: &str) -> bool {
    code(line).contains(&format!("fun {}(", name))
}

/// Parameter `index` of `fun name(...)` declared in these lines: (line, type start, type end, type).
fn fun_param(lines: &[&str], name: &str, index: usize) -> Option<(u32, u32, u32, String)> {
    let pat = format!("fun {}(", name);
    let l = lines.iter().position(|t| declares_fun(t, name))?;
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

/// Where and why the function `f` keeps its parameter number `index`: the file when it isn't
/// `current`, zero-based line and columns, and a clause like "it returns `v`". The rules that
/// decided it see the whole crate, as when it was compiled.
fn kept_reason(sources: &Sources, current: &Path, f: &str, index: usize) -> Option<(Option<PathBuf>, u32, u32, u32, String)> {
    let files: Vec<&oxie_compiler::ast::File> = sources.files.iter().map(|s| s.ast()).collect();
    let (i, span, why) = oxie_compiler::params::kept_reason(&files, f, index)?;
    let file = &sources.files[i];
    let idx = oxie_compiler::LineIndex::new(&file.text);
    let (l, c) = idx.line_col(&file.text, span.start as usize);
    let (el, ec) = idx.line_col(&file.text, span.end as usize);
    Some((sources.elsewhere(&file.path, current), l as u32, c as u32, if el == l { ec as u32 } else { c as u32 + 1 }, why))
}

/// When the borrow that ends too early is an argument of a function declared in the crate whose
/// result Oxie tied to all its reference parameters, a hint with the signature that leaves this
/// argument out. Only a hint: Oxie can't tell whether the result really doesn't borrow from it.
fn lifetime_hint(d: &mut MappedDiag, sources: &Sources, lines: &[&str]) {
    let Some((f, index)) = call_around(lines.get(d.line as usize).copied().unwrap_or(""), d.col as usize) else { return };
    let Some(file) = sources.declaring(&d.file, |ls| narrower_lifetime(ls, &f, index).is_some()) else { return };
    let Some(sig) = narrower_lifetime(&file.lines(), &f, index) else { return };
    let place = match sources.elsewhere(&file.path, &d.file) {
        Some(p) => format!(" (in {})", p.file_name().unwrap_or_default().to_string_lossy()),
        None => String::new(),
    };
    d.notes.push(help(&format!(
        "`{}` returns a reference, and Oxie ties it to all of its reference parameters, so the result can't outlive this argument. If the result doesn't borrow from it, name the lifetime yourself{}: `{}`.",
        f, place, sig
    )));
}

/// The signature of the function `f` declared in these lines, when it returns a reference and has
/// no lifetimes written (so Oxie ties the result to all its reference parameters), with the
/// lifetime on every reference parameter except number `index`.
fn narrower_lifetime(lines: &[&str], f: &str, index: usize) -> Option<String> {
    let pat = format!("fun {}(", f);
    let text = code(lines.iter().find(|t| declares_fun(t, f))?).trim();
    if text.contains('\'') {
        return None;
    }
    let open = text.find(&pat)? + pat.len();
    let mut depth = 0;
    let close = text[open..].char_indices().find_map(|(i, c)| {
        match c {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' if depth == 0 => return Some(open + i),
            ')' | ']' | '}' | '>' => depth -= 1,
            _ => {}
        }
        None
    })?;
    let ret = text[close + 1..].trim_start().strip_prefix(':')?.split(" = ").next()?.split(" {").next()?.trim();
    let params: Vec<(String, String)> = split_args(&text[open..]).into_iter().filter_map(|(_, p)| p.split_once(':').map(|(n, t)| (n.trim().to_string(), t.trim().to_string()))).collect();
    let refs = params.iter().filter(|(_, t)| t.starts_with('&')).count();
    if !ret.starts_with('&') || refs < 2 || !params.get(index)?.1.starts_with('&') {
        return None;
    }
    let tie = |t: &str| t.replacen('&', "&'a ", 1);
    let ps: Vec<String> = params.iter().enumerate().map(|(i, (n, t))| format!("{}: {}", n, if i != index && t.starts_with('&') { tie(t) } else { t.clone() })).collect();
    Some(format!("fun {}<'a>({}): {}", f, ps.join(", "), tie(ret)))
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

/// How a type is declared in these lines: "data class", "class", "enum".
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
        let elvis = |s: &str| top_level_elvis(&s.chars().collect::<Vec<_>>());
        assert_eq!(elvis(r#"a?.b ?: "x""#), Some(5));
        assert_eq!(elvis(r#"f("?:", a ?: 1) ?: "x""#), Some(16));
        assert_eq!(elvis("a?.b"), None);
        assert_eq!(byte_offset("ab\ncdé f", 1, 4), 8);
        assert_eq!(between("the trait bound `String: ElvisOr<_>` is not satisfied", "the trait bound `", ": Elvis"), Some("String".into()));
        assert_eq!(code(r#"val s = "a // b" // note"#), r#"val s = "a // b" "#);
        assert_eq!(placeholder_len(r#"println("low: $low items")"#, 14), Some(4));
        assert_eq!(placeholder_len(r#"println("low: ${low.len()} items")"#, 14), Some(12));
        assert_eq!(placeholder_len(r#"println("low: ${low} items")"#, 15), None);
        assert_eq!(first_generic_arg("Vec<String>>"), "Vec<String>");
        assert_eq!(first_generic_arg("HashMap<K, V>, E>"), "HashMap<K, V>");
        assert_eq!(matching_brace(&["spawn { println(\"}\")", "  x }"], 0, 6), Some((1, 4)));
        assert_eq!(local_class(&["fun f(c: &mut Counter) {", "    val d = Counter()", "    val n: Vec<i32> = vec[]", "    val m = make()"], "d", 3), Some("Counter".into()));
        assert_eq!(local_class(&["fun f(c: &mut Counter) {", "    c.hit()"], "c", 1), Some("Counter".into()));
        assert_eq!(local_class(&["fun f() {", "    val m = make()"], "m", 1), None);
        assert!(declares_fun("fun keep(v: Vec<i32>) = v", "keep") && !declares_fun("// see fun keep(v)", "keep"));
        assert_eq!(element_type(&["fun f() {", "    var v = vec[1, 2, 3]", "    v[0] = 4"], "v", 2), Some("i32".into()));
        assert_eq!(element_type(&["    var w = vec[1.5, -2.0]"], "w", 0), Some("f64".into()));
        assert_eq!(element_type(&["    var w = vec[1, 2.5]"], "w", 0), None);
        assert_eq!(element_type(&["fun f(v: &mut Vec<u8>) {", "    v[0] = 1"], "v", 1), Some("u8".into()));
        assert_eq!(element_type(&["    var s: Vec<String> = vec[]"], "s", 0), Some("String".into()));
    }

    #[test]
    fn shapes_with_a_change_that_costs_nothing() {
        // the loop over `iter_mut()` and the body's statement, for the loop in `src`
        let looped = |src: &str| {
            let file = oxie_compiler::parser::parse_file(src).0;
            let mut found = None;
            oxie_compiler::visit::walk_items(&file.items, &mut |e| {
                if let ExprKind::For { pat: Pat::Tuple(ps), iter, body, .. } = &e.kind {
                    found = indexed_loop(src, ps, iter, body).map(|l| (l.head, l.stmt));
                }
                true
            });
            found
        };
        let body = |b: &str| looped(&format!("fun f(v: &mut Vec<i32>) {{\n    for ((i, x) in v.iter().enumerate()) {{ {} }}\n}}\n", b));
        assert_eq!(body("v[i] = x * 2"), Some(("x in v.iter_mut()".into(), Some("*x = *x * 2".into()))));
        assert_eq!(body("v[i] += x.pow(2) + i as i32"), Some(("(i, x) in v.iter_mut().enumerate()".into(), Some("*x += x.pow(2) + i as i32".into()))));
        // `x` passed on: the loop, without the statement
        assert_eq!(body("v[i] = g(x)"), Some(("x in v.iter_mut()".into(), None)));
        // another element, the list itself, or a change of something else
        assert_eq!(body("v[i] = v[0] + x"), None);
        assert_eq!(body("v[i] = x + v.len() as i32"), None);
        assert_eq!(body("n = x * 2"), None);

        let src = "class C(var m: HashMap<u32, String> = HashMap::new()) {\n    fun f(n: &mut BTreeMap<u32, u32>, s: &mut Vec<u32>) { m.len() }\n}\n";
        let file = oxie_compiler::parser::parse_file(src).0;
        let at = src.find("m.len()").unwrap() as u32;
        assert_eq!(map_declared(&file, at, "n"), Some(true));
        assert_eq!(map_declared(&file, at, "m"), Some(false));
        assert_eq!(map_declared(&file, at, "this.m"), Some(false));
        assert_eq!(map_declared(&file, at, "s"), None);

        let value = |v: &str| {
            let src = format!("fun f() {{ g({}) }}\n", v);
            let file = oxie_compiler::parser::parse_file(&src).0;
            let mut ok = None;
            oxie_compiler::visit::walk_items(&file.items, &mut |e| {
                if let ExprKind::Call { args, .. } = &e.kind {
                    ok = ok.or(Some(lambda_body_ok(&args[0].value, "cache")));
                }
                true
            });
            ok.unwrap()
        };
        assert!(value("load(id)"));
        assert!(!value("cache.len()") && !value("load(id)?") && !value("it + 1"));
    }

    /// A rustc error mapped to `file` (zero-based line and columns).
    fn diag(file: &str, code: &str, message: &str, line: u32, col: u32, end_col: u32) -> MappedDiag {
        MappedDiag {
            level: "error".into(),
            code: Some(code.into()),
            message: message.into(),
            file: PathBuf::from(file),
            line,
            col,
            end_line: line,
            end_col,
            label: None,
            notes: Vec::new(),
            from_oxie: true,
            secondary: Vec::new(),
            suggestions: Vec::new(),
            raw_suggestions: Vec::new(),
            rust_message: None,
            gen_text: None,
            fixes: Vec::new(),
            covers_position: false,
        }
    }

    #[test]
    fn callers_in_other_files_get_mut_but_not_methods_of_the_same_name() {
        let s = Sources::from_texts(&[
            ("/crate/text.oxi", "fun add_one(v: &Vec<i32>) {\n    v.push(1)\n}\n"),
            ("/crate/main.oxi", "class Tally(var n: i32 = 0) {\n    fun add_one(x: &i32) { n += *x }\n}\n\nfun main() {\n    var nums = vec[1]\n    add_one(&nums)\n    var t = Tally()\n    t.add_one(&nums[0])\n}\n"),
        ]);
        let mut d = diag("/crate/text.oxi", "E0596", "cannot borrow `*v` as mutable, as it is behind a `&` reference", 1, 4, 5);
        explain(&mut d, &s);
        let fix = d.fixes.iter().find(|f| f.title == "let `add_one` change `v` (`&mut`)").unwrap_or_else(|| panic!("{:?}", d.fixes));
        let at: Vec<(Option<&str>, u32, u32)> = fix.edits.iter().map(|e| (e.file.as_deref().and_then(|f| f.to_str()), e.line, e.col)).collect();
        // the parameter, and the call `add_one(&nums)` in main.oxi; not `t.add_one(&nums[0])`
        assert_eq!(at, vec![(None, 0, 15), (Some("/crate/main.oxi"), 6, 12)]);
    }

    #[test]
    fn declarations_are_found_in_other_files() {
        let s = Sources::from_texts(&[
            ("/crate/main.oxi", "fun main() {\n    keep(v)\n}\n"),
            ("/crate/a.oxi", "fun keep(v: Vec<i32>): Vec<i32> = v\nfun twice() {}\n"),
            ("/crate/b.oxi", "fun twice() {}\n"),
        ]);
        let declares = |name: &'static str| move |ls: &[&str]| ls.iter().any(|t| declares_fun(t, name));
        let main = Path::new("/crate/main.oxi");
        assert_eq!(s.declaring(main, declares("keep")).map(|f| f.path.clone()), Some(PathBuf::from("/crate/a.oxi")));
        assert_eq!(s.elsewhere(Path::new("/crate/a.oxi"), main), Some(PathBuf::from("/crate/a.oxi")));
        assert_eq!(s.elsewhere(main, main), None);
        // declared in two other files: which one is meant is unknown
        assert!(s.declaring(main, declares("twice")).is_none());
        // the error's own file wins
        assert_eq!(s.declaring(Path::new("/crate/b.oxi"), declares("twice")).map(|f| f.path.clone()), Some(PathBuf::from("/crate/b.oxi")));
    }
}
