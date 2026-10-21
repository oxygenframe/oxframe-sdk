//! An icon from a set: an SVG on a 24-unit grid whose color and line width
//! are signals. `icon!("lucide:server")` in `oxframe` (feature `icons`)
//! makes the builder; [`Icon::from_svg`] is what it expands to.

use std::ops::Deref;

use oxframe_core::{Color, Signal};

use crate::builder::{BaseProps, NodeBuilder, SvgBuilder};
use crate::graph::{NodeId, SceneGraph};
use crate::handles::{Node, Path, Svg};
use crate::theme::{Ink, IntoPaint};

/// How an icon is painted: Lucide and Tabler outlines are strokes, filled
/// variants and brand logos are fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconStyle {
    /// Outlines, 2 units wide on the 24-unit grid.
    Stroke,
    /// Filled shapes.
    Fill,
}

/// Builder for an [`Icon`]: `icon!("lucide:server").size(64.0).color(Accent)`.
#[derive(Debug)]
#[must_use = "a builder does nothing until it is passed to `add`"]
pub struct IconBuilder {
    svg: SvgBuilder,
    style: IconStyle,
    size: f64,
    color: Option<Color>,
    stroke_width: f64,
}

impl IconBuilder {
    /// Width and height in pixels. Default 48.
    pub fn size(mut self, v: f64) -> Self {
        self.size = v;
        self
    }

    /// Color of the strokes or fills. Default: the theme's text color.
    pub fn color(mut self, v: impl IntoPaint) -> Self {
        self.color = Some(v.into_paint());
        self
    }

    /// Line width on the 24-unit grid, scaled with the size. Default 2;
    /// a fill icon has no lines.
    pub fn stroke_width(mut self, v: f64) -> Self {
        self.stroke_width = v;
        self
    }
}

impl NodeBuilder for IconBuilder {
    type Handle = Icon;

    fn base(&mut self) -> &mut BaseProps {
        self.svg.base()
    }

    fn build(self, g: &SceneGraph, parent: NodeId) -> Icon {
        let color = Signal::new(self.color.unwrap_or_else(|| Ink::Text.into_paint()));
        let stroke_width = Signal::new(self.stroke_width);
        let style = self.style;
        let svg = self.svg.width(self.size).height(self.size).build(g, parent);
        for path in g.descendant_paths(svg.id()) {
            let n = g.node(path);
            if n.stroke.peek().to_rgba8()[3] > 0 {
                let c = color.clone();
                n.stroke.bind(move || c.get());
                // Widths come scaled to the size; keep each path's own ratio.
                let base = n.line_width.peek() / 2.0;
                let w = stroke_width.clone();
                n.line_width.bind(move || base * w.get());
            }
            if n.fill.peek().to_rgba8()[3] > 0 {
                let c = color.clone();
                n.fill.bind(move || c.get());
            }
        }
        Icon {
            svg,
            style,
            color,
            stroke_width,
        }
    }
}

/// Handle to an icon. Derefs to [`Node`]; [`color`](Self::color) and
/// [`stroke_width`](Self::stroke_width) animate every path of it.
#[derive(Debug, Clone)]
pub struct Icon {
    svg: Svg,
    style: IconStyle,
    color: Signal<Color>,
    stroke_width: Signal<f64>,
}

impl Icon {
    /// A builder for an icon drawn by `svg`, a 24 × 24 SVG. Prefer `icon!`.
    ///
    /// # Panics
    ///
    /// When the icon is added, if `svg` does not parse.
    pub fn from_svg(svg: &[u8], style: IconStyle) -> IconBuilder {
        IconBuilder {
            svg: Svg::new().src(svg),
            style,
            size: 48.0,
            color: None,
            stroke_width: 2.0,
        }
    }

    /// The untyped node handle.
    pub fn node(&self) -> Node {
        *self.svg
    }

    /// The color of every stroke or fill.
    pub fn color(&self) -> Signal<Color> {
        self.color.clone()
    }

    /// The line width on the 24-unit grid.
    pub fn stroke_width(&self) -> Signal<f64> {
        self.stroke_width.clone()
    }

    /// Every path of the icon, in paint order.
    pub fn paths(&self) -> Vec<Path> {
        self.svg.paths()
    }

    /// Whether the icon is outlines or fills.
    pub fn style(&self) -> IconStyle {
        self.style
    }
}

impl Deref for Icon {
    type Target = Node;
    fn deref(&self) -> &Node {
        &self.svg
    }
}

impl From<Icon> for Node {
    fn from(h: Icon) -> Node {
        *h.svg
    }
}

impl From<&Icon> for Node {
    fn from(h: &Icon) -> Node {
        *h.svg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::View;
    use crate::graph::SceneGraph;

    const ARROW: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></svg>"##;
    const DOT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="#000"><circle cx="12" cy="12" r="8"/></svg>"##;

    #[test]
    fn stroke_icon_follows_its_signals() {
        SceneGraph::new(200, 200).enter(|| {
            let icon = View::current().add(
                Icon::from_svg(ARROW.as_bytes(), IconStyle::Stroke)
                    .size(96.0)
                    .color(Color::rgb(1.0, 0.0, 0.0)),
            );
            assert_eq!(icon.size(), (96.0, 96.0));
            let paths = Svg::from_id(icon.id()).paths();
            assert_eq!(paths.len(), 2);
            assert_eq!(paths[0].stroke().get(), Color::rgb(1.0, 0.0, 0.0));
            assert!((paths[0].line_width().get() - 8.0).abs() < 1e-6);
            icon.color().set(Color::WHITE);
            icon.stroke_width().set(1.0);
            assert_eq!(paths[1].stroke().get(), Color::WHITE);
            assert!((paths[1].line_width().get() - 4.0).abs() < 1e-6);
            assert_eq!(paths[1].fill().get().to_rgba8()[3], 0);
            assert_eq!(icon.style(), IconStyle::Stroke);
        });
    }

    #[test]
    fn fill_icon_colors_its_fill() {
        SceneGraph::new(200, 200).enter(|| {
            let icon = View::current()
                .add(Icon::from_svg(DOT.as_bytes(), IconStyle::Fill).color(Color::WHITE));
            let paths = Svg::from_id(icon.id()).paths();
            assert_eq!(paths[0].fill().get(), Color::WHITE);
            assert_eq!(icon.style(), IconStyle::Fill);
        });
    }
}
