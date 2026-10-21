//! `icon!("lucide:arrow-right")`: an icon from Lucide, Tabler or Simple
//! Icons as an oxframe node. Use it through `oxframe` with the `icons`
//! feature.
//!
//! The sets are read when your code compiles; only the SVG of the icons you
//! name is put into the binary. A name that does not exist is a compile
//! error that lists the closest ones.

#![warn(missing_docs)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{LitByteStr, LitStr, parse_macro_input};

struct Set {
    prefix: &'static str,
    data: &'static str,
    /// Lucide and Tabler outline icons are strokes; the rest are fills.
    stroke: bool,
}

const SETS: [Set; 3] = [
    Set {
        prefix: "lucide",
        data: include_str!("../data/lucide.tsv"),
        stroke: true,
    },
    Set {
        prefix: "tabler",
        data: include_str!("../data/tabler.tsv"),
        stroke: true,
    },
    Set {
        prefix: "simple",
        data: include_str!("../data/simple.tsv"),
        stroke: false,
    },
];

/// An icon node builder: `icon!("lucide:server")`, `icon!("tabler:database")`,
/// `icon!("tabler:heart-filled")`, `icon!("simple:github")`.
///
/// Expands to `oxframe::nodes::Icon::from_svg(..)`, so `.size(64.0)`,
/// `.color(Accent)` and `.stroke_width(2.0)` follow.
#[proc_macro]
pub fn icon(input: TokenStream) -> TokenStream {
    let lit = parse_macro_input!(input as LitStr);
    match expand(&lit) {
        Ok(tokens) => tokens.into(),
        Err(message) => syn::Error::new(lit.span(), message)
            .to_compile_error()
            .into(),
    }
}

fn expand(lit: &LitStr) -> Result<proc_macro2::TokenStream, String> {
    let full = lit.value();
    let Some((prefix, name)) = full.split_once(':') else {
        return Err(format!(
            "write the set and the name, such as \"lucide:{full}\". Sets: lucide, tabler, simple"
        ));
    };
    let set = SETS
        .iter()
        .find(|s| s.prefix == prefix)
        .ok_or_else(|| format!("no icon set `{prefix}`. Sets: lucide, tabler, simple"))?;
    let body = set
        .data
        .lines()
        .find_map(|line| {
            let (n, b) = line.split_once('\t')?;
            (n == name).then_some(b)
        })
        .ok_or_else(|| {
            let near = closest(name, set.data);
            if near.is_empty() {
                format!("no icon `{name}` in {prefix}")
            } else {
                format!(
                    "no icon `{name}` in {prefix}. Did you mean: {}?",
                    near.join(", ")
                )
            }
        })?;
    let (style, open) = if set.stroke && !name.ends_with("-filled") {
        (
            quote!(Stroke),
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">"##,
        )
    } else {
        (
            quote!(Fill),
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="#000">"##,
        )
    };
    let svg = LitByteStr::new(format!("{open}{body}</svg>").as_bytes(), lit.span());
    Ok(quote! {
        ::oxframe::nodes::Icon::from_svg(#svg, ::oxframe::nodes::IconStyle::#style)
    })
}

/// Up to three names of `data` closest to `name`.
fn closest(name: &str, data: &str) -> Vec<String> {
    let mut scored: Vec<(usize, &str)> = data
        .lines()
        .filter_map(|l| l.split_once('\t').map(|(n, _)| n))
        .map(|n| (distance(name, n), n))
        .collect();
    scored.sort();
    scored
        .into_iter()
        .take_while(|(d, _)| *d <= name.len().max(3) / 2 + 1)
        .take(3)
        .map(|(_, n)| n.to_string())
        .collect()
}

fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(name: &str) -> String {
        expand(&LitStr::new(name, proc_macro2::Span::call_site())).unwrap_err()
    }

    #[test]
    fn every_set_has_its_common_icons() {
        for name in [
            "lucide:server",
            "tabler:database",
            "tabler:heart-filled",
            "simple:github",
        ] {
            let tokens = expand(&LitStr::new(name, proc_macro2::Span::call_site()))
                .unwrap()
                .to_string();
            assert!(tokens.contains("Icon :: from_svg"), "{name}: {tokens}");
        }
    }

    #[test]
    fn a_typo_lists_the_closest_names() {
        let e = err("lucide:arow-right");
        assert!(e.contains("arrow-right"), "{e}");
        assert!(err("lucid:server").contains("Sets: lucide, tabler, simple"));
        assert!(err("server").contains("\"lucide:server\""));
    }
}
