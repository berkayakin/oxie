//! Borrowed parameters: a `String` or `Vec<T>` parameter that a function only reads becomes
//! `&str` / `&[T]`, so callers keep their value (`greet(name)` lends `name` instead of giving it
//! away). Applied to non-`pub` top-level functions, whose callers are all in the crate.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
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

pub fn borrow_params(files: &mut [&mut File]) {
    // (function, parameter index) pairs that can be borrowed, found as a greatest fixpoint
    let mut alive: HashSet<(String, usize)> = HashSet::new();
    {
        let mut funs = Vec::new();
        for f in files.iter() {
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
        for f in files.iter() {
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

        let mut cands: Vec<(&FunDecl, usize, Kind, &str, &Block)> = Vec::new();
        let mut blocks: HashMap<&str, Block> = HashMap::new();
        for (it, f) in &funs {
            let eligible = it.vis != Vis::Public
                && decls.contains_key(f.name.as_str())
                && !as_value.contains(&f.name)
                && f.name != "main"
                && f.receiver.is_none()
                && !f.mods.is_async
                && !f.mods.is_const
                && f.mods.abi.is_none()
                && f.body.is_some();
            if !eligible {
                continue;
            }
            let b = match f.body.as_ref().unwrap() {
                FunBody::Block(b) => b.clone(),
                FunBody::Expr(e) => Block { stmts: vec![Stmt::Expr { expr: e.clone(), semi: false }], span: e.span },
            };
            blocks.insert(f.name.as_str(), b);
        }
        for (_, f) in &funs {
            let Some(b) = blocks.get(f.name.as_str()) else { continue };
            for (i, p) in f.params.iter().enumerate() {
                let (Some(kind), Pat::Ident { name, mutable: false, by_ref: false, sub: None, .. }) = (kind_of(&p.ty), &p.pat) else { continue };
                if rebinds(b, name) {
                    continue;
                }
                cands.push((f, i, kind, name.as_str(), b));
                alive.insert((f.name.clone(), i));
            }
        }

        // a call argument is a read when the parameter it goes to is a reference
        let param_index = |callee: &str, pos: usize, arg: &Arg| -> Option<(usize, bool)> {
            let d = decls.get(callee)?;
            let i = match &arg.name {
                Some(n) => d.params.iter().position(|p| p.name() == Some(n.as_str()))?,
                None => pos,
            };
            Some((i, matches!(d.params.get(i)?.ty, Type::Ref { mutable: false, .. })))
        };
        loop {
            let mut removed = false;
            for (f, i, kind, name, body) in &cands {
                if !alive.contains(&(f.name.clone(), *i)) {
                    continue;
                }
                let reads = if *kind == Kind::Str { STR_READS } else { VEC_READS };
                let mut ok: HashSet<u32> = HashSet::new();
                let mut uses: Vec<u32> = Vec::new();
                let mut moved = false;
                walk_block(body, &mut |e| {
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
                            if let Some(c) = callee.ident() {
                                for (pos, a) in args.iter().enumerate() {
                                    if !is_p(&a.value) {
                                        continue;
                                    }
                                    match param_index(c, pos, a) {
                                        Some((_, true)) => {
                                            ok.insert(a.value.span.start);
                                        }
                                        Some((j, false)) if alive.contains(&(c.to_string(), j)) => {
                                            ok.insert(a.value.span.start);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        // a `move` closure or async block would hold the borrow past the call
                        ExprKind::Lambda { is_move: true, body, .. } => {
                            walk_expr(body, &mut |x| {
                                if is_p(x) {
                                    moved = true;
                                }
                                true
                            });
                        }
                        ExprKind::Async { block, .. } => {
                            walk_block(block, &mut |x| {
                                if is_p(x) {
                                    moved = true;
                                }
                                true
                            });
                        }
                        _ => {}
                    }
                    true
                });
                if moved || uses.iter().any(|u| !ok.contains(u)) {
                    alive.remove(&(f.name.clone(), *i));
                    removed = true;
                }
            }
            if !removed {
                break;
            }
        }
    }
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
