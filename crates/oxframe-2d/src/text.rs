//! Styled text for `Txt` and `Code`: rich spans, syntax themes, focus
//! dimming and glyph-level morphing between two strings.

use std::rc::Rc;

use kurbo::{Affine, BezPath, Point, Vec2};
use oxframe_core::{Color, Lerp};

use crate::theme::IntoPaint;
use oxframe_text::{Lang, TextLayout, TextSpan, TextStyle, Token};

/// A run of text with its own look; unset fields use the node's style.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RichSpan {
    /// The text of this run.
    pub text: String,
    /// Color; default: the node's fill.
    pub color: Option<Color>,
    /// Font weight 100..=900; default: the node's.
    pub weight: Option<f64>,
    /// Italic; default: the node's.
    pub italic: Option<bool>,
    /// Font size; default: the node's.
    pub size: Option<f64>,
    /// Font family; default: the node's.
    pub family: Option<String>,
}

impl RichSpan {
    /// A run with the node's style; same as [`span`].
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }
    /// Text color: a [`Color`] or a theme role such as `Warning`.
    pub fn color(mut self, c: impl IntoPaint) -> Self {
        self.color = Some(c.into_paint());
        self
    }
    /// Font weight, 100..=900.
    pub fn weight(mut self, w: f64) -> Self {
        self.weight = Some(w);
        self
    }
    /// Weight 700.
    pub fn bold(self) -> Self {
        self.weight(700.0)
    }
    /// Italic style.
    pub fn italic(mut self) -> Self {
        self.italic = Some(true);
        self
    }
    /// Font size in pixels.
    pub fn size(mut self, s: f64) -> Self {
        self.size = Some(s);
        self
    }
    /// Font family, e.g. [`MONO`](crate::MONO).
    pub fn family(mut self, f: impl Into<String>) -> Self {
        self.family = Some(f.into());
        self
    }
}

/// Shorthand for [`RichSpan::new`].
pub fn span(text: impl Into<String>) -> RichSpan {
    RichSpan::new(text)
}

/// Strings tween as a typewriter over the concatenated text, keeping the
/// styles of the target spans.
impl Lerp for RichSpan {
    fn lerp(&self, to: &Self, t: f64) -> Self {
        if t < 0.5 { self.clone() } else { to.clone() }
    }
}

/// Colors of a code theme.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeTheme {
    /// Text that is not a token below.
    pub plain: Color,
    /// `fn`, `let`, `if`…
    pub keyword: Color,
    /// Type names.
    pub types: Color,
    /// Function names.
    pub function: Color,
    /// Macro names.
    pub macros: Color,
    /// String literals.
    pub string: Color,
    /// Number literals.
    pub number: Color,
    /// Comments.
    pub comment: Color,
    /// Attributes and decorators.
    pub attribute: Color,
    /// Punctuation and operators.
    pub punct: Color,
}

impl CodeTheme {
    /// One Dark.
    pub fn dark() -> Self {
        let h = |s| Color::from_hex(s).expect("valid color");
        Self {
            plain: h("#abb2bf"),
            keyword: h("#c678dd"),
            types: h("#e5c07b"),
            function: h("#61afef"),
            macros: h("#56b6c2"),
            string: h("#98c379"),
            number: h("#d19a66"),
            comment: h("#7f848e"),
            attribute: h("#e06c75"),
            punct: h("#abb2bf"),
        }
    }

    /// One Light.
    pub fn light() -> Self {
        let h = |s| Color::from_hex(s).expect("valid color");
        Self {
            plain: h("#383a42"),
            keyword: h("#a626a4"),
            types: h("#c18401"),
            function: h("#4078f2"),
            macros: h("#0184bc"),
            string: h("#50a14f"),
            number: h("#986801"),
            comment: h("#6b6f78"),
            attribute: h("#e45649"),
            punct: h("#383a42"),
        }
    }

    /// Color for a token kind.
    pub fn color(&self, t: Token) -> Color {
        match t {
            Token::Plain => self.plain,
            Token::Keyword => self.keyword,
            Token::Type => self.types,
            Token::Function => self.function,
            Token::Macro => self.macros,
            Token::String => self.string,
            Token::Number => self.number,
            Token::Comment => self.comment,
            Token::Attribute => self.attribute,
            Token::Punct => self.punct,
        }
    }
}

impl Default for CodeTheme {
    fn default() -> Self {
        Self::dark()
    }
}

/// Text ready to draw: layout plus the color of each span (`None` = node
/// fill).
pub(crate) struct Styled {
    pub layout: Rc<TextLayout>,
    pub colors: Vec<Option<Color>>,
}

