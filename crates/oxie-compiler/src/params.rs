//! Borrowed parameters: a `String` or `Vec<T>` parameter that a function only reads becomes
//! `&str` / `&[T]`, so callers keep their value (`greet(name)` lends `name` instead of giving it
//! away). Applied to non-`pub` top-level functions, whose callers are all in the crate. A `Vec<T>`
//! is lent only when dropping a `T` just frees memory: lending moves the drop to the caller, and
//! a `MutexGuard` would unlock later.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diag::Span;
use crate::symbols::type_text;
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

/// Std types whose drop only frees memory, once their type arguments' drops do too. Not `Rc` and
/// `Arc`: dropping one changes a count that `Rc::strong_count` and `Rc::try_unwrap` see.
const DROP_FREE: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32", "f64", "bool", "char",
    "str", "Int", "Long", "Short", "Byte", "UInt", "ULong", "UShort", "UByte", "Double", "Float", "Boolean", "Char", "Unit",
    "String", "PathBuf", "OsString", "Duration", "Instant", "SystemTime", "Vec", "VecDeque", "LinkedList", "BinaryHeap",
    "HashMap", "HashSet", "BTreeMap", "BTreeSet", "Option", "Result", "Box", "Cell", "RefCell", "Cow",
];

/// A class, enum or type alias declared in Oxie.
struct Decl<'a> {
    generics: &'a Generics,
    /// The types of its fields (and of its variants' fields), or the aliased type.
    holds: Vec<&'a Type>,
    /// A field without a type whose default isn't a literal, so its type is unknown here.
    untyped: bool,
}

/// The types declared in the crate, to tell whether dropping a value only frees memory.
struct Types<'a> {
    decls: HashMap<&'a str, Vec<Decl<'a>>>,
    /// Types that implement `Drop`, in Oxie or in a `rust { }` block.
    drops: HashSet<String>,
}

impl<'a> Types<'a> {
    fn new(files: &[&'a File]) -> Types<'a> {
        fn gather<'a>(items: &'a [Item], t: &mut Types<'a>) {
            for it in items {
                match &it.kind {
                    ItemKind::Type(d) => {
                        let named = d.variants.iter().flat_map(|v| if let VariantFields::Named(fs) = &v.fields { fs.as_slice() } else { &[] });
                        let mut decl = Decl { generics: &d.generics, holds: d.tuple_fields.iter().map(|(_, ty)| ty).collect(), untyped: false };
                        for f in d.fields.iter().chain(named) {
                            match f.ty {
                                Type::Infer => decl.untyped |= !f.default.as_ref().is_some_and(|e| matches!(e.kind, ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Char(_) | ExprKind::Str(_) | ExprKind::Template(_) | ExprKind::RawStr(_))),
                                _ => decl.holds.push(&f.ty),
                            }
                        }
                        for v in &d.variants {
                            if let VariantFields::Tuple(ts) = &v.fields {
                                decl.holds.extend(ts);
                            }
                        }
                        if d.supertypes.iter().any(|s| s.last_name() == Some("Drop")) {
                            t.drops.insert(d.name.clone());
                        }
                        t.decls.entry(d.name.as_str()).or_default().push(decl);
                        gather(&d.members, t);
                    }
                    ItemKind::TypeAlias { name, generics, ty } => t.decls.entry(name.as_str()).or_default().push(Decl { generics, holds: vec![ty], untyped: false }),
                    ItemKind::Impl(i) if i.trait_.as_ref().and_then(|tr| tr.last_name()) == Some("Drop") => {
                        if let Some(n) = i.self_ty.last_name() {
                            t.drops.insert(n.to_string());
                        }
                    }
                    // `impl Drop for Name` (or `impl<T> std::ops::Drop for Name<T>`) written in Rust
                    ItemKind::Raw(text) => {
                        let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
                        for (i, _) in flat.match_indices("Drop for ") {
                            if flat[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                                continue;
                            }
                            let path: String = flat[i + "Drop for ".len()..].chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':').collect();
                            if let Some(n) = path.rsplit("::").next().filter(|n| !n.is_empty()) {
                                t.drops.insert(n.to_string());
                            }
                        }
                    }
                    ItemKind::Mod { items, .. } | ItemKind::Object { items, .. } => gather(items, t),
                    _ => {}
                }
            }
        }
        let mut t = Types { decls: HashMap::new(), drops: HashSet::new() };
        for f in files {
            gather(&f.items, &mut t);
        }
        t
    }

