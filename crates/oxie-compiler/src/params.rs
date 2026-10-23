//! Borrowed parameters: a `String` or `Vec<T>` parameter that a function only reads becomes
//! `&str` / `&[T]`, so callers keep their value (`greet(name)` lends `name` instead of giving it
//! away). Applied to non-`pub` top-level functions, whose callers are all in the crate.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diag::Span;
use crate::visit::{walk_block, walk_expr};

/// Methods that only read a `String` and exist on `str` with the same meaning.
const STR_READS: &[&str] = &[
    "len", "is_empty", "contains", "starts_with", "ends_with", "chars", "char_indices", "bytes", "lines", "split",
    "splitn", "rsplit", "rsplitn", "split_whitespace", "split_once", "rsplit_once", "trim", "trim_start", "trim_end",
    "strip_prefix", "strip_suffix", "to_uppercase", "to_lowercase", "to_string", "to_owned", "find", "rfind", "replace",
    "parse", "repeat", "as_bytes", "get", "matches", "eq_ignore_ascii_case", "is_char_boundary",
];

/// Methods that only read a `Vec<T>` and exist on slices (including Oxie's list operations).
const VEC_READS: &[&str] = &[
    "len", "is_empty", "contains", "starts_with", "ends_with", "iter", "first", "last", "get", "to_vec", "to_owned",
    "join", "concat", "windows", "chunks", "binary_search", "split_first", "split_last", "map", "map_indexed",
    "filter", "filter_not", "filter_map", "flat_map", "for_each", "for_each_indexed", "find", "find_last",
    "index_of_first", "any", "all", "none", "count", "sum", "sum_of", "max_of", "min_of", "max_by_key", "min_by_key",
    "sorted", "sorted_descending", "sorted_by", "sorted_by_descending", "reversed", "distinct", "distinct_by",
    "chunked", "group_by", "associate_by", "partition", "join_to_string", "join_with", "is_not_empty",
];

/// Formatting macros, which take their arguments by reference.
const PRINTS: &[&str] = &["println", "print", "eprintln", "eprint", "format", "panic", "assert", "assert_eq", "assert_ne", "debug_assert", "debug_assert_eq", "debug_assert_ne"];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Str,
    Vec,
}

fn kind_of(t: &Type) -> Option<Kind> {
    let Type::Path { segs, .. } = t else { return None };
    let [s] = segs.as_slice() else { return None };
    match (s.name.as_str(), s.generics.as_deref()) {
        ("String", None) => Some(Kind::Str),
        ("Vec", Some([GenericArg::Type(_)])) => Some(Kind::Vec),
        _ => None,
    }
}

fn top_level_funs<'a>(items: &'a [Item], out: &mut Vec<(&'a Item, &'a FunDecl)>) {
    for it in items {
        match &it.kind {
            ItemKind::Fun(f) => out.push((it, f)),
            ItemKind::Mod { items, .. } => top_level_funs(items, out),
            _ => {}
        }
    }
}

/// Every function body, test, `init` block and constant in the items.
fn each_body(items: &[Item], f: &mut dyn FnMut(&Block)) {
    let fun = |d: &FunDecl, f: &mut dyn FnMut(&Block)| match &d.body {
        Some(FunBody::Block(b)) => f(b),
        Some(FunBody::Expr(e)) => f(&Block { stmts: vec![Stmt::Expr { expr: e.clone(), semi: false }], span: e.span }),
        None => {}
    };
    for it in items {
        match &it.kind {
            ItemKind::Fun(d) => fun(d, f),
            ItemKind::Type(t) => {
                if let Some(b) = &t.init {
                    f(b);
                }
                each_body(&t.members, f);
            }
            ItemKind::Trait(t) => each_body(&t.members, f),
            ItemKind::Impl(i) => each_body(&i.members, f),
            ItemKind::Mod { items, .. } | ItemKind::Object { items, .. } => each_body(items, f),
            ItemKind::Test(t) => f(&t.body),
            ItemKind::Const(c) | ItemKind::Static(c) => {
                if let Some(e) = &c.value {
                    f(&Block { stmts: vec![Stmt::Expr { expr: e.clone(), semi: false }], span: e.span });
                }
            }
            _ => {}
        }
    }
}

