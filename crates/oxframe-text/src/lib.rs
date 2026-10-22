//! Text for oxframe: layout with `parley`, glyph outlines with `skrifa`.
//!
//! > **Status:** oxframe is under very active development. The API is not stable yet and
//! > may change from version to version. Pin the exact version in your Cargo.toml and check
//! > the [changelog](https://github.com/oxygenframe/oxframe-sdk/blob/main/CHANGELOG.md)
//! > before upgrading.
//!
//! Text becomes a [`BezPath`], so it renders identically on every backend
//! and can be morphed and trimmed like any other path.
//!
//! Determinism: only embedded fonts are used — Inter and JetBrains Mono
//! (SIL OFL 1.1, see `fonts/`) plus fonts registered with
//! [`register_font`]. System fonts are never consulted.

#![warn(missing_docs)]

mod code;
mod diff;

pub use code::{Lang, Token, highlight};
pub use diff::{
    CodeDiff, EditError, LineEdit, Loc, Piece, PieceEdit, code_lines, diff_code, insert_at,
    line_pieces, map_focus, remove_range, replace_range,
};

use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use kurbo::{Affine, BezPath, Rect};
use parley::fontique::{Blob, FontInfoOverride};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontWeight, Layout, LayoutContext,
    LineHeight, PositionedLayoutItem, StyleProperty,
};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};

const INTER: &[u8] = include_bytes!("../fonts/Inter.ttf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../fonts/JetBrainsMono.ttf");

/// Built-in sans-serif family (Inter).
pub const SANS: &str = "Inter";
/// Built-in monospace family (JetBrains Mono).
pub const MONO: &str = "JetBrains Mono";

/// Horizontal alignment of lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Align {
    #[default]
    /// Left.
    Start,
    /// Centered.
    Center,
    /// Right.
    End,
}

/// How text is laid out.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    /// Font family name: [`SANS`], [`MONO`] or a registered family.
    pub family: String,
    /// Font size in pixels.
    pub size: f64,
    /// 100..=900.
    pub weight: f64,
    /// Italic style.
    pub italic: bool,
    /// Multiple of the font size.
    pub line_height: f64,
    /// Extra space between letters, in pixels.
    pub letter_spacing: f64,
    /// Alignment of lines.
    pub align: Align,
    /// Wrap width; `None` means no wrapping.
    pub max_width: Option<f64>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            family: SANS.to_owned(),
            size: 48.0,
            weight: 400.0,
            italic: false,
            line_height: 1.2,
            letter_spacing: 0.0,
            align: Align::Start,
            max_width: None,
        }
    }
}

impl Hash for TextStyle {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.family.hash(h);
        for v in [
            self.size,
            self.weight,
            self.line_height,
            self.letter_spacing,
        ] {
            v.to_bits().hash(h);
        }
        self.italic.hash(h);
        self.align.hash(h);
        self.max_width.map(f64::to_bits).hash(h);
    }
}

/// A run of text with its own style; unset fields inherit the base style.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextSpan {
    /// The text.
    pub text: String,
    /// Font family.
    pub family: Option<String>,
    /// Font size in pixels.
    pub size: Option<f64>,
    /// Weight, 100..=900.
    pub weight: Option<f64>,
    /// Italic style.
    pub italic: Option<bool>,
}

impl Hash for TextSpan {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.text.hash(h);
        self.family.hash(h);
        self.size.map(f64::to_bits).hash(h);
        self.weight.map(f64::to_bits).hash(h);
        self.italic.hash(h);
    }
}

/// Where a glyph came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphInfo {
    /// Index of the span (0 for plain text).
    pub span: u16,
    /// First character of the glyph's cluster.
    pub ch: char,
    /// Pen position of the glyph (baseline origin).
    pub origin: kurbo::Point,
}

/// Laid-out text. Coordinates: origin at the top-left of the layout box,
/// y pointing down.
#[derive(Debug, Clone)]
pub struct TextLayout {
    /// All glyph outlines.
    pub path: BezPath,
    /// One path per glyph, in visual order — for per-letter animation.
    pub glyphs: Vec<BezPath>,
    /// Parallel to `glyphs`.
    pub info: Vec<GlyphInfo>,
    /// Width of the layout box.
    pub width: f64,
    /// Height of the layout box.
    pub height: f64,
}

impl TextLayout {
    /// The layout box as a rectangle.
    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width, self.height)
    }
}

struct Fonts {
    font_cx: FontContext,
    layout_cx: LayoutContext<u16>,
    cache: Vec<(u64, Rc<TextLayout>)>,
}

const CACHE_SIZE: usize = 256;

thread_local! {
    static FONTS: RefCell<Fonts> = RefCell::new(Fonts::new());
}

impl Fonts {
    fn new() -> Self {
        let mut font_cx = FontContext::new();
        for data in [INTER, JETBRAINS_MONO] {
            font_cx
                .collection
                .register_fonts(Blob::new(std::sync::Arc::new(data)), None);
        }
        Self {
            font_cx,
            layout_cx: LayoutContext::new(),
            cache: Vec::new(),
        }
    }
}