    /// Does dropping a `t` only free memory? `params` says it for the type parameters in scope.
    /// Types Oxie can't see into (from crates, type parameters, `dyn`, lambdas) may do more.
    /// `visiting` holds the declarations being checked, with their arguments' answers, so a
    /// recursive type (a tree of nodes) is drop-free unless something else in it isn't.
    fn drop_free(&self, t: &Type, params: &HashMap<&str, bool>, visiting: &mut Vec<(&'a str, Vec<bool>)>) -> bool {
        match t {
            Type::Path { segs, .. } => {
                let Some(seg) = segs.last() else { return false };
                if seg.fn_sugar.is_some() {
                    return false;
                }
                if segs.len() == 1 && seg.generics.is_none() {
                    if let Some(&ok) = params.get(seg.name.as_str()) {
                        return ok;
                    }
                }
                let args: Vec<bool> = seg.generics.iter().flatten().filter_map(|a| if let GenericArg::Type(t) = a { Some(self.drop_free(t, params, visiting)) } else { None }).collect();
                let Some((&name, decls)) = self.decls.get_key_value(seg.name.as_str()) else {
                    return DROP_FREE.contains(&seg.name.as_str()) && args.iter().all(|&a| a);
                };
                if self.drops.contains(name) {
                    return false;
                }
                if visiting.iter().any(|(n, a)| *n == name && *a == args) {
                    return true;
                }
                visiting.push((name, args.clone()));
                // with several declarations of the name (in different modules), all of them
                let ok = decls.iter().all(|d| {
                    let names: Vec<&str> = d.generics.params.iter().filter_map(|g| if let GenericParam::Type { name, .. } = g { Some(name.as_str()) } else { None }).collect();
                    let here: HashMap<&str, bool> = names.iter().copied().zip(args.iter().copied()).collect();
                    !d.untyped && names.len() == args.len() && d.holds.iter().all(|h| self.drop_free(h, &here, visiting))
                });
                visiting.pop();
                ok
            }
            Type::Ref { .. } | Type::Ptr { .. } | Type::Never => true,
            Type::Tuple(ts) => ts.iter().all(|t| self.drop_free(t, params, visiting)),
            Type::Array(t, _) | Type::Slice(t) | Type::Option(t) => self.drop_free(t, params, visiting),
            _ => false,
        }
    }

    /// For a `Vec<T>` parameter `name` of the function `f` whose elements may do more than free
    /// memory when dropped: the element type, and why the parameter stays owned.
    fn kept_for_drop(&self, ty: &Type, name: &str, f: &FunDecl) -> Option<(Span, String)> {
        let Type::Path { segs, span, .. } = ty else { return None };
        let Some([GenericArg::Type(elem)]) = segs.last()?.generics.as_deref() else { return None };
        // the function's type parameters can be anything, even when one is named like a type
        let params: HashMap<&str, bool> = f.generics.params.iter().filter_map(|g| if let GenericParam::Type { name, .. } = g { Some((name.as_str(), false)) } else { None }).collect();
        if self.drop_free(elem, &params, &mut Vec::new()) {
            return None;
        }
        let at = if let Type::Path { span, .. } = elem { *span } else { *span };
        Some((at, format!("it drops `{}` when it returns, and dropping a `{}` can do more than free memory", name, type_text(elem))))
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

/// A `String` or `Vec<T>` parameter that may be borrowed: its function (and the index of the
/// file declaring it), position, kind and name, and the function's body.
struct Cand<'a> {
    file: usize,
    fun: &'a FunDecl,
    index: usize,
    kind: Kind,
    name: &'a str,
    body: Block,
    /// For a `Vec` whose elements may do more than free memory when dropped, which keeps it
    /// owned whatever the body does: the element type and why.
    drops: Option<(Span, String)>,
}

/// The (function, parameter index) pairs that can be borrowed, found as a greatest fixpoint.
struct Analysis<'a> {
    decls: HashMap<&'a str, &'a FunDecl>,
    cands: Vec<Cand<'a>>,
    alive: HashSet<(String, usize)>,
}

impl<'a> Analysis<'a> {
    fn new(files: &[&'a File]) -> Analysis<'a> {
        // (file index, item, function)
        let mut funs = Vec::new();
        for (i, f) in files.iter().enumerate() {
            let mut here = Vec::new();
            top_level_funs(&f.items, &mut here);
            funs.extend(here.into_iter().map(|(it, d)| (i, it, d)));
        }
        let mut count: HashMap<&str, usize> = HashMap::new();
        for (_, _, f) in &funs {
            *count.entry(f.name.as_str()).or_default() += 1;
        }
        let decls: HashMap<&str, &FunDecl> = funs.iter().filter(|(_, _, f)| count[f.name.as_str()] == 1).map(|(_, _, f)| (f.name.as_str(), *f)).collect();

        // functions used as values (`xs.map(greet)`) or called through a path (`util.greet(x)`, whose
        // arguments this analysis doesn't follow) keep their signature
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

        let types = Types::new(files);
        let mut cands = Vec::new();
        let mut alive = HashSet::new();
        for &(file, it, f) in &funs {
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
                let drops = if kind == Kind::Vec { types.kept_for_drop(&p.ty, name, f) } else { None };
                if drops.is_none() {
                    alive.insert((f.name.clone(), i));
                }
                cands.push(Cand { file, fun: f, index: i, kind, name: name.as_str(), body: body.clone(), drops });
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

    /// Why a candidate stays owned: the first use that needs the owned value, and what that use
    /// does ("it returns `v`"), else the elements' drop. None when it is borrowed.
    fn kept_why(&self, c: &Cand) -> Option<(Span, String)> {
        match self.first_owned_use(c) {
            Some(at) => Some(kept_use(c, at)),
            None => c.drops.clone(),
        }
    }
}

/// What happened to a `String` or `Vec<T>` parameter that could be borrowed, and why.
#[derive(Clone, Debug)]
pub struct ParamNote {
    /// The index of the file declaring the function.
    pub file: usize,
    pub fun: String,
    /// The function's name in that file (methods and local functions may share the name).
    pub fun_span: Span,
    pub index: usize,
    /// Lent as `&str` / `&[T]` (else kept owned).
    pub borrowed: bool,
    /// "it only reads `s`", "it returns `v`"
    pub why: String,
    /// The use that keeps it owned, or the element type of a `Vec` kept for its drop.
    pub cause: Option<Span>,
}

/// Why the top-level function `fun` keeps its `String` or `Vec` parameter number `index`
/// instead of borrowing it: the index of the file declaring it, the first use that needs the
/// owned value (or the element type, for a `Vec` kept for its drop), and why ("it returns
/// `v`"). None when the parameter is borrowed or isn't a candidate.
pub fn kept_reason(files: &[&File], fun: &str, index: usize) -> Option<(usize, Span, String)> {
    let a = Analysis::new(files);
    let c = a.cands.iter().find(|c| c.fun.name == fun && c.index == index)?;
    let (span, why) = a.kept_why(c)?;
    Some((c.file, span, why))
}

/// The use of the candidate's parameter at `at` that needs the owned value, as a span and a
/// clause ("it returns `v`").
fn kept_use(c: &Cand, at: u32) -> (Span, String) {
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
    (Span { start: at, end: at + len }, what.unwrap_or_else(|| format!("it uses `{}` by value", name)))
}

/// Lends the `String` and `Vec<T>` parameters that functions only read (a `Vec` only when
/// dropping its elements just frees memory), and says what happened to each one that could
/// have been lent.
pub fn borrow_params(files: &mut [&mut File]) -> Vec<ParamNote> {
    let notes: Vec<ParamNote> = {
        let views: Vec<&File> = files.iter().map(|f| &**f).collect();
        let a = Analysis::new(&views);
        a.cands
            .iter()
            .map(|c| {
                let kept = a.kept_why(c);
                let borrowed = kept.is_none();
                let (cause, why) = match kept {
                    Some((span, why)) => (Some(span), why),
                    None => (None, format!("it only reads `{}`", c.name)),
                };
                ParamNote { file: c.file, fun: c.fun.name.clone(), fun_span: c.fun.name_span, index: c.index, borrowed, why, cause }
            })
            .collect()
    };
    let alive: HashSet<(String, usize)> = notes.iter().filter(|n| n.borrowed).map(|n| (n.fun.clone(), n.index)).collect();
    if alive.is_empty() {
        return notes;
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
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(src: &str, fun: &str) -> Option<(String, String)> {
        let (file, diags) = crate::parser::parse_file(src);
        assert!(diags.is_empty(), "{:?}", diags);
        kept_reason(&[&file], fun, 0).map(|(_, s, w)| (src[s.start as usize..s.end as usize].to_string(), w))
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

    #[test]
    fn borrow_params_says_what_it_did() {
        let src = "fun reads(s: String): usize = s.len()\nfun keeps(v: Vec<i32>): Vec<i32> = v\npub fun api(s: String): usize = s.len()\n";
        let (mut file, _) = crate::parser::parse_file(src);
        let notes = borrow_params(&mut [&mut file]);
        let note = |f: &str| notes.iter().find(|n| n.fun == f).map(|n| (n.borrowed, n.why.clone(), n.cause.map(|s| &src[s.start as usize..s.end as usize])));
        assert_eq!(note("reads"), Some((true, "it only reads `s`".into(), None)));
        assert_eq!(note("keeps"), Some((false, "it returns `v`".into(), Some("v"))));
        // `pub` signatures are never changed, so there's nothing to say
        assert_eq!(note("api"), None);
    }

    #[test]
    fn vecs_whose_elements_drop_more_than_memory_stay_owned() {
        let src = r#"
import std::sync::MutexGuard
fun guards(xs: Vec<MutexGuard<i32>>) = println(xs.len())
fun generic<T>(xs: Vec<T>): usize = xs.len()
fun shadows<Point>(xs: Vec<Point>): usize = xs.len()
fun noisy(xs: Vec<Noisy>) = println(xs.len())
fun holders(xs: Vec<Holder<Noisy>>) = println(xs.len())
fun logged(xs: Vec<Logged>) = println(xs.len())
fun counted(xs: Vec<Rc<String>>) = println(xs.len())
fun lazies(xs: Vec<Lazy>) = println(xs.len())
fun keeps(xs: Vec<Noisy>): Vec<Noisy> = xs
fun points(xs: Vec<Point>) = println(xs.len())
fun nested(xs: Vec<Vec<String?>>) = println(xs.len())
fun trees(xs: Vec<Tree>) = println(xs.len())
fun boxes(xs: Vec<Holder<Point>>) = println(xs.len())
fun refs(xs: Vec<&Noisy>) = println(xs.len())
class Noisy(val n: i32)
impl Drop for Noisy { mut fun drop() {} }
class Holder<T>(val inner: T)
class Logged(val id: i32)
rust {
    impl Drop for Logged { fn drop(&mut self) {} }
}
class Lazy {
    var items = vec[1]
}
data class Point(val x: i32, val y: i32)
class Tree(val children: Vec<Tree>, val label: String?) {
    var depth = 0
}
"#;
        let (mut file, diags) = crate::parser::parse_file(src);
        assert!(diags.is_empty(), "{:?}", diags);
        let notes = borrow_params(&mut [&mut file]);
        let note = |f: &str| notes.iter().find(|n| n.fun == f).map(|n| (n.borrowed, n.why.clone(), n.cause.map(|s| src[s.start as usize..s.end as usize].to_string())));
        let dropped = |ty: &str| Some((false, format!("it drops `xs` when it returns, and dropping a `{}` can do more than free memory", ty), Some(ty.to_string())));
        assert_eq!(note("guards"), dropped("MutexGuard<i32>"));
        assert_eq!(note("generic"), dropped("T"));
        // a type parameter, not the class
        assert_eq!(note("shadows"), dropped("Point"));
        assert_eq!(note("noisy"), dropped("Noisy"));
        assert_eq!(note("holders"), dropped("Holder<Noisy>"));
        // `impl Drop` in a `rust { }` block
        assert_eq!(note("logged"), dropped("Logged"));
        // dropping an `Rc` changes its count
        assert_eq!(note("counted"), dropped("Rc<String>"));
        // a field whose type Oxie infers from a call
        assert_eq!(note("lazies"), dropped("Lazy"));
        // what the body does comes first
        assert_eq!(note("keeps"), Some((false, "it returns `xs`".into(), Some("xs".into()))));
        let lent = Some((true, "it only reads `xs`".to_string(), None));
        for f in ["points", "nested", "trees", "boxes", "refs"] {
            assert_eq!(note(f), lent, "{}", f);
        }
    }
}