/// Does the body declare another variable called `name` (so uses may not be the parameter)?
fn rebinds(body: &Block, name: &str) -> bool {
    let mut found = false;
    let mut check = |p: &Pat| {
        let mut names = Vec::new();
        p.bindings(&mut names);
        if names.iter().any(|n| n == name) {
            found = true;
        }
    };
    fn lets(b: &Block, check: &mut dyn FnMut(&Pat)) {
        for s in &b.stmts {
            let mut s = s;
            while let Stmt::Attributed { stmt, .. } = s {
                s = stmt;
            }
            if let Stmt::Let { pat, .. } = s {
                check(pat);
            }
        }
    }
    fn conds(c: &Cond, check: &mut dyn FnMut(&Pat)) {
        match c {
            Cond::Let { pat, .. } => check(pat),
            Cond::Chain(parts) => parts.iter().for_each(|c| conds(c, check)),
            Cond::Expr(_) => {}
        }
    }
    lets(body, &mut check);
    walk_block(body, &mut |e| {
        use ExprKind::*;
        match &e.kind {
            Block(b) | Unsafe(b) | ConstBlock(b) | LabeledBlock { block: b, .. } | Async { block: b, .. } | Loop { body: b, .. } | DoWhile { body: b, .. } => lets(b, &mut check),
            While { cond, body, .. } => {
                conds(cond, &mut check);
                lets(body, &mut check);
            }
            If { cond, then, .. } => {
                conds(cond, &mut check);
                lets(then, &mut check);
            }
            For { pat, body, .. } => {
                check(pat);
                lets(body, &mut check);
            }
            Lambda { params, .. } => params.iter().for_each(|p| check(&p.pat)),
            When { binding, arms, .. } => {
                if let Some(p) = binding {
                    check(p);
                }
                for a in arms {
                    for c in &a.conds {
                        if let WhenCond::Pat(p) = c {
                            check(p);
                        }
                    }
                }
            }
            TryCatch { body, catch } => {
                lets(body, &mut check);
                if let Some(c) = catch {
                    check(&c.binding);
                    lets(&c.body, &mut check);
                }
            }
            _ => {}
        }
        true
    });
    found
}

/// A `String` or `Vec<T>` parameter that may be borrowed: its function, position, kind and name,
/// and the function's body.
struct Cand<'a> {
    fun: &'a FunDecl,
    index: usize,
    kind: Kind,
    name: &'a str,
    body: Block,
}

/// The (function, parameter index) pairs that can be borrowed, found as a greatest fixpoint.
struct Analysis<'a> {
    decls: HashMap<&'a str, &'a FunDecl>,
    cands: Vec<Cand<'a>>,
    alive: HashSet<(String, usize)>,
}