/// Registers a font (TTF/OTF/TTC bytes, e.g. from `include_bytes!`).
/// Returns the family names it provides.
pub fn register_font(data: &'static [u8]) -> Vec<String> {
    FONTS.with(|f| {
        let mut f = f.borrow_mut();
        f.cache.clear();
        let collection = &mut f.font_cx.collection;
        let families = collection.register_fonts(
            Blob::new(std::sync::Arc::new(data)),
            None::<FontInfoOverride<'_>>,
        );
        families
            .into_iter()
            .filter_map(|(id, _)| collection.family_name(id).map(str::to_owned))
            .collect()
    })
}

/// Lays out `text` (cached by text and style).
pub fn layout(text: &str, style: &TextStyle) -> Rc<TextLayout> {
    layout_rich(
        &[TextSpan {
            text: text.to_owned(),
            ..Default::default()
        }],
        style,
    )
}

/// Lays out styled spans as one paragraph (cached).
pub fn layout_rich(spans: &[TextSpan], style: &TextStyle) -> Rc<TextLayout> {
    let mut h = std::hash::DefaultHasher::new();
    spans.hash(&mut h);
    style.hash(&mut h);
    let key = h.finish();
    FONTS.with(|f| {
        let mut f = f.borrow_mut();
        if let Some(pos) = f.cache.iter().position(|(k, _)| *k == key) {
            let entry = f.cache.remove(pos);
            let value = entry.1.clone();
            f.cache.push(entry);
            return value;
        }
        let value = Rc::new(build(&mut f, spans, style));
        if f.cache.len() >= CACHE_SIZE {
            f.cache.remove(0);
        }
        f.cache.push((key, value.clone()));
        value
    })
}

fn build(f: &mut Fonts, spans: &[TextSpan], style: &TextStyle) -> TextLayout {
    let Fonts {
        font_cx, layout_cx, ..
    } = f;
    let text: String = spans.iter().map(|s| s.text.as_str()).collect();
    let text = text.as_str();
    let mut b = layout_cx.ranged_builder(font_cx, text, 1.0, true);
    b.push_default(StyleProperty::FontFamily(FontFamily::named(&style.family)));
    b.push_default(StyleProperty::FontSize(style.size as f32));
    b.push_default(StyleProperty::FontWeight(FontWeight::new(
        style.weight as f32,
    )));
    if style.italic {
        b.push_default(StyleProperty::FontStyle(parley::FontStyle::Italic));
    }
    b.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
        style.line_height as f32,
    )));
    b.push_default(StyleProperty::LetterSpacing(style.letter_spacing as f32));
    b.push_default(StyleProperty::Brush(0u16));
    let mut at = 0;
    for (i, span) in spans.iter().enumerate() {
        let r = at..at + span.text.len();
        at = r.end;
        if r.is_empty() {
            continue;
        }
        b.push(StyleProperty::Brush(i as u16), r.clone());
        if let Some(f) = &span.family {
            b.push(StyleProperty::FontFamily(FontFamily::named(f)), r.clone());
        }
        if let Some(sz) = span.size {
            b.push(StyleProperty::FontSize(sz as f32), r.clone());
        }
        if let Some(w) = span.weight {
            b.push(
                StyleProperty::FontWeight(FontWeight::new(w as f32)),
                r.clone(),
            );
        }
        if let Some(it) = span.italic {
            let st = if it {
                parley::FontStyle::Italic
            } else {
                parley::FontStyle::Normal
            };
            b.push(StyleProperty::FontStyle(st), r.clone());
        }
    }
    let mut layout: Layout<u16> = b.build(text);
    layout.break_all_lines(style.max_width.map(|w| w as f32));
    // parley aligns within the wrap width, but the block is as wide as its
    // widest line: break again at that width (the lines stay the same), so
    // centered and end-aligned lines sit inside the block's own box.
    if style.align != Align::Start && style.max_width.is_some() {
        let widest = layout.width();
        layout.break_all_lines(Some(widest + 1e-3));
    }
    let align = match style.align {
        Align::Start => Alignment::Start,
        Align::Center => Alignment::Center,
        Align::End => Alignment::End,
    };
    layout.align(align, AlignmentOptions::default());

    let mut path = BezPath::new();
    let mut glyphs = Vec::new();
    let mut info = Vec::new();
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            let r = run.run();
            let span = run.style().brush;
            // The source character of every glyph, in visual order.
            let chars: Vec<char> = r
                .visual_clusters()
                .flat_map(|c| {
                    let ch = text[c.text_range()].chars().next().unwrap_or('\0');
                    std::iter::repeat_n(ch, c.glyphs().count())
                })
                .collect();
            let font = r.font();
            let Ok(font_ref) = FontRef::from_index(font.data.as_ref(), font.index) else {
                continue;
            };
            let outlines = font_ref.outline_glyphs();
            let coords = r.normalized_coords();
            let size = Size::new(r.font_size());
            let coords: Vec<skrifa::instance::NormalizedCoord> = coords
                .iter()
                .map(|c| skrifa::raw::types::F2Dot14::from_bits(c.to_bits()))
                .collect();
            let location = LocationRef::new(&coords);
            // Synthetic italic when the family has no italic face.
            let skew = r
                .synthesis()
                .skew()
                .map_or(0.0, |deg| f64::from(deg).to_radians().tan());
            for (gi, g) in run.positioned_glyphs().enumerate() {
                let Some(outline) = outlines.get(GlyphId::new(g.id)) else {
                    continue;
                };
                let mut pen = Pen(BezPath::new());
                if outline
                    .draw(DrawSettings::unhinted(size, location), &mut pen)
                    .is_err()
                {
                    continue;
                }
                // Font units are y-up; flip and move to the pen position.
                let tf = Affine::translate((f64::from(g.x), f64::from(g.y)))
                    * Affine::scale_non_uniform(1.0, -1.0)
                    * Affine::skew(skew, 0.0);
                let glyph = tf * pen.0;
                if !glyph.elements().is_empty() {
                    path.extend(glyph.elements().iter().copied());
                    glyphs.push(glyph);
                    info.push(GlyphInfo {
                        span,
                        ch: chars.get(gi).copied().unwrap_or('\0'),
                        origin: kurbo::Point::new(f64::from(g.x), f64::from(g.y)),
                    });
                }
            }
        }
    }
    TextLayout {
        path,
        glyphs,
        info,
        width: f64::from(layout.width()),
        height: f64::from(layout.height()),
    }
}

