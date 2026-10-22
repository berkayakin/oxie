//! Lowering of statements and expressions.

use crate::ast::*;
use crate::diag::Span;
use crate::lower::*;
use crate::parser::is_type_like;
use crate::sema::{ParamInfo, Recv, VariantShape};
use crate::visit;

fn is_const_like(s: &str) -> bool {
    s.len() > 1 && s.chars().next().map_or(false, |c| c.is_uppercase()) && s.chars().all(|c| c.is_uppercase() || c == '_' || c.is_ascii_digit())
}

fn escape_braces(s: &str) -> String {
    s.replace('{', "{{").replace('}', "}}")
}

fn is_ok_err_call(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Call { callee, .. } => matches!(callee.ident(), Some("Ok") | Some("Err")),
        ExprKind::Paren(x) => is_ok_err_call(x),
        _ => false,
    }
}

fn is_literal(e: &Expr) -> bool {
    matches!(e.kind, ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Char(_) | ExprKind::Str(_) | ExprKind::RawStr(_))
        || matches!(&e.kind, ExprKind::Unary(UnOp::Neg, x) if is_literal(x))
}

fn is_string_lit(e: &Expr) -> bool {
    matches!(e.kind, ExprKind::Str(_) | ExprKind::Template(_))
}

/// Postfix chain: base expression and links (innermost first).
fn chain_links(e: &Expr) -> (&Expr, Vec<&Expr>) {
    let mut links = Vec::new();
    let mut cur = e;
    loop {
        match &cur.kind {
            ExprKind::Field { recv, .. } | ExprKind::MethodCall { recv, .. } => {
                links.push(cur);
                cur = recv;
            }
            ExprKind::Index(recv, _) => {
                links.push(cur);
                cur = recv;
            }
            _ => break,
        }
    }
    links.reverse();
    (cur, links)
}

fn link_is_safe(l: &Expr) -> bool {
    matches!(l.kind, ExprKind::Field { safe: true, .. } | ExprKind::MethodCall { safe: true, .. })
}

fn has_safe_link(e: &Expr) -> bool {
    let (_, links) = chain_links(e);
    links.iter().any(|l| link_is_safe(l))
}

impl<'a> Lowerer<'a> {
    // ------------------------------------------------------------------ statements

    /// Emits statements on new lines. With `tail`, the last expression statement is the block value.
    pub(crate) fn stmts_with_tail(&mut self, stmts: &[Stmt], tail: Option<&Target>) {
        let tail_idx = match tail {
            Some(_) => match stmts.last() {
                Some(Stmt::Expr { semi: false, .. }) => Some(stmts.len() - 1),
                _ => None,
            },
            None => None,
        };
        for (i, s) in stmts.iter().enumerate() {
            self.e.nl();
            if Some(i) == tail_idx {
                if let Stmt::Expr { expr, .. } = s {
                    self.e.mark(expr.span);
                    self.tail(expr, tail.unwrap());
                }
            } else {
                self.stmt(s);
            }
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let { pat, mutable: _, ty, init, else_, span } => {
                self.e.mark(*span);
                self.let_stmt(pat, ty.as_ref(), init.as_ref(), else_.as_ref());
            }
            Stmt::Expr { expr, semi } => {
                self.e.mark(expr.span);
                let block_like = self.stmt_expr(expr);
                if !block_like || *semi {
                    self.e.w(";");
                }
            }
            Stmt::Item(b) => {
                self.item(&b.0, ItemCtx::Local);
            }
        }
    }

    fn let_stmt(&mut self, pat: &Pat, ty: Option<&Type>, init: Option<&Expr>, else_: Option<&Block>) {
        let p = self.pat(pat);
        self.e.w(&format!("let {}", p));
        let fn_lambda = matches!(ty, Some(Type::Fn { .. })) && init.map_or(false, |e| matches!(e.kind, ExprKind::Lambda { .. } | ExprKind::Path { .. }));
        if let Some(t) = ty {
            if !fn_lambda {
                let tt = self.ty(t, TyPos::Local);
                self.e.w(&format!(": {}", tt));
            }
        }
        if let Some(e) = init {
            self.e.w(" = ");
            match ty {
                Some(Type::Fn { params, ret }) if fn_lambda => {
                    if matches!(e.kind, ExprKind::Lambda { .. }) {
                        self.lambda(e, Some(params), Some(ret));
                    } else {
                        self.expr(e, P_LOWEST);
                    }
                }
                Some(t) => self.coerce(e, t, TyPos::Local),
                None => self.expr(e, P_LOWEST),
            }
        }
        if let Some(b) = else_ {
            self.e.w(" else ");
            self.block_stmt(b);
        }
        self.e.w(";");
        self.bind_pat(pat, ty.cloned());
    }

