//! A frame's draw list as a pass graph.
//!
//! Vector items stay in one pass. A layer (opacity on a group, a blend, a
//! blur, a shadow, a clip) becomes a composite pass that runs after the
//! vector pass of its contents. Shader and effect passes are not built yet.

use std::hash::{Hash, Hasher};

use kurbo::PathEl;
use oxframe_core::BlendMode;

use crate::draw::{DrawItem, Paint, Primitive, StrokeStyle};
use crate::graph::NodeId;

/// What a pass draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassKind {
    /// Fills, strokes and images, in paint order, into the current target.
    Vector,
    /// A layer composited over the target after its contents.
    Composite,
}

impl PassKind {
    /// `"vector"` or `"composite"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Vector => "vector",
            Self::Composite => "composite",
        }
    }
}

/// One pass in topological order: contents before the composite that uses them.
#[derive(Debug, Clone, PartialEq)]
pub struct Pass {
    /// Vector or composite.
    pub kind: PassKind,
    /// Node that opened the layer. The root vector pass uses the view.
    pub node: NodeId,
    /// Draw items this pass owns. A composite counts the items inside it.
    pub items: u32,
    /// Nesting. 0 is the frame.
    pub depth: u32,
    /// Layer opacity. 1 for a vector pass.
    pub opacity: f64,
    /// Layer blend. `"normal"` for a vector pass.
    pub blend: &'static str,
    /// The layer blurs, clips or casts a shadow.
    pub filtered: bool,
}

/// Passes for `items`, in the order they must run.
///
/// A list with no layer is exactly one vector pass.
pub fn pass_graph(items: &[DrawItem]) -> Vec<Pass> {
    let mut stack = vec![Scope::frame()];
    for item in items {
        match &item.prim {
            Primitive::PushLayer(layer) => {
                flush(&mut stack);
                stack.push(Scope {
                    node: item.node,
                    items: 0,
                    run: 0,
                    depth: stack.len() as u32,
                    layer: Some(LayerInfo {
                        opacity: layer.opacity,
                        blend: blend_name(layer.blend),
                        filtered: layer.has_filter() || layer.clip.is_some(),
                    }),
                    passes: Vec::new(),
                });
            }
            Primitive::PopLayer => {
                flush(&mut stack);
                let Some(mut scope) = stack.pop() else {
                    continue;
                };
                if stack.is_empty() {
                    stack.push(scope);
                    continue;
                }
                let info = scope.layer.unwrap_or(LayerInfo {
                    opacity: 1.0,
                    blend: "normal",
                    filtered: false,
                });
                let parent = stack.last_mut().expect("frame scope");
                parent.items += scope.items;
                parent.passes.append(&mut scope.passes);
                parent.passes.push(Pass {
                    kind: PassKind::Composite,
                    node: scope.node,
                    items: scope.items,
                    depth: scope.depth - 1,
                    opacity: info.opacity,
                    blend: info.blend,
                    filtered: info.filtered,
                });
            }
            _ => {
                let scope = stack.last_mut().expect("frame scope");
                scope.items += 1;
                scope.run += 1;
            }
        }
    }
    flush(&mut stack);
    let mut passes = stack.pop().map(|s| s.passes).unwrap_or_default();
    if passes.is_empty() {
        passes.push(Pass {
            kind: PassKind::Vector,
            node: NodeId(0),
            items: 0,
            depth: 0,
            opacity: 1.0,
            blend: "normal",
            filtered: false,
        });
    }
    passes
}

struct LayerInfo {
    opacity: f64,
    blend: &'static str,
    filtered: bool,
}

struct Scope {
    node: NodeId,
    items: u32,
    run: u32,
    depth: u32,
    layer: Option<LayerInfo>,
    passes: Vec<Pass>,
}

impl Scope {
    fn frame() -> Self {
        Self {
            node: NodeId(0),
            items: 0,
            run: 0,
            depth: 0,
            layer: None,
            passes: Vec::new(),
        }
    }
}

