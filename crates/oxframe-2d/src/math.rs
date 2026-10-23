//! A formula set by Typst: `Math::new("e^(i pi) + 1 = 0")` (feature `math`).
//! Every glyph and rule is a path, so a formula can be written on glyph by
//! glyph; one signal colors all of them.

use std::ops::Deref;

use oxframe_core::{Color, Signal};
use oxframe_text::TextStyle;

use crate::builder::{BaseProps, NodeBuilder, SvgBuilder};
use crate::graph::{NodeId, SceneGraph};
use crate::handles::{Node, Path, Svg};
use crate::theme::{Ink, IntoPaint};

/// Builder for a [`Math`] node: `Math::new("a^2 + b^2 = c^2").size(64.0)`.
#[derive(Debug)]
#[must_use = "a builder does nothing until it is passed to `add`"]
pub struct MathBuilder {
    formula: String,
    display: bool,
    size: f64,
    color: Option<Color>,
    svg: SvgBuilder,
}

impl MathBuilder {
    /// Font size in pixels, as for [`Txt`](crate::Txt). Default: the same as
    /// `Txt`.
    pub fn size(mut self, v: f64) -> Self {
        self.size = v;
        self
    }

    /// Color of every glyph and rule. Default: the theme's text color.
    pub fn color(mut self, v: impl IntoPaint) -> Self {
        self.color = Some(v.into_paint());
        self
    }

    /// Sets the formula on its own line, as `$ … $` in Typst: sums and
    /// integrals get their limits above and below, fractions are full size.
    /// The default is inline math, as in running text.
    pub fn display(mut self) -> Self {
        self.display = true;
        self
    }
}

impl NodeBuilder for MathBuilder {
    type Handle = Math;

    fn base(&mut self) -> &mut BaseProps {
        self.svg.base()
    }

    fn build(self, g: &SceneGraph, parent: NodeId) -> Math {
        let set = oxframe_math::to_svg(&self.formula, self.display)
            .unwrap_or_else(|e| panic!("Math {:?}: {e}", self.formula));
        // Typst lays out at LAYOUT_PT, one em; scale that em to `size` px.
        let k = self.size / oxframe_math::LAYOUT_PT;
        let svg = self
            .svg
            .src(set.svg.as_bytes())
            .width(set.width * k)
            .height(set.height * k)
            .build(g, parent);
        let color = Signal::new(self.color.unwrap_or_else(|| Ink::Text.into_paint()));
        for path in g.descendant_paths(svg.id()) {
            let n = g.node(path);
            if n.fill.peek().to_rgba8()[3] > 0 {
                let c = color.clone();
                n.fill.bind(move || c.get());
            }
            if n.stroke.peek().to_rgba8()[3] > 0 {
                let c = color.clone();
                n.stroke.bind(move || c.get());
            }
        }
        Math {
            svg,
            color,
            formula: self.formula,
        }
    }
}

/// Handle to a formula. Derefs to [`Node`]; [`paths`](Self::paths) are the
/// glyphs and rules in reading order.
#[derive(Debug, Clone)]
pub struct Math {
    svg: Svg,
    color: Signal<Color>,
    formula: String,
}

impl Math {
    /// A formula in [Typst math syntax](https://typst.app/docs/reference/math/),
    /// without the `$`: `"x^2"`, `"sqrt(a + b)"`, `"sum_(k=1)^n k"`,
    /// `"(a + b) / c"`, `"alpha, beta, pi"`.
    ///
    /// # Panics
    ///
    /// When the node is added, if Typst cannot set the formula (an unknown
    /// symbol, unbalanced brackets); the message is Typst's.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(formula: impl Into<String>) -> MathBuilder {
        MathBuilder {
            formula: formula.into(),
            display: false,
            size: TextStyle::default().size,
            color: None,
            svg: Svg::new(),
        }
    }

    /// The untyped node handle.
    pub fn node(&self) -> Node {
        *self.svg
    }

    /// The color of every glyph and rule.
    pub fn color(&self) -> Signal<Color> {
        self.color.clone()
    }

    /// Glyphs and rules (fraction bars, roots) in reading order, for
    /// `stagger` and `draw_on`.
    pub fn paths(&self) -> Vec<Path> {
        self.svg.paths()
    }

    /// The source the node was made from.
    pub fn formula(&self) -> &str {
        &self.formula
    }
}

impl Deref for Math {
    type Target = Node;
    fn deref(&self) -> &Node {
        &self.svg
    }
}

impl From<Math> for Node {
    fn from(h: Math) -> Node {
        *h.svg
    }
}

impl From<&Math> for Node {
    fn from(h: &Math) -> Node {
        *h.svg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::View;

    #[test]
    fn a_formula_is_sized_by_its_font_size_and_colored_by_one_signal() {
        SceneGraph::new(800, 400).enter(|| {
            let small = View::current().add(Math::new("x^2 + y^2 = r^2").size(32.0));
            let big = View::current().add(
                Math::new("x^2 + y^2 = r^2")
                    .size(64.0)
                    .color(Color::rgb(1.0, 0.0, 0.0)),
            );
            let (sw, sh) = small.size();
            let (bw, bh) = big.size();
            assert!((bw / sw - 2.0).abs() < 1e-6 && (bh / sh - 2.0).abs() < 1e-6);
            // Inline math keeps to the line: the box is ascender to descender.
            assert!(
                (bh - 64.0).abs() < 1.0,
                "one em, ascender to descender ({bh})"
            );
            let paths = big.paths();
            assert!(paths.len() >= 8, "one path per glyph ({})", paths.len());
            assert_eq!(paths[0].fill().get(), Color::rgb(1.0, 0.0, 0.0));
            big.color().set(Color::WHITE);
            assert!(paths.iter().all(|p| p.fill().get() == Color::WHITE));
            assert_eq!(big.formula(), "x^2 + y^2 = r^2");
        });
    }

    #[test]
    fn display_math_sets_limits_above_and_below() {
        SceneGraph::new(800, 400).enter(|| {
            let inline = View::current().add(Math::new("sum_(k=1)^n k"));
            let display = View::current().add(Math::new("sum_(k=1)^n k").display());
            assert!(display.size().1 > inline.size().1);
        });
    }

    #[test]
    #[should_panic(expected = "Math \"#nosuch\"")]
    fn a_bad_formula_names_itself() {
        SceneGraph::new(200, 200).enter(|| {
            let _ = View::current().add(Math::new("#nosuch"));
        });
    }
}
