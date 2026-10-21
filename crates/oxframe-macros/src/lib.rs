//! Macros re-exported by the `oxframe` facade. Generated code refers to
//! `::oxframe`, so they are meant to be used through it.
//!
//! > **Status:** oxframe is under very active development. The API is not stable yet and
//! > may change from version to version. Pin the exact version in your Cargo.toml and check
//! > the [changelog](https://github.com/oxygenframe/oxframe-sdk/blob/main/CHANGELOG.md)
//! > before upgrading.

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

/// A node of your own from a struct that implements `Draw`.
///
/// Fields marked `#[signal]` become animatable: the builder gets a setter
/// of the same name, and the handle (`<Name>Handle`) a method that returns
/// the field's `Signal`. Other fields are set once on the builder. Every
/// field type needs `Default` and `Clone`; signal fields also `Lerp`.
///
/// ```ignore
/// #[derive(Node)]
/// pub struct Gauge {
///     #[signal] value: f64,
///     #[signal] size: f64,
/// }
///
/// impl Draw for Gauge {
///     fn draw(&self, ctx: &mut DrawCtx) {
///         ctx.arc((0.0, 0.0), self.size / 2.0, 135.0, 270.0 * self.value).stroke(Accent, 16.0);
///     }
///     fn size(&self) -> (f64, f64) { (self.size, self.size) }
/// }
///
/// let g = view.add(Gauge::new().size(200.0));   // GaugeHandle
/// g.value().to(0.8, 1.0).await;
/// ```
#[proc_macro_derive(Node, attributes(signal))]
pub fn derive_node(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    match expand_node(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand_node(input: syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let vis = &input.vis;
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "#[derive(Node)] does not support generic structs",
        ));
    }
    let syn::Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            name,
            "#[derive(Node)] needs a struct",
        ));
    };
    let syn::Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            name,
            "#[derive(Node)] needs a struct with named fields",
        ));
    };
    let builder = format_ident!("{name}Builder");
    let handle = format_ident!("{name}Handle");
    let state = format_ident!("__{name}Signals");
    let type_name = name.to_string();

    let mut builder_fields = Vec::new();
    let mut builder_defaults = Vec::new();
    let mut setters = Vec::new();
    let mut state_fields = Vec::new();
    let mut state_init = Vec::new();
    let mut snapshot = Vec::new();
    let mut handle_methods = Vec::new();
    let mut getters = Vec::new();
    for f in &fields.named {
        let ident = f.ident.as_ref().expect("named field");
        let ty = &f.ty;
        let signal = f.attrs.iter().any(|a| a.path().is_ident("signal"));
        let doc = format!("Sets `{ident}`.");
        builder_fields.push(quote! { #ident: #ty });
        builder_defaults.push(quote! { #ident: ::core::default::Default::default() });
        setters.push(quote! {
            #[doc = #doc]
            #vis fn #ident(mut self, v: impl ::core::convert::Into<#ty>) -> Self {
                self.#ident = v.into();
                self
            }
        });
        if signal {
            let label = format!("{name}.{ident}");
            let hdoc = format!("`{ident}` as an animatable signal.");
            let gdoc = format!("The current `{ident}`.");
            state_fields.push(quote! { #ident: ::oxframe::Signal<#ty> });
            state_init.push(quote! {
                #ident: ::oxframe::Signal::new(self.#ident).with_name(#label)
            });
            snapshot.push(quote! { #ident: s.#ident.get() });
            handle_methods.push(quote! {
                #[doc = #hdoc]
                #vis fn #ident(self) -> ::oxframe::Signal<#ty> {
                    ::oxframe::nodes::node_api::state::<#state>(self.0).#ident.clone()
                }
            });
            getters.push(quote! {
                #[doc = #gdoc]
                #vis fn #ident(&self) -> #ty {
                    ::core::clone::Clone::clone(&self.#ident)
                }
            });
        } else {
            state_fields.push(quote! { #ident: #ty });
            state_init.push(quote! { #ident: self.#ident });
            snapshot.push(quote! { #ident: ::core::clone::Clone::clone(&s.#ident) });
        }
    }
    let bdoc = format!("Builder for [`{name}`]; pass it to `add`.");
    let hdoc = format!("Handle to a [`{name}`] node. Derefs to `Node`.");
    Ok(quote! {
        #[doc = #bdoc]
        #[must_use = "a builder does nothing until it is passed to `add`"]
        #vis struct #builder {
            base: ::oxframe::nodes::BaseProps,
            #(#builder_fields,)*
        }

        impl #name {
            /// Starts a node with every field at its default.
            #[allow(clippy::new_ret_no_self)]
            #vis fn new() -> #builder {
                #builder {
                    base: ::core::default::Default::default(),
                    #(#builder_defaults,)*
                }
            }
            #(#getters)*
        }

        impl #builder {
            #(#setters)*
        }

        #[doc(hidden)]
        #vis struct #state {
            #(#state_fields,)*
        }

        #[doc = #hdoc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #vis struct #handle(::oxframe::Node);

        impl #handle {
            #(#handle_methods)*
            /// The untyped node handle.
            #vis fn node(self) -> ::oxframe::Node {
                self.0
            }
        }

        impl ::core::ops::Deref for #handle {
            type Target = ::oxframe::Node;
            fn deref(&self) -> &::oxframe::Node {
                &self.0
            }
        }

        impl ::core::convert::From<#handle> for ::oxframe::Node {
            fn from(h: #handle) -> ::oxframe::Node {
                h.0
            }
        }

        impl ::oxframe::NodeBuilder for #builder {
            type Handle = #handle;
            fn base(&mut self) -> &mut ::oxframe::nodes::BaseProps {
                &mut self.base
            }
            fn build(
                self,
                graph: &::oxframe::nodes::SceneGraph,
                parent: ::oxframe::NodeId,
            ) -> #handle {
                let state = ::std::rc::Rc::new(#state { #(#state_init,)* });
                let s = state.clone();
                let snapshot: ::std::rc::Rc<dyn Fn() -> #name> =
                    ::std::rc::Rc::new(move || #name { #(#snapshot,)* });
                let id = ::oxframe::nodes::node_api::add(
                    graph,
                    parent,
                    self.base,
                    #type_name,
                    snapshot,
                    state,
                );
                #handle(::oxframe::Node::from_id(id))
            }
        }
    })
}