fn flush(stack: &mut [Scope]) {
    let Some(scope) = stack.last_mut() else {
        return;
    };
    if scope.run == 0 {
        return;
    }
    let run = scope.run;
    scope.run = 0;
    scope.passes.push(Pass {
        kind: PassKind::Vector,
        node: scope.node,
        items: run,
        depth: scope.depth,
        opacity: 1.0,
        blend: "normal",
        filtered: false,
    });
}

fn blend_name(blend: BlendMode) -> &'static str {
    match blend {
        BlendMode::Normal => "normal",
        BlendMode::Multiply => "multiply",
        BlendMode::Screen => "screen",
        BlendMode::Overlay => "overlay",
        BlendMode::Darken => "darken",
        BlendMode::Lighten => "lighten",
        BlendMode::ColorDodge => "color_dodge",
        BlendMode::ColorBurn => "color_burn",
        BlendMode::HardLight => "hard_light",
        BlendMode::SoftLight => "soft_light",
        BlendMode::Difference => "difference",
        BlendMode::Exclusion => "exclusion",
        BlendMode::Hue => "hue",
        BlendMode::Saturation => "saturation",
        BlendMode::Color => "color",
        BlendMode::Luminosity => "luminosity",
    }
}

/// Identity of a layer slice, including its `PushLayer` at `start` through the
/// matching `PopLayer`. Used to reuse a layer whose vectors did not change.
pub fn layer_fingerprint(items: &[DrawItem], start: usize) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut depth = 0usize;
    for item in &items[start..] {
        item.node.0.hash(&mut hasher);
        for c in item.transform.as_coeffs() {
            c.to_bits().hash(&mut hasher);
        }
        match &item.prim {
            Primitive::PushLayer(layer) => {
                depth += 1;
                hash_layer(&mut hasher, layer);
            }
            Primitive::PopLayer => {
                2u8.hash(&mut hasher);
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            Primitive::Fill { path, paint } => {
                3u8.hash(&mut hasher);
                hash_path(&mut hasher, path);
                hash_paint(&mut hasher, paint);
            }
            Primitive::Stroke { path, paint, style } => {
                4u8.hash(&mut hasher);
                hash_path(&mut hasher, path);
                hash_paint(&mut hasher, paint);
                hash_stroke(&mut hasher, style);
            }
            Primitive::Image {
                image,
                rect,
                opacity,
            } => {
                5u8.hash(&mut hasher);
                image.id.hash(&mut hasher);
                rect.x0.to_bits().hash(&mut hasher);
                rect.y0.to_bits().hash(&mut hasher);
                rect.x1.to_bits().hash(&mut hasher);
                rect.y1.to_bits().hash(&mut hasher);
                opacity.to_bits().hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

fn hash_layer(hasher: &mut impl Hasher, layer: &crate::draw::Layer) {
    1u8.hash(hasher);
    layer.opacity.to_bits().hash(hasher);
    blend_name(layer.blend).hash(hasher);
    layer.blur.to_bits().hash(hasher);
    if let Some(path) = &layer.clip {
        hash_path(hasher, path);
    }
    if let Some(shadow) = &layer.shadow {
        hash_color(hasher, shadow.color);
        shadow.blur.to_bits().hash(hasher);
        shadow.offset.x.to_bits().hash(hasher);
        shadow.offset.y.to_bits().hash(hasher);
    }
}

fn hash_path(hasher: &mut impl Hasher, path: &kurbo::BezPath) {
    for el in path.elements() {
        match el {
            PathEl::MoveTo(p) => {
                0u8.hash(hasher);
                hash_point(hasher, *p);
            }
            PathEl::LineTo(p) => {
                1u8.hash(hasher);
                hash_point(hasher, *p);
            }
            PathEl::QuadTo(a, b) => {
                2u8.hash(hasher);
                hash_point(hasher, *a);
                hash_point(hasher, *b);
            }
            PathEl::CurveTo(a, b, c) => {
                3u8.hash(hasher);
                hash_point(hasher, *a);
                hash_point(hasher, *b);
                hash_point(hasher, *c);
            }
            PathEl::ClosePath => 4u8.hash(hasher),
        }
    }
}

fn hash_point(hasher: &mut impl Hasher, p: kurbo::Point) {
    p.x.to_bits().hash(hasher);
    p.y.to_bits().hash(hasher);
}

fn hash_paint(hasher: &mut impl Hasher, paint: &Paint) {
    match paint {
        Paint::Solid(c) => {
            0u8.hash(hasher);
            hash_color(hasher, *c);
        }
        Paint::Gradient(g) => {
            1u8.hash(hasher);
            match g.kind {
                oxframe_core::GradientKind::Linear { from, to } => {
                    0u8.hash(hasher);
                    hash_point(hasher, from);
                    hash_point(hasher, to);
                }
                oxframe_core::GradientKind::Radial { center, radius } => {
                    1u8.hash(hasher);
                    hash_point(hasher, center);
                    radius.to_bits().hash(hasher);
                }
            }
            for (offset, color) in &g.stops {
                offset.to_bits().hash(hasher);
                hash_color(hasher, *color);
            }
        }
    }
}

fn hash_color(hasher: &mut impl Hasher, c: oxframe_core::Color) {
    c.r.to_bits().hash(hasher);
    c.g.to_bits().hash(hasher);
    c.b.to_bits().hash(hasher);
    c.a.to_bits().hash(hasher);
}

fn hash_stroke(hasher: &mut impl Hasher, style: &StrokeStyle) {
    style.width.to_bits().hash(hasher);
    cap_tag(style.cap).hash(hasher);
    join_tag(style.join).hash(hasher);
    style.dash_offset.to_bits().hash(hasher);
    for d in &style.dash {
        d.to_bits().hash(hasher);
    }
}

fn cap_tag(cap: crate::draw::LineCap) -> u8 {
    use crate::draw::LineCap::{Butt, Round, Square};
    match cap {
        Butt => 0,
        Round => 1,
        Square => 2,
    }
}

fn join_tag(join: crate::draw::LineJoin) -> u8 {
    use crate::draw::LineJoin::{Bevel, Miter, Round};
    match join {
        Miter => 0,
        Round => 1,
        Bevel => 2,
    }
}

#[cfg(test)]
mod tests {
    use oxframe_core::Color;

    use super::*;
    use crate::graph::SceneGraph;
    use crate::{NodeBuilder, Rect, View};

    #[test]
    fn a_scene_without_effects_is_one_vector_pass() {
        let graph = SceneGraph::new(80, 80);
        graph.enter(|| {
            View::current().add(Rect::new().size(20.0).fill(Color::WHITE));
            View::current().add(Rect::new().size(10.0).x(15.0).fill(Color::BLACK));
        });
        let passes = pass_graph(&graph.draw_list());
        assert_eq!(passes.len(), 1);
        assert_eq!(passes[0].kind, PassKind::Vector);
        assert!(passes[0].items >= 2);
    }

    #[test]
    fn a_blur_is_a_composite_after_its_contents() {
        let graph = SceneGraph::new(80, 80);
        graph.enter(|| {
            View::current().add(
                Rect::new()
                    .size(20.0)
                    .fill(Color::WHITE)
                    .blur(2.0)
                    .name("soft"),
            );
        });
        let passes = pass_graph(&graph.draw_list());
        assert!(
            passes.iter().any(|p| p.kind == PassKind::Vector),
            "{passes:?}"
        );
        let composite = passes
            .iter()
            .find(|p| p.kind == PassKind::Composite)
            .expect("blur opens a layer");
        assert!(composite.filtered);
        let vector_at = passes.iter().position(|p| p.kind == PassKind::Vector);
        let composite_at = passes.iter().position(|p| p.kind == PassKind::Composite);
        assert!(vector_at < composite_at);
    }
}