pub(crate) fn rich(spans: &[RichSpan], style: &TextStyle) -> Styled {
    let ts: Vec<TextSpan> = spans
        .iter()
        .map(|s| TextSpan {
            text: s.text.clone(),
            family: s.family.clone(),
            size: s.size,
            weight: s.weight,
            italic: s.italic,
        })
        .collect();
    Styled {
        layout: oxframe_text::layout_rich(&ts, style),
        colors: spans.iter().map(|s| s.color).collect(),
    }
}

/// Highlighted code; lines outside `focus` (1-based, inclusive; (0, 0) =
/// all) get their alpha multiplied by `dim`.
pub(crate) fn code(
    src: &str,
    lang: Lang,
    theme: &CodeTheme,
    style: &TextStyle,
    focus: Vec2,
    dim: f64,
) -> Styled {
    let (a, b) = (focus.x.round(), focus.y.round());
    let focused = |line: usize| a <= 0.0 && b <= 0.0 || (line as f64 >= a && line as f64 <= b);
    let mut spans = Vec::new();
    for (range, tok) in oxframe_text::highlight(src, lang) {
        // Split tokens at newlines so each piece knows its line.
        let mut start = range.start;
        for (i, ch) in src[range.clone()].char_indices() {
            if ch == '\n' {
                spans.push((start..range.start + i + 1, tok));
                start = range.start + i + 1;
            }
        }
        if start < range.end {
            spans.push((start..range.end, tok));
        }
    }
    let mut out = Vec::with_capacity(spans.len());
    for (r, tok) in spans {
        let line = src[..r.start].matches('\n').count() + 1;
        let mut c = theme.color(tok);
        if !focused(line) {
            c = c.with_alpha((f64::from(c.a) * dim.clamp(0.0, 1.0)) as f32);
        }
        out.push(RichSpan {
            text: src[r].to_owned(),
            color: Some(c),
            ..Default::default()
        });
    }
    rich(&out, style)
}

/// One painted glyph of a code diff, in the layout box (origin top-left).
pub(crate) struct DiffGlyph {
    pub path: BezPath,
    pub color: Color,
}

/// A flash behind an inserted fragment, in the same coordinates.
pub(crate) struct DiffFlash {
    pub rect: kurbo::Rect,
    pub color: Color,
}

/// A frame of a code diff: glyphs, optional flashes, and the box size.
pub(crate) struct DiffFrame {
    pub glyphs: Vec<DiffGlyph>,
    pub flashes: Vec<DiffFlash>,
    pub size: (f64, f64),
}

/// Where one piece sits before it is drawn. Tests check that unchanged
/// tokens move continuously and keep their color; the renderer uses the glyphs.
#[derive(Debug, Clone)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct DiffSlot {
    /// Piece text.
    pub text: String,
    /// Left of the piece's slot.
    pub x: f64,
    /// Top of the line.
    pub y: f64,
    /// sRGB color, alpha included.
    pub color: Color,
}

struct Metrics {
    char_width: f64,
    line_height: f64,
}

fn metrics(style: &TextStyle) -> Metrics {
    let cell = oxframe_text::layout("M", style);
    let rows = oxframe_text::layout("M\nM", style);
    let char_width = cell.width.max(0.01);
    let line_height = match rows.info.as_slice() {
        [a, b, ..] => (b.origin.y - a.origin.y).max(style.size),
        _ => style.size * style.line_height,
    };
    Metrics {
        char_width,
        line_height,
    }
}

/// How a code diff is drawn for one frame.
pub(crate) struct DiffView<'a> {
    /// Syntax colors.
    pub theme: &'a CodeTheme,
    /// Font.
    pub style: &'a TextStyle,
    /// Focused lines, 1-based; (0, 0) is everything.
    pub focus: Vec2,
    /// Alpha of unfocused lines.
    pub dim: f64,
    /// 0 is the old source, 1 is the new one.
    pub t: f64,
    /// Background behind inserted text.
    pub flash: bool,
}

