//! Crate-wide symbol table and method receiver inference.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::parser::is_type_like;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recv {
    Static,
    Ref,
    RefMut,
    Value,
}

#[derive(Clone, Debug)]
pub struct ParamInfo {
    pub name: Option<String>,
    pub ty: Type,
    pub default: Option<Expr>,
    /// Generic parameter names of the declaring function (lambda arguments can't be annotated with them).
    pub generics: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct MethodInfo {
    pub recv: Recv,
    pub params: Vec<ParamInfo>,
    pub throws: bool,
    pub ret: Option<Type>,
    /// The receiver was written in the source (`self`, `mut fun`, `static fun`, ...).
    pub explicit: bool,
}

#[derive(Clone, Debug)]
pub struct FieldInfo {
    pub name: String,
    pub ty: Type,
    pub default: Option<Expr>,
    pub in_ctor: bool,
}

#[derive(Clone, Debug)]
pub struct ClassInfo {
    pub name: String,
    pub module: Vec<String>,
    pub fields: Vec<FieldInfo>,
    pub methods: HashMap<String, MethodInfo>,
    pub user_new: bool,
    pub is_data: bool,
    pub is_tuple: bool,
    /// `union` (constructed with a struct literal, no generated constructor)
    pub is_union: bool,
    /// No fields at all: the type name is also its only value (`struct Marker`, `class Dog : Animal`)
    pub is_unit: bool,
    /// Type or const parameters (a class with only lifetimes is used without arguments).
    pub has_generics: bool,
    /// Number of type parameters (lifetimes and consts excluded).
    pub type_params: usize,
    /// `shared class`: a handle to `{Name}Data` (`sync`: `Arc<Mutex<..>>` instead of `Rc<RefCell<..>>`).
    pub is_shared: bool,
    pub is_sync: bool,
    /// Number of lifetime parameters (`data class Token<'a>`).
    pub lifetimes: usize,
}

impl ClassInfo {
    /// Constructor parameters in order.
    pub fn ctor_params(&self) -> Vec<ParamInfo> {
        if self.user_new {
            if let Some(m) = self.methods.get("new") {
                return m.params.clone();
            }
        }
        self.fields
            .iter()
            .filter(|f| f.in_ctor)
            .map(|f| ParamInfo { name: Some(f.name.clone()), ty: f.ty.clone(), default: f.default.clone(), generics: Vec::new() })
            .collect()
    }
}

#[derive(Clone, Debug)]
pub enum VariantShape {
    Unit,
    Tuple(usize),
    Named(Vec<(String, Type)>),
}

#[derive(Clone, Debug)]
pub struct EnumInfo {
    pub name: String,
    pub module: Vec<String>,
    pub variants: Vec<(String, VariantShape)>,
    pub methods: HashMap<String, MethodInfo>,
    /// Number of lifetime parameters (`enum Tok<'a>`).
    pub lifetimes: usize,
    pub has_type_params: bool,
}

impl EnumInfo {
    pub fn variant(&self, name: &str) -> Option<&VariantShape> {
        self.variants.iter().find(|(n, _)| n == name).map(|(_, s)| s)
    }
}

#[derive(Clone, Debug)]
pub struct TraitInfo {
    pub name: String,
    pub module: Vec<String>,
    pub methods: HashMap<String, MethodInfo>,
    pub assoc_types: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct FunInfo {
    pub module: Vec<String>,
    pub params: Vec<ParamInfo>,
    pub throws: bool,
    pub ret: Option<Type>,
}

#[derive(Clone, Debug, Default)]
pub struct Crate {
    pub crate_name: String,
    pub classes: HashMap<String, ClassInfo>,
    pub enums: HashMap<String, EnumInfo>,
    pub traits: HashMap<String, TraitInfo>,
    pub funs: HashMap<String, FunInfo>,
    pub ambiguous: HashSet<String>,
    /// Top-level module names (children of the crate root).
    pub modules: HashSet<String>,
    /// External crate names from Cargo.toml.
    pub extern_crates: HashSet<String>,
    /// (module path, trait name) for every extension-function trait.
    pub ext_traits: Vec<(Vec<String>, String)>,
    pub variant_owner: HashMap<String, Vec<String>>,
    /// Whether a `lib.oxi` exists (main.oxi then imports modules through the crate name).
    pub has_lib: bool,
    /// Automatic derives per type (data classes and enums).
    pub derives: HashMap<String, Vec<String>>,
    /// Types declared inside a class body (hoisted to the module): outer name -> inner names.
    pub nested: HashMap<String, Vec<String>>,
}

pub const STATIC_METHOD_NAMES: &[&str] = &["new", "default", "from", "from_str", "try_from", "from_iter"];

/// Methods that take `&mut self` on common std types (used to infer `mut fun`).
pub const MUTATING_METHODS: &[&str] = &[
    "push", "push_str", "push_back", "push_front", "pop", "pop_back", "pop_front", "insert", "remove", "clear", "extend",
    "extend_from_slice", "append", "truncate", "retain", "retain_mut", "drain", "sort", "sort_by", "sort_by_key",
    "sort_unstable", "sort_unstable_by", "sort_unstable_by_key", "dedup", "dedup_by_key", "reverse", "swap", "resize",
    "entry", "get_mut", "iter_mut", "values_mut", "last_mut", "first_mut", "as_mut", "as_mut_slice", "take", "replace",
    "set", "fill", "rotate_left", "rotate_right", "split_off", "shrink_to_fit", "reserve", "get_or_insert",
    "get_or_insert_with", "write", "write_all", "write_str", "write_fmt", "flush", "read", "read_line",
    "read_to_string", "read_to_end", "read_exact", "next", "next_back", "or_insert", "swap_remove", "push_all",
    "borrow_mut", "chunks_mut", "split_at_mut", "nth", "advance_by", "make_ascii_uppercase", "make_ascii_lowercase",
];

impl Crate {
    pub fn class(&self, name: &str) -> Option<&ClassInfo> {
        if self.ambiguous.contains(name) {
            return None;
        }
        self.classes.get(name)
    }
    pub fn enum_(&self, name: &str) -> Option<&EnumInfo> {
        if self.ambiguous.contains(name) {
            return None;
        }
        self.enums.get(name)
    }
    /// Lifetime parameters of an Oxie class or enum whose only generics are lifetimes (`Token<'a>`),
    /// so that a bare `Token` in a signature can be completed.
    pub fn type_lifetimes(&self, name: &str) -> usize {
        if let Some(c) = self.class(name) {
            return if c.type_params == 0 { c.lifetimes } else { 0 };
        }
        match self.enum_(name) {
            Some(e) if !e.has_type_params => e.lifetimes,
            _ => 0,
        }
    }
    /// The enum owning a variant, if the variant name is unique among Oxie enums.
    pub fn variant_enum(&self, variant: &str) -> Option<&EnumInfo> {
        self.variant_enum_in(variant, None)
    }

