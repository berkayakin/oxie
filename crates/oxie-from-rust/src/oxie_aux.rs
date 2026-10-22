//! Support for Rust produced by `oxie emit`: its runtime module (`crate::__oxie`) and the
//! wrapper `main` of `fun main() throws` are turned back into Oxie.

use quote::ToTokens;

/// `crate::__oxie::AnyError` → `AnyError`.
pub fn strip_runtime_path(path: &syn::Path) -> Option<String> {
    let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    if segs.len() == 3 && segs[0] == "crate" && segs[1] == "__oxie" && segs[2] == "AnyError" {
        return Some("AnyError".into());
    }
    None
}

pub fn preprocess(file: &mut syn::File) {
    // `mod __oxie;` and `use crate::__oxie::prelude::*;`
    file.items.retain(|it| match it {
        syn::Item::Mod(m) => m.ident != "__oxie",
        syn::Item::Use(u) => !u.to_token_stream().to_string().contains("__oxie"),
        _ => true,
    });
    // fn main() { if let Err(err) = __oxie_main() { crate::__oxie::report(err); exit(1); } }
    let has_wrapped = file.items.iter().any(|it| matches!(it, syn::Item::Fn(f) if f.sig.ident == "__oxie_main"));
    if has_wrapped {
        let wrapper = file.items.iter().position(|it| matches!(it, syn::Item::Fn(f) if f.sig.ident == "main" && f.block.to_token_stream().to_string().contains("__oxie_main") && f.block.to_token_stream().to_string().contains("report")));
        if let Some(w) = wrapper {
            let syn::Item::Fn(wf) = file.items.remove(w) else { return };
            for it in file.items.iter_mut() {
                if let syn::Item::Fn(f) = it {
                    if f.sig.ident == "__oxie_main" {
                        f.sig.ident = syn::Ident::new("main", f.sig.ident.span());
                        // `#[tokio::main]` and friends were on the wrapper
                        f.attrs.extend(wf.attrs.iter().cloned());
                    }
                }
            }
        }
    }
}