/// Lays out the diff of `from` into `to`.
pub(crate) fn layout_diff(
    from: &str,
    to: &str,
    lang: Lang,
    view: DiffView<'_>,
) -> (DiffFrame, Vec<DiffSlot>) {
    let DiffView {
        theme,
        style,
        focus,
        dim,
        t,
        flash,
    } = view;
    let t = t.clamp(0.0, 1.0);
    let diff = oxframe_text::diff_code(from, to, lang);
    let m = metrics(style);
    let dim = dim.clamp(0.0, 1.0);
    let (fa, fb) = (focus.x.round(), focus.y.round());
    let all_focused = fa <= 0.0 && fb <= 0.0;
    let mapped = if all_focused {
        None
    } else {
        oxframe_text::map_focus(&diff, fa as usize, fb as usize)
    };
    let old_focused = |line: usize| all_focused || (line as f64 >= fa && line as f64 <= fb);
    let new_focused = |line: usize| match mapped {
        None if all_focused => true,
        None => false,
        Some((a, b)) => line >= a && line <= b,
    };

    let mut glyphs = Vec::new();
    let mut flashes = Vec::new();
    let mut slots = Vec::new();
    let mut y = 0.0;
    let mut width = 0.0_f64;
    for line in &diff.lines {
        let (pieces, old, new, line_factor) = match line {
            oxframe_text::LineEdit::Equal { old, new, line } => (
                pieces_of(line, lang, PieceKind::Equal),
                Some(*old),
                Some(*new),
                1.0,
            ),
            oxframe_text::LineEdit::Delete { old, line } => (
                pieces_of(line, lang, PieceKind::Delete),
                Some(*old),
                None,
                1.0 - t,
            ),
            oxframe_text::LineEdit::Insert { new, line } => (
                pieces_of(line, lang, PieceKind::Insert),
                None,
                Some(*new),
                t,
            ),
            oxframe_text::LineEdit::Change { old, new, pieces } => {
                (pieces.clone(), Some(*old), Some(*new), 1.0)
            }
        };
        let mut x = 0.0;
        for piece in pieces {
            let (text, token, kind) = match &piece {
                oxframe_text::PieceEdit::Equal(p) => (&p.text, p.token, PieceKind::Equal),
                oxframe_text::PieceEdit::Delete(p) => (&p.text, p.token, PieceKind::Delete),
                oxframe_text::PieceEdit::Insert(p) => (&p.text, p.token, PieceKind::Insert),
            };
            let natural = text.chars().count() as f64 * m.char_width;
            // A deleted or inserted token collapses sideways. A whole line
            // that appears or disappears also collapses its height.
            let x_scale = match kind {
                PieceKind::Equal => 1.0,
                PieceKind::Delete => 1.0 - t,
                PieceKind::Insert => t,
            };
            let focused = match kind {
                PieceKind::Insert => new.is_some_and(|n| new_focused(n + 1)),
                _ => old.is_some_and(|n| old_focused(n + 1)),
            };
            let fade = x_scale;
            let alpha = fade * if focused { 1.0 } else { dim };
            let mut color = theme.color(token);
            color = color.with_alpha((f64::from(color.a) * alpha) as f32);
            if flash && matches!(kind, PieceKind::Insert) && natural > 0.0 && x_scale > 0.0 {
                let a = (t * std::f64::consts::PI).sin() * 0.45;
                if a > 0.01 {
                    flashes.push(DiffFlash {
                        rect: kurbo::Rect::new(
                            x,
                            y,
                            x + natural * x_scale,
                            y + m.line_height * line_factor,
                        ),
                        color: theme.attribute.with_alpha(a as f32),
                    });
                }
            }
            if alpha > 0.0 && !text.trim().is_empty() {
                let laid = oxframe_text::layout(text, style);
                let tf = Affine::translate((x, y))
                    * Affine::scale_non_uniform(x_scale.max(0.0), line_factor.max(0.0));
                for g in &laid.glyphs {
                    let path = tf * g.clone();
                    if !path.elements().is_empty() {
                        glyphs.push(DiffGlyph { path, color });
                    }
                }
            }
            slots.push(DiffSlot {
                text: text.clone(),
                x,
                y,
                color,
            });
            x += natural * x_scale;
        }
        width = width.max(x);
        y += m.line_height * line_factor;
    }
    (
        DiffFrame {
            glyphs,
            flashes,
            size: (width, y),
        },
        slots,
    )
}

#[derive(Clone, Copy)]
enum PieceKind {
    Equal,
    Delete,
    Insert,
}

fn pieces_of(line: &str, lang: Lang, kind: PieceKind) -> Vec<oxframe_text::PieceEdit> {
    oxframe_text::line_pieces(line, lang)
        .into_iter()
        .map(|piece| match kind {
            PieceKind::Equal => oxframe_text::PieceEdit::Equal(piece),
            PieceKind::Delete => oxframe_text::PieceEdit::Delete(piece),
            PieceKind::Insert => oxframe_text::PieceEdit::Insert(piece),
        })
        .collect()
}

