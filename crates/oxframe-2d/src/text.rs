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
