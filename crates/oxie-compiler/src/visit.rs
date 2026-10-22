//! Generic read-only traversal over expressions.

use crate::ast::*;

/// Calls `f` on every expression in pre-order. If `f` returns false, children are skipped.
pub fn walk_expr(e: &Expr, f: &mut dyn FnMut(&Expr) -> bool) {
    if !f(e) {
        return;
    }
    use ExprKind::*;
    match &e.kind {
        Template(parts) => {
            for p in parts {
                if let TemplatePart::Expr(x, _) = p {
                    walk_expr(x, f);
                }
            }
        }
        Unary(_, x) | Try(x) | Unwrap(x) | Await(x) | Cast(x, _) | Paren(x) | Throw(x) => walk_expr(x, f),
        Field { recv, .. } => walk_expr(recv, f),
        Binary(_, a, b) | Assign(_, a, b) | Elvis(a, b) | Pair(a, b) | Index(a, b) | ArrayRepeat(a, b) => {
            walk_expr(a, f);
            walk_expr(b, f);
        }
        In { item, coll, .. } => {
            walk_expr(item, f);
            walk_expr(coll, f);
        }
        Is { expr, .. } => walk_expr(expr, f),
        Call { callee, args } => {
            walk_expr(callee, f);
            for a in args {
                walk_expr(&a.value, f);
            }
        }
        MethodCall { recv, args, .. } => {
            walk_expr(recv, f);
            for a in args {
                walk_expr(&a.value, f);
            }
        }
        BuiltinMacro { args, .. } => {
            for a in args {
                walk_expr(&a.value, f);
            }
        }
        If { cond, then, else_ } => {
            walk_cond(cond, f);
            walk_block(then, f);
            if let Some(e) = else_ {
                walk_expr(e, f);
            }
        }
        When { subject, arms, .. } => {
            if let Some(s) = subject {
                walk_expr(s, f);
            }
            for a in arms {
                for c in &a.conds {
                    match c {
                        WhenCond::In { expr, .. } | WhenCond::Expr(expr) => walk_expr(expr, f),
                        _ => {}
                    }
                }
                if let Some(g) = &a.guard {
                    walk_expr(g, f);
                }
                walk_expr(&a.body, f);
            }
        }
        Block(b) | Unsafe(b) | ConstBlock(b) | LabeledBlock { block: b, .. } | Loop { body: b, .. } | Async { block: b, .. } => walk_block(b, f),
        While { cond, body, .. } => {
            walk_cond(cond, f);
            walk_block(body, f);
        }
        DoWhile { body, cond, .. } => {
            walk_block(body, f);
            walk_expr(cond, f);
        }
        For { iter, body, .. } => {
            walk_expr(iter, f);
            walk_block(body, f);
        }
        Lambda { body, .. } => walk_expr(body, f),
        Return(Some(x)) => walk_expr(x, f),
        Break { value: Some(x), .. } => walk_expr(x, f),
        Tuple(v) | Array(v) | VecLit(v) | MapLit(v) | SetLit(v) => {
            for x in v {
                walk_expr(x, f);
            }
        }
        Range { start, end, .. } => {
            if let Some(x) = start {
                walk_expr(x, f);
            }
            if let Some(x) = end {
                walk_expr(x, f);
            }
        }
        StructLit { fields, base, .. } => {
            for (_, v) in fields {
                if let Some(v) = v {
                    walk_expr(v, f);
                }
            }
            if let Some(b) = base {
                walk_expr(b, f);
            }
        }
        TryCatch { body, catch } => {
            walk_block(body, f);
            if let Some(c) = catch {
                walk_block(&c.body, f);
            }
        }
        _ => {}
    }
}

pub fn walk_cond(c: &Cond, f: &mut dyn FnMut(&Expr) -> bool) {
    match c {
        Cond::Expr(e) => walk_expr(e, f),
        Cond::Let { expr, .. } => walk_expr(expr, f),
        Cond::Chain(parts) => parts.iter().for_each(|c| walk_cond(c, f)),
    }
}

