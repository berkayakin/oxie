//! Lightweight type inference used for `fun f() = expr` without a declared return type.
//! It only answers when the type is obvious from the syntax and known declarations.

use crate::ast::*;
use crate::lower::{int_suffix, Lowerer};
use crate::parser::is_type_like;

fn ty(name: &str) -> Type {
    Type::simple(name)
}

fn static_str() -> Type {
    Type::Ref { lifetime: Some("static".into()), mutable: false, inner: Box::new(ty("str")) }
}

impl<'a> Lowerer<'a> {
    pub(crate) fn infer_type(&self, e: &Expr) -> Option<Type> {
        use ExprKind::*;
        match &e.kind {
            Int(s) => Some(ty(int_suffix(s).unwrap_or("i32"))),
            Float(s) => Some(ty(if s.ends_with("f32") { "f32" } else { "f64" })),
            Bool(_) | Is { .. } | In { .. } => Some(ty("bool")),
            Char(s) => Some(ty(if s.starts_with('b') { "u8" } else { "char" })),
            Str(_) | RawStr(_) => Some(static_str()),
            Template(_) => Some(ty("String")),
            Path { .. } => {
                let n = e.ident()?;
                if let Some(t) = self.local(n) {
                    return t.clone().filter(|t| *t != Type::Infer);
                }
                if self.implicit_field(n).is_some() {
                    return self.class.as_ref()?.fields.get(n).cloned().filter(|t| *t != Type::Infer);
                }
                None
            }
            SelfValue => Some(ty("Self")),
            Binary(op, l, r) => match op {
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::And | BinOp::Or => Some(ty("bool")),
                BinOp::Add if matches!(l.kind, Str(_) | Template(_)) || matches!(r.kind, Str(_) | Template(_)) => Some(ty("String")),
                _ => self.infer_type(l).filter(|t| !matches!(t, Type::Ref { .. })).or_else(|| self.infer_type(r)),
            },
            Unary(UnOp::Neg, x) | Unary(UnOp::Not, x) => self.infer_type(x),
            Unary(UnOp::Ref, x) => self.infer_type(x).map(|t| Type::Ref { lifetime: None, mutable: false, inner: Box::new(t) }),
            Unary(UnOp::Deref, x) => match self.infer_type(x)? {
                Type::Ref { inner, .. } => Some(*inner),
                _ => None,
            },
            Cast(_, t) => Some(t.clone()),
            Paren(x) => self.infer_type(x),
            Elvis(_, r) => self.infer_type(r).map(|t| if t == static_str() { ty("String") } else { t }),
            Call { callee, .. } => {
                let n = callee.ident()?;
                if self.is_local(n) {
                    return None;
                }
                if let Some(c) = &self.class {
                    if let Some(m) = c.methods.get(n) {
                        return if m.throws { None } else { m.ret.clone() };
                    }
                }
                if let Some(c) = self.k.class(n) {
                    return if c.has_generics { None } else { Some(ty(n)) };
                }
                if let Some(f) = self.k.funs.get(n) {
                    return if f.throws { None } else { f.ret.clone() };
                }
                if let Some(en) = self.k.variant_enum_in(n, Some(&self.module)) {
                    return Some(ty(&en.name));
                }
                None
            }
            MethodCall { recv, name, .. } => {
                if matches!(recv.kind, SelfValue) {
                    if let Some(m) = self.class.as_ref().and_then(|c| c.methods.get(name)) {
                        return if m.throws { None } else { m.ret.clone() };
                    }
                }
                if let Some((p, _)) = self.as_path(recv) {
                    let owner = p.last()?;
                    let owner = if owner == "Self" { self.class.as_ref()?.name.clone() } else { owner.clone() };
                    if let Some(c) = self.k.class(&owner) {
                        if name == "new" && !c.user_new {
                            return if c.has_generics { None } else { Some(ty(&owner)) };
                        }
                        return c.methods.get(name).and_then(|m| if m.throws { None } else { m.ret.clone() });
                    }
                    if let Some(en) = self.k.enum_(&owner) {
                        if en.variant(name).is_some() {
                            return Some(ty(&owner));
                        }
                        return en.methods.get(name).and_then(|m| m.ret.clone());
                    }
                    return None;
                }
                match name.as_str() {
                    "len" | "count" | "capacity" => Some(ty("usize")),
                    "to_string" | "to_uppercase" | "to_lowercase" | "to_owned" | "repeat" | "join" | "concat" | "replace" | "trim_string" => Some(ty("String")),
                    "is_empty" | "contains" | "contains_key" | "starts_with" | "ends_with" | "is_some" | "is_none" | "is_ok" | "is_err" | "any" | "all"
                    | "is_alphabetic" | "is_numeric" | "is_alphanumeric" | "is_whitespace" | "is_digit" | "eq" | "ne" | "is_nan" => Some(ty("bool")),
                    "clone" | "abs" | "sqrt" | "sin" | "cos" | "tan" | "powi" | "powf" | "floor" | "ceil" | "round" | "min" | "max" | "exp" | "ln"
                    | "log10" | "signum" | "pow" | "wrapping_add" | "wrapping_sub" | "wrapping_mul" | "saturating_sub" | "saturating_add" | "clamp" => {
                        self.infer_type(recv).filter(|t| !matches!(t, Type::Ref { .. }))
                    }
                    _ => None,
                }
            }
            Field { recv, name, safe: false } => {
                if matches!(recv.kind, SelfValue) {
                    return self.class.as_ref()?.fields.get(name).cloned();
                }
                let rt = self.infer_type(recv)?;
                let tn = match &rt {
                    Type::Ref { inner, .. } => inner.single_name()?.to_string(),
                    t => t.single_name()?.to_string(),
                };
                let c = self.k.class(&tn)?;
                c.fields.iter().find(|f| f.name == *name).map(|f| f.ty.clone())
            }
            If { then, else_, .. } => self.infer_block(then).or_else(|| else_.as_ref().and_then(|e| self.infer_type(e))),
            When { arms, .. } => arms.iter().find_map(|a| if a.body.diverges() { None } else { self.infer_type(&a.body) }),
            Block(b) => self.infer_block(b),
            StructLit { path, generics: None, .. } => {
                let n = path.last()?;
                if path.len() == 1 && is_type_like(n) && self.k.class(n).map_or(false, |c| !c.has_generics) {
                    Some(ty(n))
                } else {
                    None
                }
            }
            Tuple(items) => items.iter().map(|x| self.infer_type(x)).collect::<Option<Vec<_>>>().map(Type::Tuple),
            VecLit(items) if !items.is_empty() => {
                let t = self.infer_type(&items[0])?;
                Some(Type::Path { global: false, segs: vec![PathSeg { name: "Vec".into(), generics: Some(vec![GenericArg::Type(t)]), fn_sugar: None }], span: Default::default() })
            }
            _ => None,
        }
    }

    fn infer_block(&self, b: &Block) -> Option<Type> {
        match b.stmts.last()? {
            Stmt::Expr { expr, semi: false } => self.infer_type(expr),
            _ => None,
        }
    }

    /// Returns the declaration with an inferred return type when it has none.
    pub(crate) fn with_inferred_ret(&mut self, f: &FunDecl, trait_ret: Option<Type>) -> FunDecl {
        if f.ret.is_some() {
            return f.clone();
        }
        let mut f2 = f.clone();
        if let Some(t) = trait_ret {
            f2.ret = Some(t);
            return f2;
        }
        if let Some(FunBody::Expr(e)) = &f.body {
            self.push_scope();
            for p in &f.params {
                self.bind_pat(&p.pat, Some(p.ty.clone()));
            }
            f2.ret = self.infer_type(e);
            self.pop_scope();
        }
        f2
    }
}