struct Pen(BezPath);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((f64::from(x), f64::from(y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((f64::from(x), f64::from(y)));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(x), f64::from(y)),
        );
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to(
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(cx1), f64::from(cy1)),
            (f64::from(x), f64::from(y)),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}

#[cfg(test)]
mod tests {
    use kurbo::Shape;

    use super::*;

    #[test]
    fn lays_out_text_with_embedded_font() {
        let l = layout("Hello", &TextStyle::default());
        assert_eq!(l.glyphs.len(), 5);
        assert!(l.width > 100.0 && l.width < 160.0, "width {}", l.width);
        let bb = l.path.bounding_box();
        assert!(bb.y0 >= 0.0 && bb.y1 <= l.height, "{bb:?} h={}", l.height);
    }

    #[test]
    fn mono_and_weight_change_shapes() {
        let s = TextStyle::default();
        let regular = layout("iii", &s);
        let mono = layout(
            "iii",
            &TextStyle {
                family: MONO.into(),
                ..s.clone()
            },
        );
        let bold = layout("iii", &TextStyle { weight: 800.0, ..s });
        assert!(mono.width > regular.width);
        assert_ne!(bold.path, regular.path);
    }

    #[test]
    fn wraps_and_centers() {
        let s = TextStyle {
            max_width: Some(120.0),
            align: Align::Center,
            ..Default::default()
        };
        let l = layout("one two three four", &s);
        assert!(l.height > 2.0 * 48.0);
    }

    #[test]
    fn centered_lines_stay_inside_their_block() {
        let s = TextStyle {
            max_width: Some(1000.0),
            align: Align::Center,
            ..Default::default()
        };
        // One short line: the block is the line, not the wrap width.
        let one = layout("Fixed", &s);
        let b = one.path.bounding_box();
        assert!(
            b.x0 >= -1.0 && b.x1 <= one.width + 1.0,
            "{b:?} in {}",
            one.width
        );
        // Wrapped lines are centered in the widest one.
        let s = TextStyle {
            max_width: Some(300.0),
            ..s
        };
        let two = layout("a long first line of text then short", &s);
        let b = two.path.bounding_box();
        assert!(
            b.x0 >= -1.0 && b.x1 <= two.width + 1.0,
            "{b:?} in {}",
            two.width
        );
        assert!(two.height > 2.0 * 30.0);
    }

    #[test]
    fn rich_spans_carry_index_and_chars() {
        let spans = [
            TextSpan {
                text: "ab ".into(),
                ..Default::default()
            },
            TextSpan {
                text: "cd".into(),
                weight: Some(800.0),
                ..Default::default()
            },
        ];
        let l = layout_rich(&spans, &TextStyle::default());
        let got: Vec<(u16, char)> = l.info.iter().map(|i| (i.span, i.ch)).collect();
        assert_eq!(got, vec![(0, 'a'), (0, 'b'), (1, 'c'), (1, 'd')]);
        assert!(l.info[1].origin.x > l.info[0].origin.x);
    }

    #[test]
    fn cyrillic_is_supported() {
        let l = layout("Привет", &TextStyle::default());
        assert_eq!(l.glyphs.len(), 6);
    }
}