pub fn walk_block(b: &Block, f: &mut dyn FnMut(&Expr) -> bool) {
    for s in &b.stmts {
        walk_stmt(s, f);
    }
}

pub fn walk_stmt(s: &Stmt, f: &mut dyn FnMut(&Expr) -> bool) {
    match s {
        Stmt::Let { init, else_, .. } => {
            if let Some(e) = init {
                walk_expr(e, f);
            }
            if let Some(b) = else_ {
                walk_block(b, f);
            }
        }
        Stmt::Expr { expr, .. } => walk_expr(expr, f),
        Stmt::Attributed { stmt, .. } => walk_stmt(stmt, f),
        Stmt::Item(_) => {}
    }
}

/// Does `body` reference the implicit lambda parameter `it`?
pub fn uses_it(body: &Expr) -> bool {
    let mut found = false;
    walk_expr(body, &mut |e| {
        if found {
            return false;
        }
        match &e.kind {
            ExprKind::Path { segs, generics: None, global: false } if segs.len() == 1 && segs[0] == "it" => {
                found = true;
                false
            }
            // nested lambdas without explicit params have their own `it`
            ExprKind::Lambda { params, .. } if params.is_empty() => false,
            ExprKind::Lambda { params, .. } => !params.iter().any(|p| matches!(&p.pat, Pat::Ident { name, .. } if name == "it")),
            ExprKind::Template(parts) => {
                for p in parts {
                    if let TemplatePart::Expr(x, _) = p {
                        if uses_it(x) {
                            found = true;
                        }
                    }
                }
                false
            }
            ExprKind::Macro { text, .. } | ExprKind::Raw(text) => {
                if crate::sema::contains_word(text, "it") {
                    found = true;
                }
                false
            }
            _ => true,
        }
    });
    found
}

/// Does the block contain an unlabeled `continue` that targets the enclosing loop?
pub fn has_own_continue(b: &Block) -> bool {
    let mut found = false;
    walk_block(b, &mut |e| match &e.kind {
        ExprKind::Continue(None) => {
            found = true;
            false
        }
        ExprKind::Loop { .. } | ExprKind::While { .. } | ExprKind::For { .. } | ExprKind::DoWhile { .. } | ExprKind::Lambda { .. } => false,
        _ => !found,
    });
    found
}

/// Does the function body call the parameter `name` directly (`name(...)`)?
pub fn calls_ident(b: &FunBody, name: &str) -> bool {
    let mut found = false;
    let mut f = |e: &Expr| {
        if let ExprKind::Call { callee, .. } = &e.kind {
            if callee.ident() == Some(name) {
                found = true;
            }
        }
        !found
    };
    match b {
        FunBody::Block(b) => walk_block(b, &mut f),
        FunBody::Expr(e) => walk_expr(e, &mut f),
    }
    found
}

/// Identifiers referenced anywhere in an expression (for error message templates).
pub fn idents_in(e: &Expr, out: &mut Vec<String>) {
    walk_expr(e, &mut |x| {
        if let Some(n) = x.ident() {
            out.push(n.to_string());
        }
        if let ExprKind::Int(n) = &x.kind {
            out.push(format!("__{}", n));
        }
        true
    });
}

/// Is `name` referenced inside any lambda within the function body?
pub fn ident_in_lambda(b: &FunBody, name: &str) -> bool {
    let mut found = false;
    let mut f = |e: &Expr| {
        if let ExprKind::Lambda { body, .. } = &e.kind {
            walk_expr(body, &mut |x| {
                if x.ident() == Some(name) {
                    found = true;
                }
                !found
            });
            return false;
        }
        !found
    };
    match b {
        FunBody::Block(b) => walk_block(b, &mut f),
        FunBody::Expr(e) => walk_expr(e, &mut f),
    }
    found
}

/// Does the expression contain an assignment (used to make closures `mut`)?
pub fn has_assignment(e: &Expr) -> bool {
    let mut found = false;
    walk_expr(e, &mut |x| {
        if matches!(x.kind, ExprKind::Assign(..)) {
            found = true;
        }
        !found
    });
    found
}