impl<'a> Analysis<'a> {
    fn new(files: &[&'a File]) -> Analysis<'a> {
        let mut funs = Vec::new();
        for f in files {
            top_level_funs(&f.items, &mut funs);
        }
        let mut count: HashMap<&str, usize> = HashMap::new();
        for (_, f) in &funs {
            *count.entry(f.name.as_str()).or_default() += 1;
        }
        let decls: HashMap<&str, &FunDecl> = funs.iter().filter(|(_, f)| count[f.name.as_str()] == 1).map(|(_, f)| (f.name.as_str(), *f)).collect();

        // functions used as values (`xs.map(greet)`) or called through a path (`util.greet(x)`,
        // whose arguments aren't matched to parameters) keep their signature
        let mut as_value: HashSet<String> = HashSet::new();
        for f in files {
            each_body(&f.items, &mut |b| {
                let mut callees = HashSet::new();
                walk_block(b, &mut |e| {
                    if let ExprKind::Call { callee, .. } = &e.kind {
                        callees.insert(callee.span.start);
                    }
                    if let ExprKind::Path { segs, .. } = &e.kind {
                        if segs.len() > 1 || !callees.contains(&e.span.start) {
                            as_value.insert(segs.last().cloned().unwrap_or_default());
                        }
                    }
                    true
                });
            });
        }

        let mut cands = Vec::new();
        let mut alive = HashSet::new();
        for (it, f) in &funs {
            let eligible = it.vis != Vis::Public
                && decls.contains_key(f.name.as_str())
                && !as_value.contains(&f.name)
                && f.name != "main"
                && f.receiver.is_none()
                && !f.mods.is_async
                && !f.mods.is_const
                && f.mods.abi.is_none();
            let body = match &f.body {
                Some(FunBody::Block(b)) if eligible => b.clone(),
                Some(FunBody::Expr(e)) if eligible => Block { stmts: vec![Stmt::Expr { expr: e.clone(), semi: false }], span: e.span },
                _ => continue,
            };
            for (i, p) in f.params.iter().enumerate() {
                let (Some(kind), Pat::Ident { name, mutable: false, by_ref: false, sub: None, .. }) = (kind_of(&p.ty), &p.pat) else { continue };
                if rebinds(&body, name) {
                    continue;
                }
                cands.push(Cand { fun: f, index: i, kind, name: name.as_str(), body: body.clone() });
                alive.insert((f.name.clone(), i));
            }
        }
        let mut a = Analysis { decls, cands, alive };
        loop {
            let dead: Vec<(String, usize)> = a
                .cands
                .iter()
                .filter(|c| a.alive.contains(&(c.fun.name.clone(), c.index)) && a.first_owned_use(c).is_some())
                .map(|c| (c.fun.name.clone(), c.index))
                .collect();
            if dead.is_empty() {
                break;
            }
            for d in dead {
                a.alive.remove(&d);
            }
        }
        a
    }

    /// Does an argument of a call to `callee` only lend the value: its parameter is a `&`
    /// reference, or one that is borrowed itself?
    fn lends(&self, callee: &str, pos: usize, arg: &Arg) -> bool {
        let Some(d) = self.decls.get(callee) else { return false };
        let i = match &arg.name {
            Some(n) => match d.params.iter().position(|p| p.name() == Some(n.as_str())) {
                Some(i) => i,
                None => return false,
            },
            None => pos,
        };
        match d.params.get(i) {
            Some(p) if matches!(p.ty, Type::Ref { mutable: false, .. }) => true,
            Some(_) => self.alive.contains(&(callee.to_string(), i)),
            None => false,
        }
    }

    /// The position of the first use of the parameter that needs the owned value: a use that
    /// isn't a read, or a `move` lambda or async block capturing it (which would hold a borrow
    /// past the call).
    fn first_owned_use(&self, c: &Cand) -> Option<u32> {
        let (name, reads) = (c.name, if c.kind == Kind::Str { STR_READS } else { VEC_READS });
        let mut ok: HashSet<u32> = HashSet::new();
        let mut uses: Vec<u32> = Vec::new();
        let mut captured: Vec<u32> = Vec::new();
        walk_block(&c.body, &mut |e| {
            let is_p = |x: &Expr| x.ident() == Some(name);
            match &e.kind {
                ExprKind::Path { .. } if is_p(e) => uses.push(e.span.start),
                ExprKind::MethodCall { recv, name: m, .. } if is_p(recv) && reads.contains(&m.as_str()) => {
                    ok.insert(recv.span.start);
                }
                ExprKind::Template(parts) => {
                    for part in parts {
                        if let TemplatePart::Expr(x, _) = part {
                            if is_p(x) {
                                ok.insert(x.span.start);
                            }
                        }
                    }
                }
                ExprKind::BuiltinMacro { name: m, args } if PRINTS.contains(&m.as_str()) => {
                    args.iter().filter(|a| is_p(&a.value)).for_each(|a| {
                        ok.insert(a.value.span.start);
                    });
                }
                ExprKind::Binary(BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge, a, b) => {
                    for x in [a, b] {
                        if is_p(x) {
                            ok.insert(x.span.start);
                        }
                    }
                }
                ExprKind::In { coll, .. } if is_p(coll) => {
                    ok.insert(coll.span.start);
                }
                ExprKind::Index(a, _) if is_p(a) => {
                    ok.insert(a.span.start);
                }
                ExprKind::Call { callee, args } => {
                    if let Some(f) = callee.ident() {
                        for (pos, a) in args.iter().enumerate() {
                            if is_p(&a.value) && self.lends(f, pos, a) {
                                ok.insert(a.value.span.start);
                            }
                        }
                    }
                }
                ExprKind::Lambda { is_move: true, body, .. } => {
                    walk_expr(body, &mut |x| {
                        if is_p(x) {
                            captured.push(e.span.start);
                        }
                        true
                    });
                }
                ExprKind::Async { block, .. } => {
                    walk_block(block, &mut |x| {
                        if is_p(x) {
                            captured.push(e.span.start);
                        }
                        true
                    });
                }
                _ => {}
            }
            true
        });
        uses.into_iter().filter(|u| !ok.contains(u)).chain(captured).min()
    }
}

/// Why the top-level function `fun` keeps its `String` or `Vec` parameter number `index`
/// instead of borrowing it: the first use that needs the owned value, and what that use does
/// ("it returns `v`"). None when the parameter is borrowed or isn't a candidate.
pub fn kept_reason(files: &[&File], fun: &str, index: usize) -> Option<(Span, String)> {
    let a = Analysis::new(files);
    let c = a.cands.iter().find(|c| c.fun.name == fun && c.index == index)?;
    let at = a.first_owned_use(c)?;
    let name = c.name;
    let is_at = |x: &Expr| x.span.start == at && x.ident() == Some(name);
    let mut what: Option<String> = None;
    for (i, s) in c.body.stmts.iter().enumerate() {
        match s {
            Stmt::Expr { expr, semi: false } if i + 1 == c.body.stmts.len() && is_at(expr) => what = Some(format!("it returns `{}`", name)),
            Stmt::Let { init: Some(x), .. } if is_at(x) => what = Some(format!("it stores `{}` in a variable", name)),
            _ => {}
        }
    }
    walk_block(&c.body, &mut |e| {
        if what.is_some() {
            return false;
        }
        what = match &e.kind {
            ExprKind::Lambda { is_move: true, .. } | ExprKind::Async { .. } if e.span.start == at => Some(format!("a `move` lambda captures `{}`", name)),
            ExprKind::Return(Some(x)) if is_at(x) => Some(format!("it returns `{}`", name)),
            ExprKind::Call { callee, args } if args.iter().any(|a| is_at(&a.value)) => match callee.ident() {
                Some(f) => Some(format!("it passes `{}` to `{}`, which takes it by value", name, f)),
                None => Some(format!("it passes `{}` to a function that takes it by value", name)),
            },
            ExprKind::MethodCall { recv, name: m, .. } if is_at(recv) => Some(format!("it calls `.{}()` on `{}`, which needs the value itself", m, name)),
            ExprKind::MethodCall { args, name: m, .. } if args.iter().any(|a| is_at(&a.value)) => Some(format!("it passes `{}` to `.{}()`", name, m)),
            ExprKind::Assign(_, _, rhs) if is_at(rhs) => Some(format!("it stores `{}`", name)),
            ExprKind::StructLit { fields, .. } if fields.iter().any(|(_, v)| v.as_ref().is_some_and(is_at)) => Some(format!("it stores `{}` in a struct", name)),
            _ => None,
        };
        true
    });
    let len = if what.as_deref().is_some_and(|w| w.starts_with("a `move`")) { 1 } else { name.len() as u32 };
    Some((Span { start: at, end: at + len }, what.unwrap_or_else(|| format!("it uses `{}` by value", name))))
}

pub fn borrow_params(files: &mut [&mut File]) {
    let alive = {
        let views: Vec<&File> = files.iter().map(|f| &**f).collect();
        Analysis::new(&views).alive
    };
    if alive.is_empty() {
        return;
    }
    fn apply(items: &mut [Item], alive: &HashSet<(String, usize)>) {
        for it in items {
            match &mut it.kind {
                ItemKind::Fun(f) => {
                    for (i, p) in f.params.iter_mut().enumerate() {
                        if !alive.contains(&(f.name.clone(), i)) {
                            continue;
                        }
                        let inner = match kind_of(&p.ty) {
                            Some(Kind::Str) => Type::simple("str"),
                            Some(Kind::Vec) => {
                                let Type::Path { segs, .. } = &p.ty else { unreachable!() };
                                let Some([GenericArg::Type(t)]) = segs[0].generics.as_deref() else { unreachable!() };
                                Type::Slice(Box::new(t.clone()))
                            }
                            None => continue,
                        };
                        p.ty = Type::Ref { lifetime: None, mutable: false, inner: Box::new(inner) };
                    }
                }
                ItemKind::Mod { items, .. } => apply(items, alive),
                _ => {}
            }
        }
    }
    for f in files.iter_mut() {
        apply(&mut f.items, &alive);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(src: &str, fun: &str) -> Option<(String, String)> {
        let (file, diags) = crate::parser::parse_file(src);
        assert!(diags.is_empty(), "{:?}", diags);
        kept_reason(&[&file], fun, 0).map(|(s, w)| (src[s.start as usize..s.end as usize].to_string(), w))
    }

    #[test]
    fn kept_parameters_say_why() {
        let src = r#"
fun returns(v: Vec<i32>): Vec<i32> = v
fun stores(s: String): Holder = Holder(name = s)
fun passes(s: String) = consume(s)
fun consume(s: String) { drop(s) }
fun calls(s: String): Vec<u8> = s.into_bytes()
fun captures(s: String) { std::thread::spawn(move { println(s) }) }
fun reads(s: String): usize = s.len()
fun forwards(s: String): usize = reads(s)
data class Holder(name: String)
"#;
        assert_eq!(reason(src, "returns"), Some(("v".into(), "it returns `v`".into())));
        assert_eq!(reason(src, "stores").map(|r| r.1), Some("it passes `s` to `Holder`, which takes it by value".into()));
        assert_eq!(reason(src, "passes").map(|r| r.1), Some("it passes `s` to `consume`, which takes it by value".into()));
        assert_eq!(reason(src, "calls").map(|r| r.1), Some("it calls `.into_bytes()` on `s`, which needs the value itself".into()));
        assert_eq!(reason(src, "captures").map(|r| r.1), Some("a `move` lambda captures `s`".into()));
        // borrowed, so nothing keeps them
        assert_eq!(reason(src, "reads"), None);
        assert_eq!(reason(src, "forwards"), None);
    }
}