/// Glyphs of a morph frame: (path, alpha) in the text box centered at 0.
pub(crate) fn morph(
    from: &TextLayout,
    to: &TextLayout,
    t: f64,
) -> (Vec<(BezPath, f64)>, (f64, f64)) {
    let a: Vec<char> = from.info.iter().map(|g| g.ch).collect();
    let b: Vec<char> = to.info.iter().map(|g| g.ch).collect();
    // Longest common subsequence of characters: matched glyphs travel.
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut pairs = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    let off_from = Vec2::new(-from.width / 2.0, -from.height / 2.0);
    let off_to = Vec2::new(-to.width / 2.0, -to.height / 2.0);
    let mut out = Vec::new();
    let mut matched_a = vec![false; n];
    let mut matched_b = vec![false; m];
    for &(i, j) in &pairs {
        matched_a[i] = true;
        matched_b[j] = true;
        let pa = from.info[i].origin + off_from;
        let pb = to.info[j].origin + off_to;
        let shift = Lerp::lerp(&Point::ZERO, &(pb - pa).to_point(), t).to_vec2() + off_from;
        out.push((Affine::translate(shift) * from.glyphs[i].clone(), 1.0));
    }
    for (i, g) in from.glyphs.iter().enumerate() {
        if !matched_a[i] {
            out.push((Affine::translate(off_from) * g.clone(), 1.0 - t));
        }
    }
    for (j, g) in to.glyphs.iter().enumerate() {
        if !matched_b[j] {
            out.push((Affine::translate(off_to) * g.clone(), t));
        }
    }
    let size = (
        from.width.lerp(&to.width, t),
        from.height.lerp(&to.height, t),
    );
    (out, size)
}

#[cfg(test)]
mod diff_tests {
    use super::{CodeTheme, DiffSlot, DiffView, layout_diff};
    use kurbo::Vec2;
    use oxframe_text::{Lang, MONO, TextStyle};

    fn style() -> TextStyle {
        TextStyle {
            family: MONO.to_owned(),
            size: 32.0,
            line_height: 1.45,
            ..TextStyle::default()
        }
    }

    fn piece<'a>(slots: &'a [DiffSlot], text: &str) -> &'a DiffSlot {
        slots
            .iter()
            .find(|s| s.text == text)
            .unwrap_or_else(|| panic!("no piece {text:?} in {slots:?}"))
    }

    #[test]
    fn an_unchanged_line_slides_and_keeps_its_color() {
        let theme = CodeTheme::dark();
        let style = style();
        let mut ys = Vec::new();
        let mut colors = Vec::new();
        for t in [0.0, 0.5, 1.0] {
            let (_, slots) = layout_diff(
                "a\nkeep\n",
                "NEW\na\nkeep\n",
                Lang::Rust,
                DiffView {
                    theme: &theme,
                    style: &style,
                    focus: Vec2::new(2.0, 2.0),
                    dim: 0.3,
                    t,
                    flash: false,
                },
            );
            let keep = piece(&slots, "keep");
            ys.push(keep.y);
            colors.push((keep.color.r, keep.color.g, keep.color.b, keep.color.a));
            // Line 1 stays dim; the focused line rides down at full strength.
            assert!((piece(&slots, "a").color.a - 0.3).abs() < 0.02, "{t}");
        }
        assert!(ys[0] < ys[1] && ys[1] < ys[2], "{ys:?}");
        assert_eq!(colors[0], colors[1]);
        assert_eq!(colors[1], colors[2]);
        assert!((colors[0].3 - 1.0).abs() < 0.02, "{colors:?}");
    }

    #[test]
    fn a_shorter_token_pulls_its_neighbor_across() {
        let theme = CodeTheme::dark();
        let style = style();
        let at = |t| {
            layout_diff(
                "let value = 1;",
                "let v = 1;",
                Lang::Rust,
                DiffView {
                    theme: &theme,
                    style: &style,
                    focus: Vec2::ZERO,
                    dim: 0.3,
                    t,
                    flash: false,
                },
            )
            .1
        };
        let (a, b, c) = (at(0.0), at(0.5), at(1.0));
        let xs = [piece(&a, "1").x, piece(&b, "1").x, piece(&c, "1").x];
        assert!(xs[0] > xs[1] && xs[1] > xs[2], "{xs:?}");
        assert_eq!(piece(&a, "let").x, piece(&c, "let").x);
        assert_eq!(piece(&a, "let").color.r, piece(&b, "let").color.r);
        assert_eq!(piece(&a, "let").color.g, piece(&c, "let").color.g);
        assert!(piece(&a, "value").color.a > 0.9);
        assert!(piece(&c, "value").color.a < 0.01);
        assert!(piece(&a, "v").color.a < 0.01);
        assert!(piece(&c, "v").color.a > 0.9);
    }
}
