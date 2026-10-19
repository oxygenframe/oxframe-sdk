//! Macros re-exported by the `oxframe` facade. Generated code refers to
//! `::oxframe`, so they are meant to be used through it.

#![warn(missing_docs)]

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Expr, ItemFn, LitStr, Token, parse_macro_input};

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn check_name(lit: &LitStr) -> syn::Result<()> {
    if valid_name(&lit.value()) {
        Ok(())
    } else {
        Err(syn::Error::new(
            lit.span(),
            "names of markers, eases and tunables must match [a-z0-9_-]+",
        ))
    }
}

/// Turns `async fn intro(view: View) { .. }` into `fn intro() -> SceneDef`.
///
/// The scene's name is the function name; its meta lives in
/// `<name>.meta.json` next to the source file.
#[proc_macro_attribute]
pub fn scene(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(
            proc_macro2::TokenStream::from(attr)
                .into_iter()
                .next()
                .unwrap()
                .span(),
            "#[scene] takes no arguments",
        )
        .to_compile_error()
        .into();
    }
    let func = parse_macro_input!(item as ItemFn);
    match expand_scene(func) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_scene(func: ItemFn) -> syn::Result<proc_macro2::TokenStream> {
    let sig = &func.sig;
    if sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            sig.fn_token,
            "a scene must be an `async fn`",
        ));
    }
    if !sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &sig.generics,
            "a scene cannot be generic",
        ));
    }
    if sig.inputs.len() > 1 {
        return Err(syn::Error::new_spanned(
            &sig.inputs,
            "a scene takes at most one argument: `view: View`",
        ));
    }
    let name = &sig.ident;
    let name_str = name.to_string();
    if !valid_name(&name_str) {
        return Err(syn::Error::new_spanned(
            name,
            "scene names must be lowercase snake_case ([a-z0-9_])",
        ));
    }
    let vis = &func.vis;
    let attrs = &func.attrs;
    let inner = format_ident!("__oxframe_scene_{}", name);
    let mut inner_fn = func.clone();
    inner_fn.sig.ident = inner.clone();
    inner_fn.vis = syn::Visibility::Inherited;
    inner_fn.attrs.clear();
    let call = if sig.inputs.is_empty() {
        quote!(#inner())
    } else {
        quote!(#inner(view))
    };

    Ok(quote! {
        #(#attrs)*
        #vis fn #name() -> ::oxframe::SceneDef {
            #inner_fn
            ::oxframe::SceneDef::new(
                #name_str,
                ::oxframe::SourceLocation {
                    file: ::core::file!(),
                    manifest_dir: ::core::env!("CARGO_MANIFEST_DIR"),
                },
                |view: ::oxframe::View| -> ::oxframe::BoxFuture {
                    let _ = &view;
                    ::std::boxed::Box::pin(#call)
                },
            )
        }
    })
}

/// `color!("#e13238")` — a `Color` checked at compile time. Accepts
/// `#rgb`, `#rgba`, `#rrggbb` and `#rrggbbaa`.
#[proc_macro]
pub fn color(input: TokenStream) -> TokenStream {
    let lit = parse_macro_input!(input as LitStr);
    let s = lit.value();
    let hex = s.strip_prefix('#').unwrap_or(&s);
    let digits: Option<Vec<u8>> = hex
        .chars()
        .map(|c| c.to_digit(16).map(|d| d as u8))
        .collect();
    let channels = match (digits, hex.len()) {
        (Some(d), 3 | 4) => Some(d.iter().map(|v| v * 17).collect::<Vec<_>>()),
        (Some(d), 6 | 8) => Some(d.chunks(2).map(|p| p[0] * 16 + p[1]).collect()),
        _ => None,
    };
    let Some(mut c) = channels else {
        return syn::Error::new(lit.span(), "expected #rgb, #rgba, #rrggbb or #rrggbbaa")
            .to_compile_error()
            .into();
    };
    if c.len() == 3 {
        c.push(255);
    }
    let (r, g, b, a) = (c[0], c[1], c[2], c[3]);
    quote!(::oxframe::Color::from_rgba8(#r, #g, #b, #a)).into()
}

struct NameArgs {
    name: LitStr,
    rest: Punctuated<Expr, Token![,]>,
}

impl Parse for NameArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: LitStr = input.parse()?;
        check_name(&name)?;
        let rest = if input.is_empty() {
            Punctuated::new()
        } else {
            input.parse::<Token![,]>()?;
            Punctuated::parse_terminated(input)?
        };
        Ok(Self { name, rest })
    }
}

/// `ease!("slide")` or `ease!("slide", presets::OUT_BACK)` — a named Bézier
/// easing editable in the curve editor; the value lives in `meta.json`.
#[proc_macro]
pub fn ease(input: TokenStream) -> TokenStream {
    let NameArgs { name, rest } = parse_macro_input!(input as NameArgs);
    match rest.len() {
        0 => quote!(::oxframe::ease_named(#name, ::core::option::Option::None)).into(),
        1 => {
            let d = &rest[0];
            quote!(::oxframe::ease_named(#name, ::core::option::Option::Some(#d))).into()
        }
        _ => syn::Error::new_spanned(&rest, "usage: ease!(\"name\") or ease!(\"name\", default)")
            .to_compile_error()
            .into(),
    }
}

/// `tune!("speed", 1.4)` — a number editable with a slider in the
/// inspector; the value lives in `meta.json`.
#[proc_macro]
pub fn tune(input: TokenStream) -> TokenStream {
    let NameArgs { name, rest } = parse_macro_input!(input as NameArgs);
    if rest.len() != 1 {
        return syn::Error::new(name.span(), "usage: tune!(\"name\", default)")
            .to_compile_error()
            .into();
    }
    let d = &rest[0];
    quote!(::oxframe::tunable(#name, (#d) as f64)).into()
}
