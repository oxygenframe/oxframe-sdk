//! Typst math to SVG, for the `Math` node of `oxframe` (feature `math`).
//!
//! The formula is laid out by [Typst](https://typst.app) with the bundled
//! New Computer Modern Math font, so it looks the same on every machine
//! and needs no TeX install. Glyphs come out as paths in reading order.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

/// New Computer Modern Math (GUST Font License, `fonts/LICENSE-GUST.txt`).
const FONT: &[u8] = include_bytes!("../fonts/NewCMMath-Book.otf");

/// Font size the formula is laid out at, in points (one em).
pub const LAYOUT_PT: f64 = 12.0;

/// A laid-out formula.
#[derive(Debug, Clone, PartialEq)]
pub struct MathSvg {
    /// SVG document whose view box hugs the formula, in points.
    pub svg: String,
    /// Width in points.
    pub width: f64,
    /// Height in points.
    pub height: f64,
}

/// A formula Typst could not lay out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MathError {
    /// Typst's message, with its hints after it.
    pub message: String,
}

impl std::fmt::Display for MathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MathError {}

/// Lays out `formula` (Typst math syntax, without the `$`) at
/// [`LAYOUT_PT`] in black. Results are cached per formula.
///
/// `display` sets the formula on its own line (larger operators, limits
/// above and below); inline math is set as in running text.
///
/// # Errors
///
/// If Typst reports an error, such as an unknown symbol or unbalanced
/// brackets.
pub fn to_svg(formula: &str, display: bool) -> Result<MathSvg, MathError> {
    type Cache = HashMap<(String, bool), Result<MathSvg, MathError>>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let key = (formula.to_owned(), display);
    if let Some(hit) = cache.lock().expect("math cache").get(&key) {
        return hit.clone();
    }
    let set = layout(formula, display);
    cache.lock().expect("math cache").insert(key, set.clone());
    set
}

fn layout(formula: &str, display: bool) -> Result<MathSvg, MathError> {
    let body = if display {
        format!("$ {formula} $")
    } else {
        format!("${formula}$")
    };
    let text = format!(
        "#set page(width: auto, height: auto, margin: 0pt, fill: none)\n\
         #set text(font: \"New Computer Modern Math\", size: {LAYOUT_PT}pt, fill: black, top-edge: \"ascender\", bottom-edge: \"descender\")\n\
         {body}\n"
    );
    let world = MathWorld {
        source: Source::detached(text),
    };
    let doc = typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|errors| MathError {
            message: errors
                .iter()
                .map(|e| {
                    let mut m = e.message.to_string();
                    for hint in &e.hints {
                        m.push_str("; hint: ");
                        m.push_str(&hint.v);
                    }
                    m
                })
                .collect::<Vec<_>>()
                .join("\n"),
        })?;
    let page = doc.pages().first().ok_or_else(|| MathError {
        message: "the formula produced no output".into(),
    })?;
    let size = page.frame.size();
    Ok(MathSvg {
        svg: typst_svg::svg(page, &typst_svg::SvgOptions::default()),
        width: size.x.to_pt(),
        height: size.y.to_pt(),
    })
}

struct Shared {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| {
        let fonts: Vec<Font> = Font::iter(Bytes::new(FONT)).collect();
        Shared {
            library: LazyHash::new(Library::builder().build()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
        }
    })
}

/// One formula, one in-memory file, one font. No file or network access.
struct MathWorld {
    source: Source,
}

impl World for MathWorld {
    fn library(&self) -> &LazyHash<Library> {
        &shared().library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &shared().book
    }

    fn main(&self) -> FileId {
        self.source.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(FileError::AccessDenied)
        }
    }

    fn file(&self, _id: FileId) -> FileResult<Bytes> {
        Err(FileError::AccessDenied)
    }

    fn font(&self, index: usize) -> Option<Font> {
        shared().fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        // No clock: the same source always gives the same picture.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_formula_becomes_paths() {
        let set = layout("e^(i pi) + 1 = 0", false).unwrap();
        assert!(set.svg.starts_with("<svg") && set.svg.contains("<path"));
        assert!(set.width > set.height && set.height > 6.0, "{set:?}");
        assert_eq!(set, layout("e^(i pi) + 1 = 0", false).unwrap());
    }

    #[test]
    fn display_math_is_taller_for_big_operators() {
        let inline = to_svg("sum_(k=1)^n k", false).unwrap();
        let display = to_svg("sum_(k=1)^n k", true).unwrap();
        assert!(display.height > inline.height);
    }

    /// The syntax the agent reference (templates/agents/reference.md in
    /// oxframe-cli) promises.
    #[test]
    fn the_documented_examples_set() {
        for f in [
            "x^2",
            "x_i",
            "sqrt(x)",
            "(a + b) / c",
            "sum_(k=1)^n",
            "integral_0^1",
            "alpha pi",
            "vec(1, 2)",
            "mat(1, 2; 3, 4)",
            "\"text\"",
        ] {
            assert!(to_svg(f, true).is_ok(), "{f}: {:?}", to_svg(f, true));
        }
    }

    #[test]
    fn an_unknown_symbol_is_an_error_with_typst_message() {
        let err = to_svg("#nosuchthing", false).unwrap_err();
        assert!(err.message.contains("nosuchthing"), "{err}");
    }
}