    /// Like `variant_enum`, but when several enums have the variant, prefers the one declared in `module`.
    pub fn variant_enum_in(&self, variant: &str, module: Option<&[String]>) -> Option<&EnumInfo> {
        if matches!(variant, "Some" | "None" | "Ok" | "Err") {
            return None;
        }
        match self.variant_owner.get(variant) {
            Some(v) if v.len() == 1 => self.enum_(&v[0]),
            Some(v) => {
                let m = module?;
                let local: Vec<&EnumInfo> = v.iter().filter_map(|n| self.enum_(n)).filter(|e| e.module == m).collect();
                if local.len() == 1 {
                    Some(local[0])
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    pub fn is_trait(&self, name: &str) -> bool {
        self.traits.contains_key(name)
    }
    pub fn is_user_type(&self, name: &str) -> bool {
        self.classes.contains_key(name) || self.enums.contains_key(name) || self.traits.contains_key(name)
    }
}

// -------------------------------------------------------------------------------------------
// Collection

pub struct SourceUnit<'a> {
    pub module: Vec<String>,
    pub file: &'a File,
}

pub fn collect(units: &[SourceUnit], crate_name: &str, extern_crates: HashSet<String>, has_lib: bool) -> Crate {
    let mut k = Crate { crate_name: crate_name.to_string(), extern_crates, has_lib, ..Default::default() };
    for u in units {
        if let Some(first) = u.module.first() {
            k.modules.insert(first.clone());
        }
    }
    // pass 1: traits (needed for impl receivers)
    for u in units {
        collect_traits(&mut k, &u.module, &u.file.items);
    }
    // pass 2: types and functions
    for u in units {
        collect_items(&mut k, &u.module, &u.file.items);
    }
    // pass 3: inherent impl blocks add methods to classes
    for u in units {
        collect_impls(&mut k, &u.file.items);
    }
    // pass 4: receiver inference (fixpoint over mutating methods)
    infer_receivers(&mut k, units);
    compute_derives(&mut k, units);
    k
}

/// Generics made only of lifetimes (or none), which don't stand in the way of derives.
fn only_lifetimes(g: &Generics) -> bool {
    g.params.iter().all(|p| matches!(p, GenericParam::Lifetime { .. }))
}

/// A path segment without type arguments (`Token` or `Token<'a>`).
fn no_type_args(seg: &PathSeg) -> bool {
    seg.generics.as_ref().is_none_or(|g| g.iter().all(|a| matches!(a, GenericArg::Lifetime(_))))
}

/// Classes and enums whose fields hold references get a lifetime parameter: `class Parser(input: &str)`
/// becomes `struct Parser<'a> { input: &'a str }`. A field naming such a type (`Vec<Token>`) needs
/// one too, so this runs over the whole crate before symbols are collected.
pub fn auto_type_lifetimes(files: &mut [&mut File]) {
    fn gather<'a>(items: &'a [Item], out: &mut Vec<&'a TypeDecl>) {
        for it in items {
            match &it.kind {
                ItemKind::Type(t) => {
                    out.push(t);
                    gather(&t.members, out);
                }
                ItemKind::Mod { items, .. } => gather(items, out),
                _ => {}
            }
        }
    }
    fn needs(t: &Type, lt: &HashMap<String, usize>) -> bool {
        match t {
            Type::Ref { lifetime: None, .. } => true,
            Type::Ref { inner, .. } | Type::Slice(inner) | Type::Array(inner, _) | Type::Option(inner) => needs(inner, lt),
            Type::Tuple(v) => v.iter().any(|t| needs(t, lt)),
            Type::Path { segs, .. } => segs.iter().any(|s| match &s.generics {
                None => segs.len() == 1 && lt.contains_key(&s.name),
                Some(g) => g.iter().any(|a| matches!(a, GenericArg::Type(t) if needs(t, lt))),
            }),
            _ => false,
        }
    }
    fn fill(t: &mut Type, lt: &HashMap<String, usize>) {
        match t {
            Type::Ref { lifetime, inner, .. } => {
                if lifetime.is_none() {
                    *lifetime = Some("a".into());
                }
                fill(inner, lt);
            }
            Type::Slice(inner) | Type::Array(inner, _) | Type::Option(inner) => fill(inner, lt),
            Type::Tuple(v) => v.iter_mut().for_each(|t| fill(t, lt)),
            Type::Path { segs, .. } => {
                let single = segs.len() == 1;
                for s in segs {
                    match &mut s.generics {
                        None if single => {
                            if let Some(&n) = lt.get(&s.name) {
                                s.generics = Some(vec![GenericArg::Lifetime("a".into()); n]);
                            }
                        }
                        Some(g) => {
                            for a in g {
                                if let GenericArg::Type(t) = a {
                                    fill(t, lt);
                                }
                            }
                        }
                        None => {}
                    }
                }
            }
            _ => {}
        }
    }
    fn field_types(t: &mut TypeDecl) -> Vec<&mut Type> {
        let mut v: Vec<&mut Type> = t.fields.iter_mut().map(|f| &mut f.ty).collect();
        v.extend(t.tuple_fields.iter_mut().map(|(_, ty)| ty));
        for var in &mut t.variants {
            match &mut var.fields {
                VariantFields::Unit => {}
                VariantFields::Tuple(ts) => v.extend(ts.iter_mut()),
                VariantFields::Named(fs) => v.extend(fs.iter_mut().map(|f| &mut f.ty)),
            }
        }
        v
    }
    fn apply(items: &mut [Item], auto: &HashSet<String>, lt: &HashMap<String, usize>) {
        for it in items {
            match &mut it.kind {
                ItemKind::Type(t) => {
                    if t.generics.is_empty() && auto.contains(&t.name) {
                        t.generics.params.push(GenericParam::Lifetime { name: "a".into(), bounds: Vec::new() });
                        for ty in field_types(t) {
                            fill(ty, lt);
                        }
                    }
                    apply(&mut t.members, auto, lt);
                }
                ItemKind::Mod { items, .. } => apply(items, auto, lt),
                _ => {}
            }
        }
    }

    let mut lt: HashMap<String, usize> = HashMap::new();
    let mut auto: HashSet<String> = HashSet::new();
    {
        let mut decls = Vec::new();
        for f in files.iter() {
            gather(&f.items, &mut decls);
        }
        for t in &decls {
            let n = t.generics.params.iter().filter(|g| matches!(g, GenericParam::Lifetime { .. })).count();
            if n > 0 && only_lifetimes(&t.generics) {
                lt.insert(t.name.clone(), n);
            }
        }
        let has_ref = |t: &TypeDecl, lt: &HashMap<String, usize>| {
            t.fields.iter().map(|f| &f.ty).chain(t.tuple_fields.iter().map(|(_, ty)| ty)).any(|ty| needs(ty, lt))
                || t.variants.iter().any(|v| match &v.fields {
                    VariantFields::Unit => false,
                    VariantFields::Tuple(ts) => ts.iter().any(|ty| needs(ty, lt)),
                    VariantFields::Named(fs) => fs.iter().any(|f| needs(&f.ty, lt)),
                })
        };
        loop {
            let mut changed = false;
            for t in &decls {
                if t.generics.is_empty() && !auto.contains(&t.name) && has_ref(t, &lt) {
                    auto.insert(t.name.clone());
                    lt.insert(t.name.clone(), 1);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
    if auto.is_empty() {
        return;
    }
    for f in files.iter_mut() {
        apply(&mut f.items, &auto, &lt);
    }
}

fn has_attr(attrs: &[Attr], name: &str) -> bool {
    attrs.iter().any(|a| a.name() == name)
}

/// Kotlin-like value semantics: data classes and enums get Debug/Clone/PartialEq,
/// plus Copy/Eq/Hash when every field type is known to support them.
fn compute_derives(k: &mut Crate, units: &[SourceUnit]) {
    fn gather<'a>(items: &'a [Item], out: &mut Vec<(&'a TypeDecl, &'a [Attr])>) {
        for it in items {
            match &it.kind {
                ItemKind::Type(t) => {
                    out.push((t, &it.attrs));
                    gather(&t.members, out);
                }
                ItemKind::Mod { items, .. } => gather(items, out),
                _ => {}
            }
        }
    }
    let mut decls = Vec::new();
    for u in units {
        gather(&u.file.items, &mut decls);
    }
    let mut copy: HashSet<String> = HashSet::new();
    let mut hash: HashSet<String> = HashSet::new();
    let field_types = |t: &TypeDecl| -> Vec<Type> {
        let mut v: Vec<Type> = t.fields.iter().map(|f| f.ty.clone()).collect();
        for var in &t.variants {
            match &var.fields {
                VariantFields::Unit => {}
                VariantFields::Tuple(ts) => v.extend(ts.iter().cloned()),
                VariantFields::Named(fs) => v.extend(fs.iter().map(|f| f.ty.clone())),
            }
        }
        v
    };
    let eligible = |t: &TypeDecl, attrs: &[Attr]| {
        !t.is_error && !has_attr(attrs, "no_auto_derive") && matches!(t.kind, TypeKind::Data | TypeKind::Enum) && only_lifetimes(&t.generics)
    };
    for _ in 0..6 {
        let mut changed = false;
        for (t, attrs) in &decls {
            if !eligible(t, attrs) {
                continue;
            }
            let fts = field_types(t);
            if !copy.contains(&t.name) && fts.iter().all(|ft| is_copy_simple(ft, &copy)) {
                copy.insert(t.name.clone());
                changed = true;
            }
            if !hash.contains(&t.name) && fts.iter().all(|ft| is_hash_simple(ft, &hash)) {
                hash.insert(t.name.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (t, attrs) in &decls {
        let mut d: Vec<String> = Vec::new();
        if t.is_error {
            d.push("Debug".into());
        } else if has_attr(attrs, "no_auto_derive") {
        } else if matches!(t.kind, TypeKind::Data | TypeKind::Enum) {
            d.extend(["Debug", "Clone"].iter().map(|s| s.to_string()));
            if only_lifetimes(&t.generics) && copy.contains(&t.name) {
                d.push("Copy".into());
            }
            d.push("PartialEq".into());
            if only_lifetimes(&t.generics) && hash.contains(&t.name) {
                d.push("Eq".into());
                d.push("Hash".into());
            }
        }
        k.derives.insert(t.name.clone(), d);
    }
}

fn collect_traits(k: &mut Crate, module: &[String], items: &[Item]) {
    for it in items {
        match &it.kind {
            ItemKind::Trait(t) => {
                let mut methods = HashMap::new();
                let mut assoc = Vec::new();
                for m in &t.members {
                    match &m.kind {
                        ItemKind::Fun(f) => {
                            methods.insert(f.name.clone(), method_info_explicit(f, None).unwrap_or_else(|| default_mi(f)));
                        }
                        ItemKind::TypeAlias { name, .. } => assoc.push(name.clone()),
                        _ => {}
                    }
                }
                insert_unique(&mut k.traits, &mut k.ambiguous, t.name.clone(), TraitInfo { name: t.name.clone(), module: module.to_vec(), methods, assoc_types: assoc });
            }
            ItemKind::Mod { name, items } => {
                let mut m = module.to_vec();
                m.push(name.clone());
                collect_traits(k, &m, items);
            }
            _ => {}
        }
    }
}

fn insert_unique<V>(map: &mut HashMap<String, V>, amb: &mut HashSet<String>, key: String, v: V) {
    if map.contains_key(&key) {
        amb.insert(key);
    } else {
        map.insert(key, v);
    }
}

pub fn params_info(f: &FunDecl) -> Vec<ParamInfo> {
    let generics: Vec<String> = f
        .generics
        .params
        .iter()
        .filter_map(|g| match g {
            GenericParam::Type { name, .. } | GenericParam::Const { name, .. } => Some(name.clone()),
            GenericParam::Lifetime { .. } => None,
        })
        .collect();
    f.params
        .iter()
        .map(|p| ParamInfo { name: p.name().map(|s| s.to_string()), ty: p.ty.clone(), default: p.default.clone(), generics: generics.clone() })
        .collect()
}

fn default_mi(f: &FunDecl) -> MethodInfo {
    MethodInfo { recv: Recv::Ref, params: params_info(f), throws: !matches!(f.throws, Throws::No), ret: f.ret.clone(), explicit: false }
}

/// Receiver determined purely from syntax, or None if it must be inferred.
fn method_info_explicit(f: &FunDecl, trait_recv: Option<Recv>) -> Option<MethodInfo> {
    let params = params_info(f);
    let throws = !matches!(f.throws, Throws::No);
    let recv = match &f.self_param {
        Some(SelfParam::Ref) => Some(Recv::Ref),
        Some(SelfParam::RefMut) => Some(Recv::RefMut),
        Some(SelfParam::Typed(Type::Ref { mutable: true, .. })) => Some(Recv::RefMut),
        Some(SelfParam::Typed(Type::Ref { mutable: false, .. })) => Some(Recv::Ref),
        Some(SelfParam::Value { .. }) | Some(SelfParam::Typed(_)) => Some(Recv::Value),
        None if f.mods.is_static => Some(Recv::Static),
        None if f.mods.is_mut => Some(Recv::RefMut),
        None if trait_recv.is_some() => trait_recv,
        None if STATIC_METHOD_NAMES.contains(&f.name.as_str()) => Some(Recv::Static),
        None if f.mods.is_operator => Some(Recv::Value),
        None => None,
    };
    recv.map(|recv| MethodInfo { recv, params, throws, ret: f.ret.clone(), explicit: trait_recv.is_none() || f.self_param.is_some() || f.mods.is_static || f.mods.is_mut })
}

fn collect_items(k: &mut Crate, module: &[String], items: &[Item]) {
    for it in items {
        if let ItemKind::Type(t) = &it.kind {
            let inner: Vec<&Item> = t.members.iter().filter(|m| matches!(m.kind, ItemKind::Type(_) | ItemKind::Trait(_))).collect();
            if !inner.is_empty() {
                let names: Vec<String> = inner
                    .iter()
                    .filter_map(|m| match &m.kind {
                        ItemKind::Type(x) => Some(x.name.clone()),
                        ItemKind::Trait(x) => Some(x.name.clone()),
                        _ => None,
                    })
                    .collect();
                k.nested.insert(t.name.clone(), names);
                let owned: Vec<Item> = inner.into_iter().cloned().collect();
                collect_items(k, module, &owned);
            }
        }
        match &it.kind {
            ItemKind::Fun(f) if f.receiver.is_none() => {
                let info = FunInfo { module: module.to_vec(), params: params_info(f), throws: !matches!(f.throws, Throws::No), ret: f.ret.clone() };
                insert_unique(&mut k.funs, &mut k.ambiguous, f.name.clone(), info);
            }
            ItemKind::Fun(f) => {
                k.ext_traits.push((module.to_vec(), ext_trait_name(module, &f.name)));
            }
            ItemKind::Type(t) => match t.kind {
                TypeKind::Class | TypeKind::Data | TypeKind::TupleStruct | TypeKind::Union => {
                    let fields = t
                        .fields
                        .iter()
                        .map(|f| FieldInfo { name: f.name.clone(), ty: f.ty.clone(), default: f.default.clone(), in_ctor: f.in_ctor })
                        .collect();
                    let mut methods = HashMap::new();
                    let mut user_new = false;
                    for m in &t.members {
                        if let ItemKind::Fun(f) = &m.kind {
                            if f.name == "new" {
                                user_new = true;
                            }
                            if let Some(mi) = method_info_explicit(f, None) {
                                methods.insert(f.name.clone(), mi);
                            } else {
                                methods.insert(f.name.clone(), default_mi(f));
                            }
                        }
                    }
                    let info = ClassInfo {
                        name: t.name.clone(),
                        module: module.to_vec(),
                        fields,
                        methods,
                        user_new,
                        is_data: t.kind == TypeKind::Data,
                        is_tuple: t.kind == TypeKind::TupleStruct,
                        is_union: t.kind == TypeKind::Union,
                        is_unit: t.kind != TypeKind::Union && !t.shared && t.fields.is_empty() && t.tuple_fields.is_empty(),
                        is_shared: t.shared,
                        is_sync: t.shared && it.attrs.iter().any(|a| a.name() == "sync"),
                        has_generics: !only_lifetimes(&t.generics),
                        type_params: t.generics.params.iter().filter(|g| matches!(g, GenericParam::Type { .. })).count(),
                        lifetimes: t.generics.params.iter().filter(|g| matches!(g, GenericParam::Lifetime { .. })).count(),
                    };
                    insert_unique(&mut k.classes, &mut k.ambiguous, t.name.clone(), info);
                }
                TypeKind::Enum => {
                    let variants: Vec<(String, VariantShape)> = t
                        .variants
                        .iter()
                        .map(|v| {
                            let shape = match &v.fields {
                                VariantFields::Unit => VariantShape::Unit,
                                VariantFields::Tuple(ts) => VariantShape::Tuple(ts.len()),
                                VariantFields::Named(fs) => VariantShape::Named(fs.iter().map(|f| (f.name.clone(), f.ty.clone())).collect()),
                            };
                            (v.name.clone(), shape)
                        })
                        .collect();
                    for (vn, _) in &variants {
                        k.variant_owner.entry(vn.clone()).or_default().push(t.name.clone());
                    }
                    let mut methods = HashMap::new();
                    for m in &t.members {
                        if let ItemKind::Fun(f) = &m.kind {
                            let mi = method_info_explicit(f, None).unwrap_or_else(|| default_mi(f));
                            methods.insert(f.name.clone(), mi);
                        }
                    }
                    let info = EnumInfo {
                        name: t.name.clone(),
                        module: module.to_vec(),
                        variants,
                        methods,
                        lifetimes: t.generics.params.iter().filter(|g| matches!(g, GenericParam::Lifetime { .. })).count(),
                        has_type_params: !only_lifetimes(&t.generics),
                    };
                    insert_unique(&mut k.enums, &mut k.ambiguous, t.name.clone(), info);
                }
            },
            ItemKind::Mod { name, items } => {
                let mut m = module.to_vec();
                m.push(name.clone());
                collect_items(k, &m, items);
            }
            _ => {}
        }
    }
}

fn collect_impls(k: &mut Crate, items: &[Item]) {
    for it in items {
        match &it.kind {
            ItemKind::Impl(imp) => {
                let Some(name) = imp.self_ty.last_name() else { continue };
                let trait_info = imp.trait_.as_ref().and_then(|t| t.last_name()).and_then(|n| k.traits.get(n)).cloned();
                for m in &imp.members {
                    if let ItemKind::Fun(f) = &m.kind {
                        let trecv = trait_info.as_ref().and_then(|t| t.methods.get(&f.name)).map(|m| m.recv);
                        let mi = method_info_explicit(f, trecv).unwrap_or_else(|| default_mi(f));
                        if let Some(c) = k.classes.get_mut(name) {
                            if f.name == "new" && imp.trait_.is_none() {
                                c.user_new = true;
                            }
                            c.methods.entry(f.name.clone()).or_insert(mi);
                        } else if let Some(e) = k.enums.get_mut(name) {
                            e.methods.entry(f.name.clone()).or_insert(mi);
                        }
                    }
                }
            }
            ItemKind::Mod { items, .. } => collect_impls(k, items),
            _ => {}
        }
    }
}

pub fn ext_trait_name(module: &[String], fun: &str) -> String {
    let mut s = String::from("__OxieExt");
    for m in module {
        s.push('_');
        s.push_str(m);
    }
    s.push('_');
    s.push_str(fun);
    s
}

// -------------------------------------------------------------------------------------------
// Receiver inference

/// Information about a method body relevant to receiver inference.
#[derive(Default, Debug)]
pub struct BodyFacts {
    pub uses_self: bool,
    pub mutates_self: bool,
}

struct MethodRef<'a> {
    owner: String,
    fun: &'a FunDecl,
    trait_recv: Option<Recv>,
    /// Implements this Oxie trait method (whose receiver is inferred from its implementations).
    implements: Option<String>,
}

fn infer_receivers(k: &mut Crate, units: &[SourceUnit]) {
    // gather method bodies per owner type
    let mut methods: Vec<MethodRef> = Vec::new();
    fn gather<'a>(k: &Crate, items: &'a [Item], out: &mut Vec<MethodRef<'a>>) {
        for it in items {
            match &it.kind {
                ItemKind::Type(t) => {
                    gather(k, &t.members, out);
                    for m in &t.members {
                        if let ItemKind::Fun(f) = &m.kind {
                            // methods implementing an Oxie interface take its receiver
                            let mut trecv = None;
                            let mut implements = None;
                            for st in &t.supertypes {
                                if let Some(ti) = st.last_name().and_then(|n| k.traits.get(n)) {
                                    if let Some(mi) = ti.methods.get(&f.name) {
                                        if mi.explicit {
                                            trecv = Some(mi.recv);
                                        } else {
                                            implements = Some(ti.name.clone());
                                        }
                                    }
                                }
                            }
                            out.push(MethodRef { owner: t.name.clone(), fun: f, trait_recv: trecv, implements });
                        }
                    }
                }
                ItemKind::Impl(imp) => {
                    let Some(owner) = imp.self_ty.last_name() else { continue };
                    let tinfo = imp.trait_.as_ref().and_then(|t| t.last_name()).and_then(|n| k.traits.get(n));
                    for m in &imp.members {
                        if let ItemKind::Fun(f) = &m.kind {
                            let mi = tinfo.and_then(|t| t.methods.get(&f.name));
                            let trecv = mi.filter(|m| m.explicit).map(|m| m.recv);
                            let implements = mi.filter(|m| !m.explicit).and(tinfo.map(|t| t.name.clone()));
                            out.push(MethodRef { owner: owner.to_string(), fun: f, trait_recv: trecv, implements });
                        }
                    }
                }
                ItemKind::Mod { items, .. } => gather(k, items, out),
                _ => {}
            }
        }
    }
    for u in units {
        gather(k, &u.file.items, &mut methods);
    }

    // initial: explicit receivers fixed, others start as Ref/Static
    let mut decided: HashMap<(String, String), Recv> = HashMap::new();
    let mut pending: Vec<usize> = Vec::new();
    for (i, m) in methods.iter().enumerate() {
        if let Some(mi) = method_info_explicit(m.fun, m.trait_recv) {
            decided.insert((m.owner.clone(), m.fun.name.clone()), mi.recv);
        } else {
            pending.push(i);
            decided.insert((m.owner.clone(), m.fun.name.clone()), Recv::Ref);
        }
    }
    // `&mut` parameters of crate functions and methods
    let mut mut_params: HashMap<String, Vec<usize>> = HashMap::new();
    let is_mut_ref = |t: &Type| matches!(t, Type::Ref { mutable: true, .. });
    for (name, f) in &k.funs {
        let idx: Vec<usize> = f.params.iter().enumerate().filter(|(_, p)| is_mut_ref(&p.ty)).map(|(i, _)| i).collect();
        if !idx.is_empty() {
            mut_params.insert(name.clone(), idx);
        }
    }
    for m in &methods {
        let idx: Vec<usize> = m.fun.params.iter().enumerate().filter(|(_, p)| is_mut_ref(&p.ty)).map(|(i, _)| i).collect();
        if !idx.is_empty() {
            mut_params.entry(m.fun.name.clone()).or_insert(idx);
        }
    }
    // fixpoint
    for _round in 0..8 {
        let mut changed = false;
        for &i in &pending {
            let m = &methods[i];
            let fields: HashSet<String> = k.classes.get(&m.owner).map(|c| c.fields.iter().map(|f| f.name.clone()).collect()).unwrap_or_default();
            let recvs: HashMap<String, Recv> = decided.iter().filter(|((o, _), _)| *o == m.owner).map(|((_, n), r)| (n.clone(), *r)).collect();
            let facts = analyze_fun_with(m.fun, &fields, &recvs, &mut_params);
            let returns_self = m.fun.ret.as_ref().is_some_and(|t| t.single_name() == Some("Self") || t.single_name() == Some(m.owner.as_str()));
            let recv = if facts.mutates_self {
                Recv::RefMut
            } else if !facts.uses_self && returns_self && m.trait_recv.is_none() && m.implements.is_none() {
                Recv::Static
            } else {
                Recv::Ref
            };
            let key = (m.owner.clone(), m.fun.name.clone());
            if decided.get(&key) != Some(&recv) {
                decided.insert(key, recv);
                changed = true;
            }
        }
        // An interface method is `&mut self` if any implementation needs it; then all implementations use it.
        let mut trait_recv: HashMap<(String, String), Recv> = HashMap::new();
        for m in &methods {
            if let Some(t) = &m.implements {
                let r = decided[&(m.owner.clone(), m.fun.name.clone())];
                let e = trait_recv.entry((t.clone(), m.fun.name.clone())).or_insert(Recv::Ref);
                if r == Recv::RefMut {
                    *e = Recv::RefMut;
                }
            }
        }
        for m in &methods {
            if let Some(t) = &m.implements {
                let want = trait_recv[&(t.clone(), m.fun.name.clone())];
                let key = (m.owner.clone(), m.fun.name.clone());
                if want == Recv::RefMut && decided.get(&key) != Some(&Recv::RefMut) {
                    decided.insert(key, Recv::RefMut);
                    changed = true;
                }
            }
        }
        for ((t, name), r) in trait_recv {
            if let Some(ti) = k.traits.get_mut(&t) {
                if let Some(mi) = ti.methods.get_mut(&name) {
                    mi.recv = r;
                }
            }
        }
        if !changed {
            break;
        }
    }
    for ((owner, name), recv) in decided {
        if let Some(c) = k.classes.get_mut(&owner) {
            if let Some(mi) = c.methods.get_mut(&name) {
                mi.recv = recv;
            }
        } else if let Some(e) = k.enums.get_mut(&owner) {
            if let Some(mi) = e.methods.get_mut(&name) {
                mi.recv = recv;
            }
        }
    }
}

pub fn analyze_fun(f: &FunDecl, fields: &HashSet<String>, methods: &HashMap<String, Recv>) -> BodyFacts {
    analyze_fun_with(f, fields, methods, &HashMap::new())
}

/// Like `analyze_fun`, also knowing which parameters of callable functions are `&mut`
/// (passing a field there borrows it mutably: `fill(items)` becomes `fill(&mut self.items)`).
pub fn analyze_fun_with(f: &FunDecl, fields: &HashSet<String>, methods: &HashMap<String, Recv>, mut_params: &HashMap<String, Vec<usize>>) -> BodyFacts {
    let mut w = Walker { fields, methods, mut_params, scopes: vec![HashSet::new()], facts: BodyFacts::default() };
    for p in &f.params {
        w.bind(&p.pat);
    }
    match &f.body {
        Some(FunBody::Block(b)) => w.block(b),
        Some(FunBody::Expr(e)) => w.expr(e),
        None => {}
    }
    w.facts
}

pub fn analyze_block(b: &Block, fields: &HashSet<String>, methods: &HashMap<String, Recv>) -> BodyFacts {
    let none = HashMap::new();
    let mut w = Walker { fields, methods, mut_params: &none, scopes: vec![HashSet::new()], facts: BodyFacts::default() };
    w.block(b);
    w.facts
}

struct Walker<'a> {
    fields: &'a HashSet<String>,
    methods: &'a HashMap<String, Recv>,
    /// Function and method name -> indices of its `&mut` parameters.
    mut_params: &'a HashMap<String, Vec<usize>>,
    scopes: Vec<HashSet<String>>,
    facts: BodyFacts,
}

impl<'a> Walker<'a> {
    fn is_local(&self, n: &str) -> bool {
        self.scopes.iter().any(|s| s.contains(n))
    }
    fn bind(&mut self, p: &Pat) {
        let mut names = Vec::new();
        p.bindings(&mut names);
        let top = self.scopes.last_mut().unwrap();
        for n in names {
            top.insert(n);
        }
    }
    fn push(&mut self) {
        self.scopes.push(HashSet::new());
    }
    fn pop(&mut self) {
        self.scopes.pop();
    }

    /// A field passed to a `&mut` parameter is borrowed mutably.
    fn mut_args(&mut self, fun: &str, args: &[Arg]) {
        let Some(idx) = self.mut_params.get(fun) else { return };
        for (i, a) in args.iter().enumerate() {
            if a.name.is_none() && idx.contains(&i) && self.rooted_in_self(&a.value) {
                self.facts.mutates_self = true;
            }
        }
    }

    /// Is the root of this place expression `self` or an implicit field?
    fn rooted_in_self(&self, e: &Expr) -> bool {
        let mut cur = e;
        loop {
            match &cur.kind {
                ExprKind::Field { recv, .. } => cur = recv,
                ExprKind::Index(r, _) => cur = r,
                ExprKind::MethodCall { recv, .. } => cur = recv,
                ExprKind::Unary(UnOp::Deref, r) => cur = r,
                ExprKind::Paren(r) => cur = r,
                ExprKind::SelfValue => return true,
                ExprKind::Path { .. } => {
                    return cur.ident().is_some_and(|n| !self.is_local(n) && self.fields.contains(n));
                }
                _ => return false,
            }
        }
    }

    fn block(&mut self, b: &Block) {
        self.push();
        for s in &b.stmts {
            self.stmt(s);
        }
        self.pop();
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let { pat, init, else_, .. } => {
                if let Some(e) = init {
                    self.expr(e);
                }
                if let Some(b) = else_ {
                    self.block(b);
                }
                self.bind(pat);
            }
            Stmt::Expr { expr, .. } => self.expr(expr),
            Stmt::Attributed { stmt, .. } => self.stmt(stmt),
            Stmt::Item(_) => {}
        }
    }

    fn cond(&mut self, c: &Cond) {
        match c {
            Cond::Expr(e) => self.expr(e),
            Cond::Let { pat, expr, .. } => {
                self.expr(expr);
                self.bind(pat);
            }
            Cond::Chain(parts) => parts.iter().for_each(|c| self.cond(c)),
        }
    }

    fn expr(&mut self, e: &Expr) {
        use ExprKind::*;
        match &e.kind {
            Path { .. } => {
                if let Some(n) = e.ident() {
                    if !self.is_local(n) && self.fields.contains(n) {
                        self.facts.uses_self = true;
                    }
                }
            }
            SelfValue => self.facts.uses_self = true,
            Template(parts) => {
                for p in parts {
                    if let TemplatePart::Expr(x, _) = p {
                        self.expr(x);
                    }
                }
            }
            Unary(op, x) => {
                if *op == UnOp::RefMut && self.rooted_in_self(x) {
                    self.facts.mutates_self = true;
                }
                self.expr(x);
            }
            Binary(_, a, b) | Elvis(a, b) | Pair(a, b) | Index(a, b) | ArrayRepeat(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            In { item, coll, .. } => {
                self.expr(item);
                self.expr(coll);
            }
            Is { expr, .. } => self.expr(expr),
            Assign(_, t, v) => {
                if self.rooted_in_self(t) {
                    self.facts.mutates_self = true;
                }
                self.expr(t);
                self.expr(v);
            }
            Call { callee, args } => {
                if let Some(n) = callee.ident() {
                    if !self.is_local(n) {
                        if let Some(r) = self.methods.get(n) {
                            if *r != Recv::Static {
                                self.facts.uses_self = true;
                            }
                            if *r == Recv::RefMut {
                                self.facts.mutates_self = true;
                            }
                        }
                    }
                }
                if let Some(n) = callee.ident().filter(|n| !self.is_local(n)) {
                    self.mut_args(n, args);
                }
                self.expr(callee);
                for a in args {
                    self.expr(&a.value);
                }
            }
            MethodCall { recv, name, args, .. } => {
                if matches!(recv.kind, SelfValue) {
                    if let Some(Recv::RefMut) = self.methods.get(name) {
                        self.facts.mutates_self = true;
                    }
                    self.mut_args(name, args);
                } else if MUTATING_METHODS.contains(&name.as_str()) && self.rooted_in_self(recv) {
                    self.facts.mutates_self = true;
                }
                self.expr(recv);
                for a in args {
                    self.expr(&a.value);
                }
            }
            Field { recv, .. } | Try(recv) | Unwrap(recv) | Await(recv) | Cast(recv, _) | Paren(recv) | Throw(recv) => self.expr(recv),
            If { cond, then, else_ } => {
                self.push();
                self.cond(cond);
                self.block(then);
                self.pop();
                if let Some(e) = else_ {
                    self.expr(e);
                }
            }
            When { subject, binding, arms, .. } => {
                if let Some(s) = subject {
                    self.expr(s);
                }
                if let Some(b) = binding {
                    self.bind(b);
                }
                for a in arms {
                    self.push();
                    for c in &a.conds {
                        match c {
                            WhenCond::Pat(p) => self.bind(p),
                            WhenCond::In { expr, .. } | WhenCond::Expr(expr) => self.expr(expr),
                            WhenCond::Else => {}
                        }
                    }
                    if let Some(g) = &a.guard {
                        self.expr(g);
                    }
                    self.expr(&a.body);
                    self.pop();
                }
            }
            Block(b) | Unsafe(b) | ConstBlock(b) | LabeledBlock { block: b, .. } | Loop { body: b, .. } => self.block(b),
            Async { block, .. } => self.block(block),
            While { cond, body, .. } => {
                self.push();
                self.cond(cond);
                self.block(body);
                self.pop();
            }
            DoWhile { body, cond, .. } => {
                self.block(body);
                self.expr(cond);
            }
            For { pat, iter, body, .. } => {
                self.expr(iter);
                self.push();
                self.bind(pat);
                self.block(body);
                self.pop();
            }
            Lambda { params, body, .. } => {
                self.push();
                for p in params {
                    self.bind(&p.pat);
                }
                self.scopes.last_mut().unwrap().insert("it".into());
                self.expr(body);
                self.pop();
            }
            Return(Some(x)) => self.expr(x),
            Break { value: Some(x), .. } => self.expr(x),
            Tuple(v) | Array(v) | VecLit(v) | MapLit(v) | SetLit(v) => {
                for x in v {
                    self.expr(x);
                }
            }
            Range { start, end, .. } => {
                if let Some(s) = start {
                    self.expr(s);
                }
                if let Some(s) = end {
                    self.expr(s);
                }
            }
            StructLit { fields, base, .. } => {
                for (n, v) in fields {
                    match v {
                        Some(v) => self.expr(v),
                        None => {
                            if !self.is_local(n) && self.fields.contains(n) {
                                self.facts.uses_self = true;
                            }
                        }
                    }
                }
                if let Some(b) = base {
                    self.expr(b);
                }
            }
            BuiltinMacro { args, .. } => {
                for a in args {
                    self.expr(&a.value);
                }
            }
            Macro { text, .. } | Raw(text) => {
                if contains_word(text, "self") {
                    self.facts.uses_self = true;
                }
            }
            TryCatch { body, catch } => {
                self.block(body);
                if let Some(c) = catch {
                    self.push();
                    self.bind(&c.binding);
                    self.block(&c.body);
                    self.pop();
                }
            }
            _ => {}
        }
    }
}

pub fn contains_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while let Some(pos) = text[i..].find(word) {
        let s = i + pos;
        let e = s + word.len();
        let before_ok = s == 0 || !(bytes[s - 1].is_ascii_alphanumeric() || bytes[s - 1] == b'_');
        let after_ok = e >= bytes.len() || !(bytes[e].is_ascii_alphanumeric() || bytes[e] == b'_');
        if before_ok && after_ok {
            return true;
        }
        i = e;
    }
    false
}

// -------------------------------------------------------------------------------------------
// Type facts used for automatic derives

const INTS: &[&str] = &["i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "Int", "Long", "Short", "Byte", "UInt", "ULong", "UByte", "UShort"];

/// Copy-able without knowing user types.
pub fn is_copy_simple(t: &Type, copy_types: &HashSet<String>) -> bool {
    match t {
        Type::Path { segs, .. } if segs.len() == 1 => {
            let n = segs[0].name.as_str();
            if segs[0].generics.is_none() && (INTS.contains(&n) || matches!(n, "f32" | "f64" | "bool" | "char" | "Double" | "Float" | "Boolean" | "Char")) {
                return true;
            }
            if n == "Option" {
                if let Some(g) = &segs[0].generics {
                    return g.iter().all(|a| matches!(a, GenericArg::Type(t) if is_copy_simple(t, copy_types)));
                }
            }
            no_type_args(&segs[0]) && copy_types.contains(n)
        }
        Type::Option(inner) => is_copy_simple(inner, copy_types),
        Type::Tuple(v) => v.iter().all(|t| is_copy_simple(t, copy_types)),
        Type::Array(inner, _) => is_copy_simple(inner, copy_types),
        Type::Ref { mutable: false, .. } => true,
        _ => false,
    }
}

/// Eq + Hash-able without knowing user types.
pub fn is_hash_simple(t: &Type, hash_types: &HashSet<String>) -> bool {
    match t {
        Type::Path { segs, .. } if segs.len() == 1 => {
            let n = segs[0].name.as_str();
            let args_ok = |seg: &PathSeg| seg.generics.as_ref().is_none_or(|g| g.iter().all(|a| matches!(a, GenericArg::Type(t) if is_hash_simple(t, hash_types))));
            if INTS.contains(&n) || matches!(n, "bool" | "char" | "String" | "Boolean" | "Char") {
                return segs[0].generics.is_none();
            }
            if matches!(n, "Vec" | "Option" | "Box" | "BTreeMap" | "BTreeSet" | "Rc" | "Arc") {
                return args_ok(&segs[0]);
            }
            no_type_args(&segs[0]) && hash_types.contains(n)
        }
        Type::Option(inner) => is_hash_simple(inner, hash_types),
        Type::Tuple(v) => v.iter().all(|t| is_hash_simple(t, hash_types)),
        Type::Array(inner, _) | Type::Slice(inner) => is_hash_simple(inner, hash_types),
        Type::Ref { mutable: false, inner, .. } => matches!(inner.single_name(), Some("str")) || is_hash_simple(inner, hash_types),
        _ => false,
    }
}

pub fn is_type_name_like(s: &str) -> bool {
    is_type_like(s)
}