    /// Emits an expression in statement position. Returns true if it is block-like (no `;` needed).
    fn stmt_expr(&mut self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::If { cond, then, else_ } => {
                self.if_expr(cond, then, else_.as_deref(), None);
                true
            }
            ExprKind::When { subject, arms, rust_match } => {
                self.when_expr(subject.as_deref(), arms, *rust_match, None, e.span);
                true
            }
            ExprKind::Block(b) => {
                self.block_stmt(b);
                true
            }
            ExprKind::Unsafe(b) => {
                self.e.w("unsafe ");
                self.block_stmt(b);
                true
            }
            ExprKind::Loop { .. } | ExprKind::While { .. } | ExprKind::For { .. } | ExprKind::DoWhile { .. } => {
                self.expr(e, P_LOWEST);
                true
            }
            ExprKind::TryCatch { body, catch } => {
                self.try_catch(body, catch.as_deref(), None);
                true
            }
            ExprKind::Field { .. } | ExprKind::MethodCall { .. } if has_safe_link(e) && self.safe_chain_stmt(e) => true,
            ExprKind::Paren(x) => self.stmt_expr(x),
            ExprKind::Call { callee, args } if callee.ident() == Some("run") && self.is_builtin_call("run") && args.len() == 1 => {
                if let ExprKind::Lambda { body, .. } = &args[0].value.kind {
                    match &body.kind {
                        ExprKind::Block(b) => self.block_stmt(b),
                        _ => {
                            self.e.w("{");
                            self.e.indent();
                            self.e.nl();
                            let bl = self.stmt_expr(body);
                            if !bl {
                                self.e.w(";");
                            }
                            self.e.dedent();
                            self.e.nl();
                            self.e.w("}");
                        }
                    }
                    true
                } else {
                    self.expr(e, P_LOWEST);
                    false
                }
            }
            _ => {
                self.expr(e, P_LOWEST);
                false
            }
        }
    }

    /// `{ stmts }` in statement mode.
    pub(crate) fn block_stmt(&mut self, b: &Block) {
        self.e.w("{");
        self.e.indent();
        self.push_scope();
        self.stmts_with_tail(&b.stmts, None);
        self.pop_scope();
        self.e.dedent();
        if !b.stmts.is_empty() {
            self.e.nl();
        }
        self.e.w("}");
    }

    /// `{ stmts; tail }` in value mode.
    fn block_value(&mut self, b: &Block, t: &Target) {
        self.e.w("{");
        self.e.indent();
        self.push_scope();
        self.stmts_with_tail(&b.stmts, Some(t));
        let has_tail = matches!(b.stmts.last(), Some(Stmt::Expr { semi: false, .. }));
        if !has_tail {
            if let Some(w) = &t.wrap {
                if !ends_diverging(&b.stmts) {
                    self.e.nl();
                    self.e.w(&format!("{}(())", w));
                }
            }
        }
        self.pop_scope();
        self.e.dedent();
        if !b.stmts.is_empty() || t.wrap.is_some() {
            self.e.nl();
        }
        self.e.w("}");
    }

    /// Emits a value-producing expression: coercion to the target type and `Ok` wrapping.
    fn tail(&mut self, e: &Expr, t: &Target) {
        if e.diverges() {
            self.expr(e, P_LOWEST);
            return;
        }
        match &e.kind {
            ExprKind::If { cond, then, else_ } if else_.is_some() => self.if_expr(cond, then, else_.as_deref(), Some(t)),
            ExprKind::When { subject, arms, rust_match } => self.when_expr(subject.as_deref(), arms, *rust_match, Some(t), e.span),
            ExprKind::Block(b) => self.block_value(b, t),
            ExprKind::Paren(x) => self.tail(x, t),
            _ => {
                let wrap = t.wrap.clone().filter(|_| !is_ok_err_call(e));
                if let Some(w) = &wrap {
                    self.e.w(&format!("{}(", w));
                }
                match &t.ty {
                    Some(ty) => self.coerce(e, ty, TyPos::Local),
                    None => self.expr(e, P_LOWEST),
                }
                if wrap.is_some() {
                    self.e.w(")");
                }
            }
        }
    }

    /// Emits `e` converted to `ty` where Brs allows implicit conversion.
    pub(crate) fn coerce(&mut self, e: &Expr, ty: &Type, pos: TyPos) {
        if matches!(e.kind, ExprKind::If { .. } | ExprKind::When { .. } | ExprKind::Block(_)) && !matches!(&e.kind, ExprKind::If { else_: None, .. }) {
            self.tail(e, &Target::ty(ty));
            return;
        }
        match ty {
            Type::Option(inner) => match &e.kind {
                ExprKind::Null => self.e.w("None"),
                _ if self.is_value_like(e) => {
                    self.e.w("Some(");
                    self.coerce(e, inner, TyPos::Generic);
                    self.e.w(")");
                }
                _ if self.is_place(e) && !self.place_is_option(e) => {
                    self.expr(e, P_POSTFIX);
                    self.e.w(".into()");
                }
                _ => self.expr(e, P_LOWEST),
            },
            Type::Path { segs, .. } if segs.len() == 1 && segs[0].name == "String" && segs[0].generics.is_none() => match &e.kind {
                ExprKind::Str(parts) => self.e.w(&format!("String::from(\"{}\")", crate::lower::lit_parts(parts))),
                ExprKind::RawStr(s) => self.e.w(&format!("String::from({})", s)),
                _ => self.expr(e, P_LOWEST),
            },
            Type::Fn { params, ret } => {
                let boxed = matches!(pos, TyPos::Field | TyPos::Generic | TyPos::Local);
                if matches!(e.kind, ExprKind::Lambda { .. }) {
                    if boxed {
                        self.e.w("Box::new(");
                    }
                    self.lambda(e, Some(params), Some(ret));
                    if boxed {
                        self.e.w(")");
                    }
                } else if boxed && matches!(e.kind, ExprKind::Path { .. }) && !self.is_place(e) {
                    self.e.w("Box::new(");
                    self.expr(e, P_LOWEST);
                    self.e.w(")");
                } else {
                    self.expr(e, P_LOWEST);
                }
            }
            Type::Path { segs, .. } if segs.len() == 1 && segs[0].name == "Vec" && segs[0].generics.as_ref().map_or(false, |g| g.len() == 1) => {
                let elem = match &segs[0].generics.as_ref().unwrap()[0] {
                    GenericArg::Type(t) => Some(t.clone()),
                    _ => None,
                };
                match (&e.kind, elem) {
                    (ExprKind::VecLit(items), Some(elem)) if !items.iter().any(|x| matches!(x.kind, ExprKind::ArrayRepeat(..))) => {
                        self.e.w("vec![");
                        for (i, x) in items.iter().enumerate() {
                            if i > 0 {
                                self.e.w(", ");
                            }
                            self.coerce(x, &elem, TyPos::Generic);
                        }
                        self.e.w("]");
                    }
                    _ => self.expr(e, P_LOWEST),
                }
            }
            Type::Path { segs, .. }
                if segs.len() == 1 && matches!(pos, TyPos::Field | TyPos::Generic | TyPos::Local) && self.is_trait_name(&segs[0].name) && self.is_constructed_value(e) =>
            {
                self.e.w("Box::new(");
                self.expr(e, P_LOWEST);
                self.e.w(")");
            }
            _ => self.expr(e, P_LOWEST),
        }
    }

    /// Values that can never be an `Option` themselves (safe to wrap in `Some`).
    fn is_value_like(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Char(_) | ExprKind::Str(_) | ExprKind::RawStr(_)
            | ExprKind::Template(_) | ExprKind::StructLit { .. } | ExprKind::Tuple(_) | ExprKind::Array(_) | ExprKind::VecLit(_)
            | ExprKind::MapLit(_) | ExprKind::SetLit(_) | ExprKind::Cast(..) | ExprKind::Range { .. } | ExprKind::Lambda { .. }
            | ExprKind::Pair(..) | ExprKind::Is { .. } | ExprKind::In { .. } => true,
            ExprKind::Binary(..) | ExprKind::Unary(UnOp::Neg, _) | ExprKind::Unary(UnOp::Not, _) => true,
            ExprKind::Paren(x) => self.is_value_like(x),
            _ => self.is_constructed_value(e),
        }
    }

    /// Constructor calls: `Point(1, 2)`, `Shape.Circle(1.0)`, `Point { .. }`.
    fn is_constructed_value(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::StructLit { .. } => true,
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Path { segs, .. } => {
                    let last = segs.last().map(|s| s.as_str()).unwrap_or("");
                    is_type_like(last) && !matches!(last, "Some" | "Ok" | "Err" | "None" | "Box" | "Rc" | "Arc")
                }
                _ => false,
            },
            ExprKind::MethodCall { recv, name, .. } => {
                is_type_like(name) && matches!(self.as_path(recv), Some((_, PathKind::Type)))
            }
            _ => false,
        }
    }

    pub(crate) fn is_place(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Path { segs, generics: None, .. } => {
                segs.len() == 1 && !is_type_like(&segs[0]) && !self.k.funs.contains_key(&segs[0]) || is_const_like(segs.last().unwrap())
            }
            ExprKind::SelfValue => true,
            ExprKind::Field { recv, safe: false, .. } => self.as_path(e).is_none() && self.is_place(recv),
            ExprKind::Index(r, _) => self.is_place(r),
            ExprKind::Paren(x) => self.is_place(x),
            ExprKind::Unary(UnOp::Deref, x) => self.is_place(x),
            _ => false,
        }
    }

    fn place_is_option(&self, e: &Expr) -> bool {
        if let Some(n) = e.ident() {
            if let Some(Some(t)) = self.local(n) {
                return matches!(t, Type::Option(_)) || t.single_name() == Some("Option");
            }
            if let Some(c) = self.class.as_ref() {
                if let Some(t) = c.fields.get(n) {
                    return matches!(t, Type::Option(_)) || t.single_name() == Some("Option");
                }
            }
        }
        false
    }

    // ------------------------------------------------------------------ expressions

    fn prec_of(&self, e: &Expr) -> u8 {
        use ExprKind::*;
        match &e.kind {
            Assign(..) | Return(_) | Break { .. } | Continue(_) | Throw(_) | Lambda { .. } => P_LOWEST,
            Range { .. } => P_RANGE,
            Binary(op, ..) => {
                if *op == BinOp::Add && self.is_string_concat(e) {
                    P_PRIMARY
                } else {
                    op.prec()
                }
            }
            Cast(..) => P_CAST,
            Unary(..) => P_UNARY,
            Is { negated: true, .. } | In { negated: true, .. } => P_UNARY,
            Try(_) if self.in_try_block() => P_PRIMARY,
            Try(_) | Unwrap(_) | Await(_) | Field { .. } | MethodCall { .. } | Call { .. } | Index(..) | In { .. } => P_POSTFIX,
            Paren(x) => self.prec_of(x),
            _ => P_PRIMARY,
        }
    }

    fn emits_block_like(&self, e: &Expr) -> bool {
        use ExprKind::*;
        match &e.kind {
            If { .. } | When { .. } | Block(_) | Unsafe(_) | Loop { .. } | While { .. } | For { .. } | DoWhile { .. } | TryCatch { .. } | Async { .. } => true,
            Elvis(_, r) => !is_literal(r),
            Try(_) => self.in_try_block(),
            MethodCall { name, args, .. } => (name == "copy" && !args.is_empty() && args.iter().all(|a| a.name.is_some())) || name == "let",
            Paren(x) => self.emits_block_like(x),
            _ => false,
        }
    }

    fn in_try_block(&self) -> bool {
        self.fns.last().map_or(false, |f| !f.tries.is_empty())
    }

    pub(crate) fn expr(&mut self, e: &Expr, min: u8) {
        let parens = self.prec_of(e) < min || (min > P_LOWEST && self.emits_block_like(e));
        if parens {
            self.e.w("(");
        }
        self.e.mark(e.span);
        self.expr_inner(e);
        if parens {
            self.e.w(")");
        }
    }

    fn expr_inner(&mut self, e: &Expr) {
        use ExprKind::*;
        match &e.kind {
            Int(s) | Float(s) | Char(s) | RawStr(s) => self.e.w(s),
            Bool(b) => self.e.w(if *b { "true" } else { "false" }),
            Str(parts) => self.e.w(&format!("\"{}\"", crate::lower::lit_parts(parts))),
            Template(parts) => {
                self.e.w("format!(");
                self.format_args(parts);
                self.e.w(")");
            }
            Null => self.e.w("None"),
            Path { segs, generics, global } => self.path_expr(segs, generics.as_ref(), *global),
            QualifiedPath(t) => {
                let s = self.ty(t, TyPos::Plain);
                self.e.w(&s);
            }
            SelfValue => {
                let n = self.class.as_ref().map(|c| c.self_name.clone()).unwrap_or_else(|| "self".into());
                self.e.w(&n);
            }
            Unary(op, x) => {
                let s = match op {
                    UnOp::Neg => "-",
                    UnOp::Not => "!",
                    UnOp::Deref => "*",
                    UnOp::Ref => "&",
                    UnOp::RefMut => "&mut ",
                };
                self.e.w(s);
                self.expr(x, P_UNARY);
            }
            Binary(op, l, r) => {
                if *op == BinOp::Add && self.is_string_concat(e) {
                    self.string_concat(e);
                    return;
                }
                let p = op.prec();
                let cmp = p == 5;
                let lp = if cmp { p + 1 } else { p };
                let lcast = matches!(l.kind, ExprKind::Cast(..)) && matches!(op, BinOp::Lt | BinOp::Shl);
                if lcast {
                    self.e.w("(");
                    self.expr(l, P_LOWEST);
                    self.e.w(")");
                } else {
                    self.expr(l, lp);
                }
                self.e.w(&format!(" {} ", op.as_str()));
                self.expr(r, p + 1);
            }
            Assign(op, target, value) => {
                let target_ty = self.assign_target_type(target);
                self.expr(target, P_LOWEST + 1);
                match op {
                    Some(o) => self.e.w(&format!(" {}= ", o.as_str())),
                    None => self.e.w(" = "),
                }
                match (op, target_ty) {
                    (None, Some(t)) => self.coerce(value, &t, TyPos::Local),
                    _ => self.expr(value, P_LOWEST + 1),
                }
            }
            Call { callee, args } => self.call(callee, args, e.span),
            MethodCall { .. } | Field { .. } | Index(..) if has_safe_link(e) => self.safe_chain(e),
            MethodCall { recv, name, generics, args, .. } => self.method_call(recv, name, generics.as_ref(), args, e.span),
            Field { recv, name, .. } => {
                if let Some((p, _)) = self.field_path(e) {
                    self.e.w(&p);
                    return;
                }
                self.expr(recv, P_POSTFIX);
                self.e.w(".");
                self.e.w(&member(name));
            }
            Index(r, i) => {
                self.expr(r, P_POSTFIX);
                self.e.w("[");
                self.expr(i, P_LOWEST);
                self.e.w("]");
            }
            Try(x) => {
                if let Some(label) = self.fns.last().and_then(|f| f.tries.last().cloned()) {
                    let v = self.fresh("v");
                    let err = self.fresh("e");
                    self.e.w("match ");
                    self.expr(x, P_LOWEST);
                    self.e.w(&format!(" {{ Ok({v}) => {v}, Err({err}) => break '{label} Err(::std::convert::From::from({err})) }}"));
                } else {
                    self.expr(x, P_POSTFIX);
                    self.e.w("?");
                }
            }
            Unwrap(x) => {
                self.expr(x, P_POSTFIX);
                self.e.w(".unwrap()");
            }
            Await(x) => {
                self.expr(x, P_POSTFIX);
                self.e.w(".await");
            }
            Cast(x, t) => {
                self.expr(x, P_CAST);
                let tt = self.ty(t, TyPos::Plain);
                self.e.w(&format!(" as {}", tt));
            }
            Elvis(l, r) => self.elvis(l, r),
            In { item, coll, negated } => {
                if *negated {
                    self.e.w("!");
                }
                self.expr(coll, P_POSTFIX);
                self.e.w(".contains(");
                if !is_string_lit(coll) {
                    self.e.w("&");
                    self.expr(item, P_UNARY);
                } else {
                    self.expr(item, P_LOWEST);
                }
                self.e.w(")");
            }
            Is { expr, pat, negated } => {
                if *negated {
                    self.e.w("!");
                }
                self.e.w("matches!(");
                self.expr(expr, P_LOWEST);
                let p = self.pat(pat);
                self.e.w(&format!(", {})", p));
            }
            Pair(a, b) => {
                self.e.w("(");
                self.expr(a, P_LOWEST);
                self.e.w(", ");
                self.expr(b, P_LOWEST);
                self.e.w(")");
            }
            If { cond, then, else_ } => {
                let t = Target::none();
                if else_.is_some() {
                    self.if_expr(cond, then, else_.as_deref(), Some(&t));
                } else {
                    self.if_expr(cond, then, None, None);
                }
            }
            When { subject, arms, rust_match } => {
                let t = Target::none();
                self.when_expr(subject.as_deref(), arms, *rust_match, Some(&t), e.span);
            }
            Block(b) => self.block_value(b, &Target::none()),
            Unsafe(b) => {
                self.e.w("unsafe ");
                self.block_value(b, &Target::none());
            }
            Async { is_move, block } => {
                self.e.w(if *is_move { "async move " } else { "async " });
                self.fns.push(FnCtx { throws: false, ret: None, is_lambda: true, tries: Vec::new() });
                let saved = std::mem::take(&mut self.loops);
                self.block_value(block, &Target::none());
                self.loops = saved;
                self.fns.pop();
            }
            Loop { label, body } => {
                if let Some(l) = label {
                    self.e.w(&format!("'{}: ", l));
                }
                self.e.w("loop ");
                self.loops.push(LoopCtx { dowhile_label: None });
                self.block_stmt(body);
                self.loops.pop();
            }
            While { label, cond, body } => {
                if let Some(l) = label {
                    self.e.w(&format!("'{}: ", l));
                }
                self.e.w("while ");
                self.push_scope();
                self.cond(cond);
                self.e.w(" ");
                self.loops.push(LoopCtx { dowhile_label: None });
                self.block_stmt(body);
                self.loops.pop();
                self.pop_scope();
            }
            DoWhile { label, body, cond } => {
                if let Some(l) = label {
                    self.e.w(&format!("'{}: ", l));
                }
                let needs_label = visit::has_own_continue(body);
                let lbl = if needs_label { Some(self.fresh("body")) } else { None };
                self.e.w("loop {");
                self.e.indent();
                self.e.nl();
                self.loops.push(LoopCtx { dowhile_label: lbl.clone() });
                if let Some(l) = &lbl {
                    self.e.w(&format!("'{}: ", l));
                }
                self.block_stmt(body);
                self.loops.pop();
                self.e.nl();
                self.e.w("if !(");
                self.expr(cond, P_LOWEST);
                self.e.w(") {");
                self.e.indent();
                self.e.nl();
                self.e.w("break;");
                self.e.dedent();
                self.e.nl();
                self.e.w("}");
                self.e.dedent();
                self.e.nl();
                self.e.w("}");
            }
            For { label, pat, iter, body } => {
                if let Some(l) = label {
                    self.e.w(&format!("'{}: ", l));
                }
                let p = self.pat(pat);
                self.e.w(&format!("for {} in ", p));
                let needs_parens = contains_struct_lit(iter);
                if needs_parens {
                    self.e.w("(");
                }
                self.expr(iter, P_LOWEST);
                if needs_parens {
                    self.e.w(")");
                }
                self.e.w(" ");
                self.push_scope();
                self.bind_pat(pat, None);
                self.loops.push(LoopCtx { dowhile_label: None });
                self.block_stmt(body);
                self.loops.pop();
                self.pop_scope();
            }
            Lambda { .. } => self.lambda(e, None, None),
            Return(v) => self.return_expr(v.as_deref()),
            Break { label, value } => {
                self.e.w("break");
                if let Some(l) = label {
                    self.e.w(&format!(" '{}", l));
                }
                if let Some(v) = value {
                    self.e.w(" ");
                    self.expr(v, P_LOWEST);
                }
            }
            Continue(label) => match label {
                Some(l) => self.e.w(&format!("continue '{}", l)),
                None => match self.loops.last().and_then(|l| l.dowhile_label.clone()) {
                    Some(l) => self.e.w(&format!("break '{}", l)),
                    None => self.e.w("continue"),
                },
            },
            Throw(x) => {
                if let Some(label) = self.fns.last().and_then(|f| f.tries.last().cloned()) {
                    self.e.w(&format!("break '{} Err(::std::convert::From::from(", label));
                    self.expr(x, P_LOWEST);
                    self.e.w("))");
                } else {
                    self.e.w("return Err(::std::convert::From::from(");
                    self.expr(x, P_LOWEST);
                    self.e.w("))");
                }
            }
            Tuple(v) => {
                self.e.w("(");
                for (i, x) in v.iter().enumerate() {
                    if i > 0 {
                        self.e.w(", ");
                    }
                    self.expr(x, P_LOWEST);
                }
                if v.len() == 1 {
                    self.e.w(",");
                }
                self.e.w(")");
            }
            Array(v) => {
                self.e.w("[");
                self.comma_list(v);
                self.e.w("]");
            }
            ArrayRepeat(x, n) => {
                self.e.w("[");
                self.expr(x, P_LOWEST);
                self.e.w("; ");
                self.expr(n, P_LOWEST);
                self.e.w("]");
            }
            VecLit(v) => {
                if v.len() == 1 {
                    if let ArrayRepeat(x, n) = &v[0].kind {
                        self.e.w("vec![");
                        self.expr(x, P_LOWEST);
                        self.e.w("; ");
                        self.expr(n, P_LOWEST);
                        self.e.w("]");
                        return;
                    }
                }
                self.e.w("vec![");
                self.comma_list(v);
                self.e.w("]");
            }
            MapLit(v) => {
                if v.is_empty() {
                    self.e.w("::std::collections::HashMap::new()");
                } else {
                    self.e.w("::std::collections::HashMap::from([");
                    self.comma_list(v);
                    self.e.w("])");
                }
            }
            SetLit(v) => {
                if v.is_empty() {
                    self.e.w("::std::collections::HashSet::new()");
                } else {
                    self.e.w("::std::collections::HashSet::from([");
                    self.comma_list(v);
                    self.e.w("])");
                }
            }
            Range { start, end, inclusive } => {
                if let Some(s) = start {
                    self.expr(s, P_RANGE + 1);
                }
                self.e.w(if *inclusive { "..=" } else { ".." });
                if let Some(x) = end {
                    self.expr(x, P_RANGE + 1);
                }
            }
            StructLit { path, generics, fields, base } => self.struct_lit(path, generics.as_ref(), fields, base.as_deref()),
            Macro { path, text } => {
                self.e.w(&format!("{}!", path));
                self.e.raw_lines(text);
            }
            BuiltinMacro { name, args } => self.macro_call(name, args, e.span),
            TryCatch { body, catch } => {
                let t = Target::none();
                self.try_catch(body, catch.as_deref(), Some(&t));
            }
            Paren(x) => self.expr(x, P_LOWEST),
            Raw(s) => self.e.raw_lines(s),
            Error => self.e.w("()"),
        }
    }

    fn comma_list(&mut self, v: &[Expr]) {
        for (i, x) in v.iter().enumerate() {
            if i > 0 {
                self.e.w(", ");
            }
            self.expr(x, P_LOWEST);
        }
    }

    fn assign_target_type(&self, target: &Expr) -> Option<Type> {
        match &target.kind {
            ExprKind::Path { .. } => {
                let n = target.ident()?;
                if let Some(t) = self.local(n) {
                    return t.clone();
                }
                self.class.as_ref().and_then(|c| if c.is_static { None } else { c.fields.get(n).cloned() })
            }
            ExprKind::Field { recv, name, .. } if matches!(recv.kind, ExprKind::SelfValue) => self.class.as_ref().and_then(|c| c.fields.get(name).cloned()),
            _ => None,
        }
        .filter(|t| *t != Type::Infer)
    }

    fn cond(&mut self, c: &Cond) {
        match c {
            Cond::Expr(e) => {
                let parens = contains_struct_lit(e);
                if parens {
                    self.e.w("(");
                }
                self.expr(e, P_LOWEST);
                if parens {
                    self.e.w(")");
                }
            }
            Cond::Let { pat, expr, implicit_some } => {
                let p = self.pat(pat);
                if *implicit_some {
                    self.e.w(&format!("let Some({}) = ", p));
                } else {
                    self.e.w(&format!("let {} = ", p));
                }
                self.expr(expr, P_LOWEST + 1);
                self.bind_pat(pat, None);
            }
        }
    }

    // ------------------------------------------------------------------ paths

    fn path_expr(&mut self, segs: &[String], generics: Option<&Vec<GenericArg>>, global: bool) {
        let mut s = String::new();
        if global {
            s.push_str("::");
        }
        if segs.len() == 1 {
            let n = &segs[0];
            if let Some(f) = self.implicit_field(n) {
                self.e.w(&f);
                return;
            }
            if !self.is_local(n) && !self.k.classes.contains_key(n) && !self.k.funs.contains_key(n) {
                if let Some(en) = self.k.variant_enum(n) {
                    s.push_str(&format!("{}::{}", en.name, n));
                    self.e.w(&s);
                    return;
                }
            }
            s.push_str(&esc(n));
        } else {
            s.push_str(&self.join_path(segs));
        }
        if let Some(g) = generics {
            s.push_str("::");
            s.push_str(&self.generic_args_text(g));
        }
        self.e.w(&s);
    }

    pub(crate) fn generic_args_text(&self, g: &[GenericArg]) -> String {
        let parts: Vec<String> = g
            .iter()
            .map(|a| match a {
                GenericArg::Type(t) => self.ty(t, TyPos::Generic),
                GenericArg::Lifetime(l) => format!("'{}", l),
                GenericArg::Binding(n, t) => format!("{} = {}", n, self.ty(t, TyPos::Generic)),
                GenericArg::Const(c) => c.clone(),
            })
            .collect();
        format!("<{}>", parts.join(", "))
    }

    /// Interprets `Type.member` / `module.item` chains as Rust paths.
    pub(crate) fn as_path(&self, e: &Expr) -> Option<(Vec<String>, PathKind)> {
        match &e.kind {
            ExprKind::Path { segs, generics: None, global: false } if segs.len() == 1 => {
                let n = &segs[0];
                if self.is_local(n) || self.implicit_field(n).is_some() {
                    return None;
                }
                if n == "Self" || crate::lower::PRIMITIVES.contains(&n.as_str()) {
                    return Some((segs.clone(), PathKind::Type));
                }
                if is_type_like(n) {
                    if self.k.variant_enum(n).is_some() && !self.k.is_user_type(n) {
                        return None;
                    }
                    return Some((segs.clone(), PathKind::Type));
                }
                if self.path_roots.contains(n) {
                    return Some((segs.clone(), PathKind::Module));
                }
                None
            }
            ExprKind::Path { segs, generics: None, .. } => {
                let last = segs.last().unwrap();
                let kind = if is_type_like(last) {
                    PathKind::Type
                } else if is_const_like(last) {
                    PathKind::Value
                } else {
                    PathKind::Module
                };
                Some((segs.clone(), kind))
            }
            ExprKind::Field { recv, name, safe: false } => {
                let (mut p, k) = self.as_path(recv)?;
                match k {
                    PathKind::Module => {
                        let kind = if is_type_like(name) {
                            PathKind::Type
                        } else if is_const_like(name) {
                            PathKind::Value
                        } else {
                            PathKind::Module
                        };
                        p.push(name.clone());
                        Some((p, kind))
                    }
                    PathKind::Type => {
                        p.push(name.clone());
                        Some((p, PathKind::Value))
                    }
                    PathKind::Value => None,
                }
            }
            _ => None,
        }
    }

    /// Path text for a field access that is really a path (`Color.Red`, `std.f64.consts.PI`).
    fn field_path(&self, e: &Expr) -> Option<(String, PathKind)> {
        if let ExprKind::Field { recv, .. } = &e.kind {
            if matches!(self.as_path(recv), Some((_, PathKind::Module)) | Some((_, PathKind::Type))) {
                let (p, k) = self.as_path(e)?;
                let mut segs = p;
                if segs[0] == "Self" {
                    // keep
                } else if segs.len() == 2 && self.k.enum_(&segs[0]).is_none() {
                    // fine
                }
                if segs.first().map(|s| s.as_str()) == Some("this") {
                    segs[0] = "self".into();
                }
                return Some((self.join_path(&segs), k));
            }
        }
        None
    }

    // ------------------------------------------------------------------ calls

    pub(crate) fn is_builtin_call(&self, name: &str) -> bool {
        !self.is_local(name) && !self.k.funs.contains_key(name) && self.class.as_ref().map_or(true, |c| !c.methods.contains_key(name))
    }

    fn call(&mut self, callee: &Expr, args: &[Arg], span: Span) {
        if let ExprKind::Path { segs, generics, global: false } = &callee.kind {
            if segs.len() == 1 {
                let name = segs[0].clone();
                if self.is_local(&name) {
                    self.e.w(&esc(&name));
                    self.arg_list(args, None, TyPos::Param, span);
                    return;
                }
                // calling a function-typed property
                if let Some(f) = self.implicit_field(&name) {
                    self.e.w(&format!("({})", f));
                    self.arg_list(args, None, TyPos::Param, span);
                    return;
                }
                if let Some(c) = self.class.as_ref().cloned() {
                    if let Some(m) = c.methods.get(&name) {
                        let static_ctx = c.is_static;
                        if m.recv == Recv::Static {
                            self.e.w(&format!("Self::{}", esc(&name)));
                        } else if !static_ctx {
                            self.e.w(&format!("{}.{}", c.self_name, esc(&name)));
                        } else {
                            self.e.w(&esc(&name));
                        }
                        let params = m.params.clone();
                        self.arg_list(args, Some(&params), TyPos::Param, span);
                        return;
                    }
                }
                if self.is_builtin_call(&name) {
                    if crate::lower::FORMAT_MACROS.iter().any(|(n, _)| *n == name) || crate::lower::OTHER_MACROS.contains(&name.as_str()) {
                        self.macro_call(&name, args, span);
                        return;
                    }
                    if self.builtin_fn(&name, args, span) {
                        return;
                    }
                }
                if let Some(cls) = self.k.class(&name).cloned() {
                    if !cls.is_tuple {
                        let g = generics.as_ref().map(|g| format!("::{}", self.generic_args_text(g))).unwrap_or_default();
                        self.e.w(&format!("{}{}::new", esc(&name), g));
                        let params = cls.ctor_params();
                        let pos = if cls.user_new { TyPos::Param } else { TyPos::Field };
                        self.arg_list(args, Some(&params), pos, span);
                        return;
                    }
                }
                if let Some(fi) = self.k.funs.get(&name).cloned() {
                    if !self.k.ambiguous.contains(&name) {
                        self.e.w(&esc(&name));
                        if let Some(g) = generics {
                            let gt = self.generic_args_text(g);
                            self.e.w(&format!("::{}", gt));
                        }
                        self.arg_list(args, Some(&fi.params), TyPos::Param, span);
                        return;
                    }
                }
                if !self.k.is_user_type(&name) {
                    if let Some(en) = self.k.variant_enum(&name).cloned() {
                        let shape = en.variant(&name).cloned().unwrap();
                        self.variant_ctor(&format!("{}::{}", en.name, name), &shape, args, span);
                        return;
                    }
                }
                if is_type_like(&name) && args.is_empty() && !matches!(name.as_str(), "Some" | "None" | "Ok" | "Err") {
                    let g = generics.as_ref().map(|g| format!("::{}", self.generic_args_text(g))).unwrap_or_default();
                    self.e.w(&format!("{}{}::new()", esc(&name), g));
                    return;
                }
            } else {
                self.path_call(segs, generics.as_ref(), args, span);
                return;
            }
        }
        self.expr(callee, P_POSTFIX);
        self.arg_list(args, None, TyPos::Param, span);
    }

    /// Call through a path: `Shape::Circle(1.0)`, `Point.origin()`, `std.mem.swap(a, b)`.
    fn path_call(&mut self, segs: &[String], generics: Option<&Vec<GenericArg>>, args: &[Arg], span: Span) {
        let n = segs.len();
        if n >= 2 {
            let owner = if segs[n - 2] == "Self" {
                self.class.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| "Self".into())
            } else {
                segs[n - 2].clone()
            };
            let last = &segs[n - 1];
            let path = self.join_path(segs);
            if let Some(en) = self.k.enum_(&owner).cloned() {
                if let Some(shape) = en.variant(last).cloned() {
                    self.variant_ctor(&path, &shape, args, span);
                    return;
                }
                if let Some(m) = en.methods.get(last).cloned() {
                    self.e.w(&path);
                    self.arg_list(args, Some(&m.params), TyPos::Param, span);
                    return;
                }
            }
            if let Some(c) = self.k.class(&owner).cloned() {
                if last == "new" && !c.user_new {
                    self.e.w(&path);
                    if let Some(g) = generics {
                        let gt = self.generic_args_text(g);
                        self.e.w(&format!("::{}", gt));
                    }
                    let params = c.ctor_params();
                    self.arg_list(args, Some(&params), TyPos::Field, span);
                    return;
                }
                if let Some(m) = c.methods.get(last).cloned() {
                    self.e.w(&path);
                    self.arg_list(args, Some(&m.params), TyPos::Param, span);
                    return;
                }
            }
        }
        let mut path = self.join_path(segs);
        if let Some(g) = generics {
            path.push_str("::");
            path.push_str(&self.generic_args_text(g));
        }
        self.e.w(&path);
        self.arg_list(args, None, TyPos::Param, span);
    }

    fn variant_ctor(&mut self, path: &str, shape: &VariantShape, args: &[Arg], span: Span) {
        match shape {
            VariantShape::Unit => {
                self.e.w(path);
                if !args.is_empty() {
                    self.error(span, "this enum variant has no fields");
                }
            }
            VariantShape::Tuple(_) => {
                self.e.w(path);
                self.arg_list(args, None, TyPos::Field, span);
            }
            VariantShape::Named(fields) => {
                let params: Vec<ParamInfo> = fields.iter().map(|(n, t)| ParamInfo { name: Some(n.clone()), ty: t.clone(), default: None }).collect();
                let slots = self.fill_args(args, &params, span);
                self.e.w(&format!("{} {{ ", path));
                for (i, (p, a)) in params.iter().zip(slots.iter()).enumerate() {
                    if i > 0 {
                        self.e.w(", ");
                    }
                    let name = p.name.clone().unwrap();
                    self.e.w(&format!("{}: ", esc(&name)));
                    match a {
                        Some(x) => self.coerce(x, &p.ty, TyPos::Field),
                        None => self.e.w("::std::default::Default::default()"),
                    }
                }
                self.e.w(" }");
            }
        }
    }

    /// Maps positional and named arguments onto parameters, filling defaults.
    fn fill_args(&mut self, args: &[Arg], params: &[ParamInfo], span: Span) -> Vec<Option<Expr>> {
        let mut slots: Vec<Option<Expr>> = vec![None; params.len()];
        let mut next = 0;
        for a in args {
            match &a.name {
                Some(n) => match params.iter().position(|p| p.name.as_deref() == Some(n.as_str())) {
                    Some(i) => slots[i] = Some(a.value.clone()),
                    None => self.error(a.value.span, format!("no parameter named `{}`", n)),
                },
                None => {
                    while next < slots.len() && slots[next].is_some() {
                        next += 1;
                    }
                    if next < slots.len() {
                        slots[next] = Some(a.value.clone());
                        next += 1;
                    } else {
                        self.error(a.value.span, format!("too many arguments: expected {}", params.len()));
                    }
                }
            }
        }
        for (i, p) in params.iter().enumerate() {
            if slots[i].is_none() {
                if let Some(d) = &p.default {
                    slots[i] = Some(d.clone());
                } else {
                    let n = p.name.clone().unwrap_or_else(|| format!("#{}", i + 1));
                    self.error(span, format!("missing argument `{}`", n));
                }
            }
        }
        slots
    }

    fn arg_list(&mut self, args: &[Arg], params: Option<&[ParamInfo]>, pos: TyPos, span: Span) {
        self.e.w("(");
        match params {
            Some(params) => {
                let slots = self.fill_args(args, params, span);
                let mut first = true;
                for (p, a) in params.iter().zip(slots.iter()) {
                    if let Some(a) = a {
                        if !first {
                            self.e.w(", ");
                        }
                        first = false;
                        self.coerce(a, &p.ty, pos);
                    }
                }
            }
            None => {
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        self.e.w(", ");
                    }
                    if let Some(n) = &a.name {
                        self.error(a.value.span, format!("named argument `{}` can only be used with functions declared in brs", n));
                    }
                    self.expr(&a.value, P_LOWEST);
                }
            }
        }
        self.e.w(")");
    }

    /// Kotlin-style helper functions. Returns false if `name` is not one.
    fn builtin_fn(&mut self, name: &str, args: &[Arg], _span: Span) -> bool {
        let values: Vec<Expr> = args.iter().map(|a| a.value.clone()).collect();
        match name {
            "listOf" | "mutableListOf" | "arrayListOf" | "vecOf" => {
                if values.is_empty() {
                    self.e.w("Vec::new()");
                } else {
                    self.e.w("vec![");
                    self.comma_list(&values);
                    self.e.w("]");
                }
                true
            }
            "arrayOf" => {
                self.e.w("[");
                self.comma_list(&values);
                self.e.w("]");
                true
            }
            "mapOf" | "mutableMapOf" | "hashMapOf" => {
                let e = Expr::new(ExprKind::MapLit(values), Span::default());
                self.expr_inner(&e);
                true
            }
            "setOf" | "mutableSetOf" | "hashSetOf" => {
                let e = Expr::new(ExprKind::SetLit(values), Span::default());
                self.expr_inner(&e);
                true
            }
            "run" if values.len() == 1 => {
                if let ExprKind::Lambda { body, .. } = &values[0].kind {
                    match &body.kind {
                        ExprKind::Block(b) => self.block_value(b, &Target::none()),
                        _ => {
                            self.e.w("{ ");
                            self.expr(body, P_LOWEST);
                            self.e.w(" }");
                        }
                    }
                    true
                } else {
                    false
                }
            }
            "repeat" if values.len() == 2 => {
                if let ExprKind::Lambda { params, body, .. } = &values[1].kind {
                    let var = match params.first() {
                        Some(p) => self.pat(&p.pat),
                        None => {
                            if visit::uses_it(body) {
                                "it".into()
                            } else {
                                "_".into()
                            }
                        }
                    };
                    self.e.w(&format!("for {} in 0..", var));
                    self.expr(&values[0], P_RANGE + 1);
                    self.e.w(" ");
                    self.push_scope();
                    self.bind_name(&var, None);
                    self.loops.push(LoopCtx { dowhile_label: None });
                    match &body.kind {
                        ExprKind::Block(b) => self.block_stmt(b),
                        _ => {
                            self.e.w("{");
                            self.e.indent();
                            self.e.nl();
                            let bl = self.stmt_expr(body);
                            if !bl {
                                self.e.w(";");
                            }
                            self.e.dedent();
                            self.e.nl();
                            self.e.w("}");
                        }
                    }
                    self.loops.pop();
                    self.pop_scope();
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// `println(...)` and friends.
    fn macro_call(&mut self, name: &str, args: &[Arg], span: Span) {
        let fmt_pos = crate::lower::FORMAT_MACROS.iter().find(|(n, _)| *n == name).map(|(_, p)| *p);
        if name == "vec" {
            self.e.w("vec![");
            let v: Vec<Expr> = args.iter().map(|a| a.value.clone()).collect();
            self.comma_list(&v);
            self.e.w("]");
            return;
        }
        self.e.w(&format!("{}!(", name));
        let Some(pos) = fmt_pos else {
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    self.e.w(", ");
                }
                self.expr(&a.value, P_LOWEST);
            }
            self.e.w(")");
            return;
        };
        let lead = pos.min(args.len());
        for (i, a) in args[..lead].iter().enumerate() {
            if i > 0 {
                self.e.w(", ");
            }
            self.expr(&a.value, P_LOWEST);
        }
        let rest = &args[lead..];
        if !rest.is_empty() {
            if lead > 0 {
                self.e.w(", ");
            }
            match &rest[0].value.kind {
                ExprKind::Template(parts) => {
                    self.format_args(parts);
                    for a in &rest[1..] {
                        self.e.w(", ");
                        self.expr(&a.value, P_LOWEST);
                    }
                }
                ExprKind::Str(parts) if rest.len() == 1 => {
                    self.e.w(&format!("\"{}\"", escape_braces(&crate::lower::lit_parts(parts))));
                }
                ExprKind::Str(_) | ExprKind::RawStr(_) => {
                    for (i, a) in rest.iter().enumerate() {
                        if i > 0 {
                            self.e.w(", ");
                        }
                        self.expr(&a.value, P_LOWEST);
                    }
                }
                _ => {
                    let sep = if matches!(name, "println" | "print" | "eprintln" | "eprint") { " " } else { "" };
                    let fmt = vec!["{}"; rest.len()].join(sep);
                    self.e.w(&format!("\"{}\"", fmt));
                    for a in rest {
                        self.e.w(", ");
                        self.expr(&a.value, P_LOWEST);
                    }
                }
            }
        }
        let _ = span;
        self.e.w(")");
    }

    /// `"text {} {name}", args` for a template.
    pub(crate) fn format_args(&mut self, parts: &[TemplatePart]) {
        let mut fmt = String::new();
        let mut args: Vec<&Expr> = Vec::new();
        for p in parts {
            match p {
                TemplatePart::Lit(s) => fmt.push_str(&escape_braces(s)),
                TemplatePart::Expr(e, spec) => {
                    let spec_s = spec.as_ref().map(|s| format!(":{}", s)).unwrap_or_default();
                    if let Some(n) = e.ident() {
                        if self.is_local(n) && !n.starts_with("r#") && esc(n) == n {
                            fmt.push_str(&format!("{{{}{}}}", n, spec_s));
                            continue;
                        }
                    }
                    fmt.push_str(&format!("{{{}}}", spec_s));
                    args.push(e);
                }
            }
        }
        self.e.w(&format!("\"{}\"", fmt));
        for a in args {
            self.e.w(", ");
            self.expr(a, P_LOWEST);
        }
    }

    fn method_call(&mut self, recv: &Expr, name: &str, generics: Option<&Vec<GenericArg>>, args: &[Arg], span: Span) {
        // Kotlin scope function: x.let { ... }
        if name == "let" && args.len() == 1 {
            if let ExprKind::Lambda { params, body, .. } = &args[0].value.kind {
                let var = match params.first() {
                    Some(p) => self.pat(&p.pat),
                    None => "it".to_string(),
                };
                self.e.w(&format!("{{ let {} = ", var));
                self.expr(recv, P_LOWEST);
                self.e.w("; ");
                self.push_scope();
                match params.first() {
                    Some(p) => self.bind_pat(&p.pat, None),
                    None => self.bind_name("it", None),
                }
                match &body.kind {
                    ExprKind::Block(b) => {
                        self.e.indent();
                        self.stmts_with_tail(&b.stmts, Some(&Target::none()));
                        self.e.dedent();
                        self.e.nl();
                        self.e.w("}");
                    }
                    _ => {
                        self.expr(body, P_LOWEST);
                        self.e.w(" }");
                    }
                }
                self.pop_scope();
                return;
            }
        }
        if let Some((mut p, kind)) = self.as_path(recv) {
            if matches!(kind, PathKind::Module | PathKind::Type) {
                p.push(name.to_string());
                self.path_call(&p, generics, args, span);
                return;
            }
        }
        // data class copy(field = value)
        if name == "copy" && !args.is_empty() && args.iter().all(|a| a.name.is_some()) {
            let v = self.fresh("copy");
            self.e.w(&format!("{{ let mut {} = ", v));
            self.expr(recv, P_POSTFIX);
            self.e.w(".clone();");
            for a in args {
                self.e.w(&format!(" {}.{} = ", v, esc(a.name.as_ref().unwrap())));
                self.expr(&a.value, P_LOWEST);
                self.e.w(";");
            }
            self.e.w(&format!(" {} }}", v));
            return;
        }
        // self.method(...) with known parameters / calling a function-typed field
        if matches!(recv.kind, ExprKind::SelfValue) {
            if let Some(c) = self.class.as_ref().cloned() {
                if let Some(m) = c.methods.get(name) {
                    self.expr(recv, P_POSTFIX);
                    self.e.w(&format!(".{}", esc(name)));
                    let params = m.params.clone();
                    self.arg_list(args, Some(&params), TyPos::Param, span);
                    return;
                }
                if matches!(c.fields.get(name), Some(Type::Fn { .. })) {
                    self.e.w("(");
                    self.expr(recv, P_POSTFIX);
                    self.e.w(&format!(".{})", esc(name)));
                    self.arg_list(args, None, TyPos::Param, span);
                    return;
                }
            }
        }
        self.expr(recv, P_POSTFIX);
        self.e.w(".");
        self.e.w(&member(name));
        if let Some(g) = generics {
            let gt = self.generic_args_text(g);
            self.e.w(&format!("::{}", gt));
        }
        self.arg_list(args, None, TyPos::Param, span);
    }

    // ------------------------------------------------------------------ safe navigation

    fn link_suffix(&mut self, link: &Expr) {
        match &link.kind {
            ExprKind::Field { name, .. } => {
                self.e.w(".");
                self.e.w(&member(name));
            }
            ExprKind::MethodCall { name, generics, args, .. } => {
                self.e.w(".");
                self.e.w(&member(name));
                if let Some(g) = generics {
                    let gt = self.generic_args_text(g);
                    self.e.w(&format!("::{}", gt));
                }
                self.arg_list(args, None, TyPos::Param, link.span);
            }
            ExprKind::Index(_, idx) => {
                self.e.w("[");
                self.expr(idx, P_LOWEST);
                self.e.w("]");
            }
            _ => {}
        }
    }

    fn split_segments<'e>(links: &[&'e Expr]) -> (Vec<&'e Expr>, Vec<Vec<&'e Expr>>) {
        let first = links.iter().position(|l| link_is_safe(l)).unwrap_or(links.len());
        let prefix = links[..first].to_vec();
        let mut segs: Vec<Vec<&Expr>> = Vec::new();
        for l in &links[first..] {
            if link_is_safe(l) {
                segs.push(vec![*l]);
            } else {
                segs.last_mut().unwrap().push(*l);
            }
        }
        (prefix, segs)
    }

    fn emit_chain_base(&mut self, base: &Expr, prefix: &[&Expr]) -> bool {
        let place = self.is_place(base) && prefix.iter().all(|l| matches!(l.kind, ExprKind::Field { .. } | ExprKind::Index(..)));
        self.expr(base, P_POSTFIX);
        for l in prefix {
            self.link_suffix(l);
        }
        place
    }

    fn let_lambda(seg: &[&Expr]) -> Option<Expr> {
        if seg.len() == 1 {
            if let ExprKind::MethodCall { name, args, .. } = &seg[0].kind {
                if name == "let" && args.len() == 1 && matches!(args[0].value.kind, ExprKind::Lambda { .. }) {
                    return Some(args[0].value.clone());
                }
            }
        }
        None
    }

    /// `a?.b.c?.d()` -> `a.as_ref().and_then(|v| v.b.c.as_ref()).map(|v| v.d())`
    fn safe_chain(&mut self, e: &Expr) {
        let (base, links) = chain_links(e);
        let (prefix, segs) = Self::split_segments(&links);
        let place = self.emit_chain_base(base, &prefix);
        if place {
            self.e.w(".as_ref()");
        }
        let n = segs.len();
        for (i, seg) in segs.iter().enumerate() {
            let last = i == n - 1;
            let method = if last { "map" } else { "and_then" };
            if let Some(lam) = Self::let_lambda(seg) {
                self.e.w(&format!(".{}(", method));
                self.lambda(&lam, None, None);
                self.e.w(")");
                continue;
            }
            let v = self.fresh("v");
            self.e.w(&format!(".{}(|{}| {}", method, v, v));
            for l in seg {
                self.link_suffix(l);
            }
            if !last && matches!(seg.last().unwrap().kind, ExprKind::Field { .. } | ExprKind::Index(..)) {
                self.e.w(".as_ref()");
            }
            self.e.w(")");
        }
    }

    /// Statement form: `if let Some(v) = &a { v.b(); }`. Returns false if not applicable.
    fn safe_chain_stmt(&mut self, e: &Expr) -> bool {
        let (base, links) = chain_links(e);
        let (prefix, segs) = Self::split_segments(&links);
        if segs.len() != 1 {
            return false;
        }
        let seg = &segs[0];
        let lam = Self::let_lambda(seg);
        let var = match &lam {
            Some(l) => match &l.kind {
                ExprKind::Lambda { params, .. } => params.first().map(|p| self.pat(&p.pat)).unwrap_or_else(|| "it".into()),
                _ => unreachable!(),
            },
            None => self.fresh("v"),
        };
        let place = self.is_place(base) && prefix.iter().all(|l| matches!(l.kind, ExprKind::Field { .. } | ExprKind::Index(..)));
        self.e.w(&format!("if let Some({}) = ", var));
        if place {
            self.e.w("&");
        }
        self.emit_chain_base(base, &prefix);
        self.e.w(" {");
        self.e.indent();
        self.push_scope();
        self.bind_name(&var, None);
        match lam {
            Some(l) => {
                if let ExprKind::Lambda { body, .. } = &l.kind {
                    match &body.kind {
                        ExprKind::Block(b) => self.stmts_with_tail(&b.stmts, None),
                        _ => {
                            self.e.nl();
                            let bl = self.stmt_expr(body);
                            if !bl {
                                self.e.w(";");
                            }
                        }
                    }
                }
            }
            None => {
                self.e.nl();
                self.e.w(&var);
                for l in seg {
                    self.link_suffix(l);
                }
                self.e.w(";");
            }
        }
        self.pop_scope();
        self.e.dedent();
        self.e.nl();
        self.e.w("}");
        true
    }

    // ------------------------------------------------------------------ operators

    fn is_string_concat(&self, e: &Expr) -> bool {
        let mut ops = Vec::new();
        concat_operands(e, &mut ops);
        ops.iter().any(|o| is_string_lit(o))
    }

    fn string_concat(&mut self, e: &Expr) {
        let mut nodes: Vec<&Expr> = Vec::new();
        let mut ops: Vec<&Expr> = Vec::new();
        spine(e, &mut ops, &mut nodes);
        let first = ops.iter().position(|o| is_string_lit(o)).unwrap();
        let mut fmt = String::new();
        let mut args: Vec<&Expr> = Vec::new();
        if first == 1 {
            fmt.push_str("{}");
            args.push(ops[0]);
        } else if first > 1 {
            fmt.push_str("{}");
            args.push(nodes[first - 2]);
        }
        let mut inline_args: Vec<(usize, Vec<TemplatePart>)> = Vec::new();
        let _ = &mut inline_args;
        for o in &ops[first..] {
            match &o.kind {
                ExprKind::Str(parts) => fmt.push_str(&escape_braces(&crate::lower::lit_parts(parts))),
                ExprKind::Template(parts) => {
                    for p in parts {
                        match p {
                            TemplatePart::Lit(s) => fmt.push_str(&escape_braces(s)),
                            TemplatePart::Expr(x, spec) => {
                                fmt.push_str(&format!("{{{}}}", spec.as_ref().map(|s| format!(":{}", s)).unwrap_or_default()));
                                args.push(x);
                            }
                        }
                    }
                }
                _ => {
                    fmt.push_str("{}");
                    args.push(o);
                }
            }
        }
        self.e.w(&format!("format!(\"{}\"", fmt));
        for a in args {
            self.e.w(", ");
            self.expr(a, P_LOWEST);
        }
        self.e.w(")");
    }

    fn elvis(&mut self, l: &Expr, r: &Expr) {
        if is_literal(r) {
            self.e.w(&format!("{}::ElvisOr::or_val(", RT));
            self.expr(l, P_LOWEST);
            self.e.w(", ");
            self.expr(r, P_LOWEST);
            self.e.w(")");
            return;
        }
        let v = self.fresh("v");
        self.e.w(&format!("match {}::Elvis::opt(", RT));
        self.expr(l, P_LOWEST);
        self.e.w(&format!(") {{ Some({v}) => {v}, None => "));
        self.expr(r, P_LOWEST);
        self.e.w(" }");
    }

    fn return_expr(&mut self, v: Option<&Expr>) {
        let ctx = self.fns.last().cloned();
        self.e.w("return");
        match ctx {
            Some(f) if !f.is_lambda => {
                let wrap = f.throws && !v.map_or(false, is_ok_err_call) && !v.map_or(false, |x| x.diverges());
                match v {
                    Some(x) => {
                        self.e.w(" ");
                        if wrap {
                            self.e.w("Ok(");
                        }
                        match &f.ret {
                            Some(t) => self.coerce(x, t, TyPos::Local),
                            None => self.expr(x, P_LOWEST),
                        }
                        if wrap {
                            self.e.w(")");
                        }
                    }
                    None => {
                        if f.throws {
                            self.e.w(" Ok(())");
                        }
                    }
                }
            }
            _ => {
                if let Some(x) = v {
                    self.e.w(" ");
                    match self.fns.last().and_then(|f| f.ret.clone()) {
                        Some(t) => self.coerce(x, &t, TyPos::Local),
                        None => self.expr(x, P_LOWEST),
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------ control flow

    /// `mode`: None = statement, Some(target) = value.
    fn if_expr(&mut self, cond: &Cond, then: &Block, else_: Option<&Expr>, mode: Option<&Target>) {
        self.e.w("if ");
        self.push_scope();
        self.cond(cond);
        self.e.w(" ");
        match mode {
            Some(t) => self.block_value(then, t),
            None => self.block_stmt(then),
        }
        self.pop_scope();
        if let Some(el) = else_ {
            self.e.w(" else ");
            match &el.kind {
                ExprKind::If { cond, then, else_ } => self.if_expr(cond, then, else_.as_deref(), mode),
                ExprKind::Block(b) => match mode {
                    Some(t) => self.block_value(b, t),
                    None => self.block_stmt(b),
                },
                _ => {
                    self.e.w("{");
                    self.e.indent();
                    self.e.nl();
                    self.e.mark(el.span);
                    match mode {
                        Some(t) => self.tail(el, t),
                        None => {
                            let bl = self.stmt_expr(el);
                            if !bl {
                                self.e.w(";");
                            }
                        }
                    }
                    self.e.dedent();
                    self.e.nl();
                    self.e.w("}");
                }
            }
        }
    }

    fn when_expr(&mut self, subject: Option<&Expr>, arms: &[WhenArm], rust_match: bool, mode: Option<&Target>, span: Span) {
        match subject {
            Some(s) => {
                self.e.w("match ");
                self.expr(s, P_LOWEST);
                self.e.w(" {");
                self.e.indent();
                let mut has_catch_all = false;
                let mut has_literal = false;
                for arm in arms {
                    self.e.nl();
                    self.e.mark(arm.span);
                    self.push_scope();
                    let has_in = arm.conds.iter().any(|c| matches!(c, WhenCond::In { .. }));
                    if has_in {
                        has_literal = true;
                        let w = self.fresh("w");
                        let mut checks = Vec::new();
                        for c in &arm.conds {
                            match c {
                                WhenCond::In { .. } => checks.push(c.clone()),
                                WhenCond::Pat(_) => checks.push(c.clone()),
                                _ => {}
                            }
                        }
                        self.e.w(&format!("ref {} if ", w));
                        for (i, c) in checks.iter().enumerate() {
                            if i > 0 {
                                self.e.w(" || ");
                            }
                            match c {
                                WhenCond::In { expr, negated } => {
                                    if *negated {
                                        self.e.w("!");
                                    }
                                    self.expr(expr, P_POSTFIX);
                                    self.e.w(&format!(".contains({})", w));
                                }
                                WhenCond::Pat(p) => {
                                    let ps = self.pat(p);
                                    self.e.w(&format!("matches!({}, {})", w, ps));
                                }
                                _ => {}
                            }
                        }
                        if let Some(g) = &arm.guard {
                            self.e.w(" && (");
                            self.expr(g, P_LOWEST);
                            self.e.w(")");
                        }
                    } else {
                        let pats: Vec<String> = arm
                            .conds
                            .iter()
                            .map(|c| match c {
                                WhenCond::Pat(p) => {
                                    if matches!(p, Pat::Lit(_) | Pat::Range { .. }) {
                                        has_literal = true;
                                    }
                                    if arm.guard.is_none() && matches!(p, Pat::Wild | Pat::Ident { sub: None, .. }) {
                                        has_catch_all = true;
                                    }
                                    self.pat(p)
                                }
                                WhenCond::Else => {
                                    has_catch_all = arm.guard.is_none() || has_catch_all;
                                    "_".into()
                                }
                                WhenCond::Expr(e) => {
                                    self.error(e.span, "expected a pattern");
                                    "_".into()
                                }
                                WhenCond::In { .. } => unreachable!(),
                            })
                            .collect();
                        self.e.w(&pats.join(" | "));
                        for c in &arm.conds {
                            if let WhenCond::Pat(p) = c {
                                self.bind_pat(p, None);
                            }
                        }
                        if let Some(g) = &arm.guard {
                            self.e.w(" if ");
                            self.expr(g, P_LOWEST);
                        }
                    }
                    self.e.w(" => ");
                    self.arm_body(&arm.body, mode);
                    self.pop_scope();
                }
                if mode.is_none() && !rust_match && has_literal && !has_catch_all {
                    self.e.nl();
                    self.e.w("_ => {}");
                }
                self.e.dedent();
                self.e.nl();
                self.e.w("}");
            }
            None => {
                // subject-less when: if / else if chain
                let mut first = true;
                let mut has_else = false;
                for arm in arms {
                    let is_else = arm.conds.iter().any(|c| matches!(c, WhenCond::Else));
                    if is_else {
                        has_else = true;
                        if first {
                            self.e.w("{");
                        } else {
                            self.e.w(" else {");
                        }
                    } else {
                        self.e.w(if first { "if " } else { " else if " });
                        for (i, c) in arm.conds.iter().enumerate() {
                            if i > 0 {
                                self.e.w(" || ");
                            }
                            match c {
                                WhenCond::Expr(e) => self.expr(e, if arm.conds.len() > 1 { 4 } else { P_LOWEST }),
                                WhenCond::Pat(p) => {
                                    let s = self.pat(p);
                                    self.e.w(&s);
                                }
                                _ => {}
                            }
                        }
                        self.e.w(" {");
                    }
                    first = false;
                    self.e.indent();
                    self.push_scope();
                    match &arm.body.kind {
                        ExprKind::Block(b) => match mode {
                            Some(t) => {
                                self.stmts_with_tail(&b.stmts, Some(t));
                            }
                            None => self.stmts_with_tail(&b.stmts, None),
                        },
                        _ => {
                            self.e.nl();
                            self.e.mark(arm.body.span);
                            match mode {
                                Some(t) => self.tail(&arm.body, t),
                                None => {
                                    let bl = self.stmt_expr(&arm.body);
                                    if !bl {
                                        self.e.w(";");
                                    }
                                }
                            }
                        }
                    }
                    self.pop_scope();
                    self.e.dedent();
                    self.e.nl();
                    self.e.w("}");
                    if is_else {
                        break;
                    }
                }
                if !has_else && mode.is_some() {
                    self.e.w(" else {\n    unreachable!(\"no `when` branch matched\")\n}");
                }
                let _ = span;
            }
        }
    }

    fn arm_body(&mut self, body: &Expr, mode: Option<&Target>) {
        match (&body.kind, mode) {
            (ExprKind::Block(b), Some(t)) => self.block_value(b, t),
            (ExprKind::Block(b), None) => self.block_stmt(b),
            (_, Some(t)) => {
                self.tail(body, t);
                self.e.w(",");
            }
            (_, None) => {
                if body.diverges() {
                    self.expr(body, P_LOWEST);
                    self.e.w(",");
                } else if body.is_block_like() {
                    self.stmt_expr(body);
                    self.e.w(",");
                } else {
                    self.e.w("{");
                    self.e.indent();
                    self.e.nl();
                    self.stmt_expr(body);
                    self.e.w(";");
                    self.e.dedent();
                    self.e.nl();
                    self.e.w("}");
                }
            }
        }
    }

    fn try_catch(&mut self, body: &Block, catch: Option<&Catch>, mode: Option<&Target>) {
        let label = self.fresh("try");
        let err_ty = match catch.and_then(|c| c.ty.as_ref()) {
            Some(t) => self.ty(t, TyPos::Generic),
            None => format!("{}::AnyError", RT),
        };
        let wrap = format!("Ok::<_, {}>", err_ty);
        let emit_block = |l: &mut Self, value: bool| {
            l.e.w(&format!("'{}: ", label));
            if let Some(f) = l.fns.last_mut() {
                f.tries.push(label.clone());
            }
            let t = Target { ty: None, wrap: Some(wrap.clone()) };
            if value {
                l.block_value(body, &t);
            } else {
                l.e.w("{");
                l.e.indent();
                l.push_scope();
                l.stmts_with_tail(&body.stmts, None);
                if !ends_diverging(&body.stmts) {
                    l.e.nl();
                    l.e.w(&format!("{}(())", wrap));
                }
                l.pop_scope();
                l.e.dedent();
                l.e.nl();
                l.e.w("}");
            }
            if let Some(f) = l.fns.last_mut() {
                f.tries.pop();
            }
        };
        match catch {
            None => {
                if mode.is_none() {
                    self.e.w("let _ = ");
                    emit_block(self, false);
                    self.e.w(";");
                } else {
                    emit_block(self, true);
                }
            }
            Some(c) => {
                self.e.w("match ");
                emit_block(self, mode.is_some());
                self.e.w(" {");
                self.e.indent();
                self.e.nl();
                let v = self.fresh("v");
                if mode.is_some() {
                    self.e.w(&format!("Ok({v}) => {v},"));
                } else {
                    self.e.w("Ok(()) => {}");
                }
                self.e.nl();
                self.push_scope();
                let bp = self.pat(&c.binding);
                self.bind_pat(&c.binding, c.ty.clone());
                self.e.w(&format!("Err({}) => ", bp));
                match mode {
                    Some(t) => self.block_value(&c.body, t),
                    None => self.block_stmt(&c.body),
                }
                self.pop_scope();
                self.e.dedent();
                self.e.nl();
                self.e.w("}");
            }
        }
    }

    // ------------------------------------------------------------------ lambdas & literals

    pub(crate) fn lambda(&mut self, e: &Expr, param_tys: Option<&[Type]>, ret_ty: Option<&Type>) {
        let ExprKind::Lambda { params, ret, body, is_move, is_async, .. } = &e.kind else {
            self.expr(e, P_LOWEST);
            return;
        };
        let implicit_it = params.is_empty() && visit::uses_it(body);
        if *is_async {
            self.e.w("async ");
        }
        if *is_move {
            self.e.w("move ");
        }
        self.push_scope();
        let mut ps = Vec::new();
        if implicit_it {
            let t = param_tys.and_then(|p| p.first()).map(|t| format!(": {}", self.ty(t, TyPos::Local))).unwrap_or_default();
            ps.push(format!("it{}", t));
            self.bind_name("it", param_tys.and_then(|p| p.first()).cloned());
        }
        for (i, p) in params.iter().enumerate() {
            let pt = self.pat(&p.pat);
            let ty = p.ty.clone().or_else(|| param_tys.and_then(|v| v.get(i)).cloned());
            match &ty {
                Some(t) => {
                    let tt = self.ty(t, TyPos::Local);
                    ps.push(format!("{}: {}", pt, tt));
                }
                None => ps.push(pt),
            }
            self.bind_pat(&p.pat, ty);
        }
        self.e.w(&format!("|{}|", ps.join(", ")));
        let ret = ret.clone().or_else(|| ret_ty.cloned().filter(|t| !t.is_unit()));
        let explicit_ret = matches!(&e.kind, ExprKind::Lambda { ret: Some(_), .. });
        self.fns.push(FnCtx { throws: false, ret: ret.clone(), is_lambda: true, tries: Vec::new() });
        let saved_loops = std::mem::take(&mut self.loops);
        let target = Target { ty: ret.clone(), wrap: None };
        if explicit_ret {
            let rt = self.ty(ret.as_ref().unwrap(), TyPos::Ret);
            self.e.w(&format!(" -> {} ", rt));
            match &body.kind {
                ExprKind::Block(b) => self.block_value(b, &target),
                _ => {
                    self.e.w("{ ");
                    self.tail(body, &target);
                    self.e.w(" }");
                }
            }
        } else {
            self.e.w(" ");
            match &body.kind {
                ExprKind::Block(b) => self.block_value(b, &target),
                _ => {
                    if ret.is_some() {
                        self.tail(body, &target);
                    } else {
                        self.expr(body, P_LOWEST);
                    }
                }
            }
        }
        self.loops = saved_loops;
        self.fns.pop();
        self.pop_scope();
    }

    fn struct_lit(&mut self, path: &[String], generics: Option<&Vec<GenericArg>>, fields: &[(String, Option<Expr>)], base: Option<&Expr>) {
        let owner = if path.len() == 1 && path[0] == "Self" { self.class.as_ref().map(|c| c.name.clone()) } else { path.last().cloned() };
        let (p, shape) = self.resolve_variant_path(path);
        let field_types: Vec<(String, Type)> = match shape {
            Some(VariantShape::Named(f)) => f,
            _ => owner
                .as_ref()
                .and_then(|o| self.k.class(o))
                .map(|c| c.fields.iter().map(|f| (f.name.clone(), f.ty.clone())).collect())
                .unwrap_or_default(),
        };
        let mut text = p;
        if let Some(g) = generics {
            text.push_str("::");
            text.push_str(&self.generic_args_text(g));
        }
        self.e.w(&format!("{} {{ ", text));
        for (i, (n, v)) in fields.iter().enumerate() {
            if i > 0 {
                self.e.w(", ");
            }
            let fty = field_types.iter().find(|(fname, _)| fname == n).map(|(_, t)| t.clone());
            match v {
                Some(v) => {
                    self.e.w(&format!("{}: ", esc(n)));
                    match fty {
                        Some(t) => self.coerce(v, &t, TyPos::Field),
                        None => self.expr(v, P_LOWEST),
                    }
                }
                None => match self.implicit_field(n) {
                    Some(f) => self.e.w(&format!("{}: {}", esc(n), f)),
                    None => self.e.w(&esc(n)),
                },
            }
        }
        if let Some(b) = base {
            if !fields.is_empty() {
                self.e.w(", ");
            }
            self.e.w("..");
            self.expr(b, P_LOWEST);
        }
        self.e.w(" }");
    }
}

fn member(name: &str) -> String {
    if name.chars().all(|c| c.is_ascii_digit()) {
        name.to_string()
    } else {
        esc(name)
    }
}

fn contains_struct_lit(e: &Expr) -> bool {
    let mut found = false;
    visit::walk_expr(e, &mut |x| {
        match &x.kind {
            ExprKind::StructLit { .. } => found = true,
            ExprKind::Call { .. } | ExprKind::MethodCall { .. } | ExprKind::Lambda { .. } | ExprKind::Paren(_) | ExprKind::Index(..) => {
                return matches!(x.kind, ExprKind::MethodCall { .. }) && !found;
            }
            _ => {}
        }
        !found
    });
    found
}

fn concat_operands<'e>(e: &'e Expr, out: &mut Vec<&'e Expr>) {
    match &e.kind {
        ExprKind::Binary(BinOp::Add, l, r) => {
            concat_operands(l, out);
            out.push(r);
        }
        _ => out.push(e),
    }
}

/// Left spine of a `+` chain: operands, and the Add nodes covering operands[0..=k+1].
fn spine<'e>(e: &'e Expr, ops: &mut Vec<&'e Expr>, nodes: &mut Vec<&'e Expr>) {
    match &e.kind {
        ExprKind::Binary(BinOp::Add, l, r) => {
            spine(l, ops, nodes);
            ops.push(r);
            nodes.push(e);
        }
        _ => ops.push(e),
    }
}
