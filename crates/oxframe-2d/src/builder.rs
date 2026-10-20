use kurbo::{BezPath, Vec2};
use oxframe_core::{BlendMode, Color, Gradient, Signal};
use oxframe_text::TextStyle;

use crate::draw::{LineCap, LineJoin};
use crate::graph::{Direction, Justify, LayoutAlign, NodeId, NodeKind, SceneGraph, with_graph};
use crate::handles::{
    Camera, Circle, Code, Grid, Group, Img, Layout, Line, Path, Polygon, Rect, Star, Txt,
};
use crate::image::ImageSource;
use crate::place::{self, Axis, Edge, Place, Side, Span};
use crate::text::{CodeTheme, RichSpan};
use crate::theme::{self, Corner, IntoPaint, Len, Step, TypeRole};

/// Initial values shared by every node; set through [`NodeBuilder`] methods.
#[doc(hidden)]
#[derive(Debug)]
pub struct BaseProps {
    pub name: Option<String>,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub scale: Vec2,
    pub opacity: f64,
    pub fill: Color,
    /// Set when `fill` was chosen explicitly. Text leaves this false and
    /// takes the theme's text color at build time.
    pub fill_explicit: bool,
    pub stroke: Color,
    pub line_width: f64,
    pub start: f64,
    pub end: f64,
    pub fill_gradient: Option<Gradient>,
    pub stroke_gradient: Option<Gradient>,
    pub dash: Vec<f64>,
    pub dash_offset: f64,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub blur: f64,
    pub shadow_color: Color,
    pub shadow_blur: f64,
    pub shadow_offset: Vec2,
    pub clip: bool,
    pub blend: BlendMode,
    pub z_index: f64,
    pub anchor: Vec2,
    pub grow: f64,
    pub shrink: f64,
    pub basis: Option<f64>,
    pub margin: f64,
    pub align_self: Option<LayoutAlign>,
    pub children: Vec<ChildBuilder>,
    /// Resolved every frame. Set by `next_to`, `align_to`, `to_edge`, `center_in`.
    pub place: Option<Place>,
}

impl Default for BaseProps {
    fn default() -> Self {
        Self {
            name: None,
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            scale: Vec2::new(1.0, 1.0),
            opacity: 1.0,
            fill: Color::TRANSPARENT,
            fill_explicit: false,
            stroke: Color::TRANSPARENT,
            line_width: 0.0,
            start: 0.0,
            end: 1.0,
            fill_gradient: None,
            stroke_gradient: None,
            dash: Vec::new(),
            dash_offset: 0.0,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            blur: 0.0,
            shadow_color: Color::TRANSPARENT,
            shadow_blur: 0.0,
            shadow_offset: Vec2::ZERO,
            clip: false,
            blend: BlendMode::Normal,
            z_index: 0.0,
            anchor: Vec2::ZERO,
            grow: 0.0,
            shrink: 0.0,
            basis: None,
            margin: 0.0,
            align_self: None,
            children: Vec::new(),
            place: None,
        }
    }
}

/// A type-erased child added with [`NodeBuilder::child`].
type BuildFn = Box<dyn FnOnce(&SceneGraph, NodeId)>;

/// A child declared with [`NodeBuilder::child`], built together with its parent.
pub struct ChildBuilder(BuildFn);

impl std::fmt::Debug for ChildBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChildBuilder")
    }
}

/// Describes a node before it is added to the scene.
pub trait NodeBuilder: Sized + 'static {
    /// Handle returned by [`Node::add`](crate::Node::add).
    type Handle;

    #[doc(hidden)]
    fn base(&mut self) -> &mut BaseProps;

    #[doc(hidden)]
    fn build(self, graph: &SceneGraph, parent: NodeId) -> Self::Handle;

    /// Name shown in the editor and in `oxframe check` / `oxframe inspect`.
    /// Without it, the node is labeled by the `add` call site (`src/main.rs:42`).
    fn name(mut self, v: impl Into<String>) -> Self {
        self.base().name = Some(v.into());
        self
    }
    /// Horizontal position; 0 is the parent's center, positive is right.
    fn x(mut self, v: f64) -> Self {
        self.base().x = v;
        self
    }
    /// Vertical position; 0 is the parent's center, positive is down.
    fn y(mut self, v: f64) -> Self {
        self.base().y = v;
        self
    }
    /// Sets `x` and `y` together, e.g. `.position((100.0, -50.0))`.
    fn position(mut self, v: impl Into<Vec2>) -> Self {
        let v = v.into();
        let b = self.base();
        (b.x, b.y) = (v.x, v.y);
        self
    }
    /// Degrees, clockwise.
    fn rotation(mut self, v: f64) -> Self {
        self.base().rotation = v;
        self
    }
    /// Uniform scale; 1 is the natural size.
    fn scale(mut self, v: f64) -> Self {
        self.base().scale = Vec2::new(v, v);
        self
    }
    /// Separate horizontal and vertical scale.
    fn scale_xy(mut self, v: impl Into<Vec2>) -> Self {
        self.base().scale = v.into();
        self
    }
    /// Opacity of the node and its children, 0 (invisible) to 1.
    fn opacity(mut self, v: f64) -> Self {
        self.base().opacity = v;
        self
    }
    /// Fill color: a [`Color`](oxframe_core::Color) or a theme role such as `Accent`.
    /// Default: transparent, except text, which uses the theme's text color.
    fn fill(mut self, v: impl IntoPaint) -> Self {
        let b = self.base();
        b.fill = v.into_paint();
        b.fill_explicit = true;
        self
    }
    /// Outline color; also set `line_width`. A theme role works the same as a color.
    /// Default: transparent.
    fn stroke(mut self, v: impl IntoPaint) -> Self {
        self.base().stroke = v.into_paint();
        self
    }
    /// Outline width in pixels. Default: 0 (no outline).
    fn line_width(mut self, v: f64) -> Self {
        self.base().line_width = v;
        self
    }
    /// Start of the drawn part of the outline, fraction of its length.
    fn start(mut self, v: f64) -> Self {
        self.base().start = v;
        self
    }
    /// End of the drawn part of the outline; animate 0 → 1 to draw it on.
    fn end(mut self, v: f64) -> Self {
        self.base().end = v;
        self
    }
    /// Gradient fill (local coordinates, origin at the node center);
    /// overrides `fill`.
    fn fill_gradient(mut self, g: Gradient) -> Self {
        self.base().fill_gradient = Some(g);
        self
    }
    /// Gradient outline; overrides `stroke`.
    fn stroke_gradient(mut self, g: Gradient) -> Self {
        self.base().stroke_gradient = Some(g);
        self
    }
    /// Dash pattern: on, off, on, … lengths. Animate `dash_offset` for
    /// marching ants.
    fn dash(mut self, pattern: impl Into<Vec<f64>>) -> Self {
        self.base().dash = pattern.into();
        self
    }
    /// Shifts the dash pattern along the outline.
    fn dash_offset(mut self, v: f64) -> Self {
        self.base().dash_offset = v;
        self
    }
    /// Shape of outline ends. Default: round.
    fn line_cap(mut self, v: LineCap) -> Self {
        self.base().line_cap = v;
        self
    }
    /// Shape of outline corners. Default: round.
    fn line_join(mut self, v: LineJoin) -> Self {
        self.base().line_join = v;
        self
    }
    /// Gaussian blur of the node and its children (standard deviation, in
    /// the node's units).
    fn blur(mut self, v: f64) -> Self {
        self.base().blur = v;
        self
    }
    /// Drop shadow under the node and its children.
    fn shadow(mut self, color: impl IntoPaint, blur: f64, offset: impl Into<Vec2>) -> Self {
        let b = self.base();
        (b.shadow_color, b.shadow_blur, b.shadow_offset) =
            (color.into_paint(), blur, offset.into());
        self
    }
    /// The theme's shadow for this step (`Xs` and `Sm` are the small one, `Xl` the large).
    fn elevation(self, step: Step) -> Self {
        let s = theme::current().shadow(step);
        self.shadow(s.color, s.blur, s.offset)
    }
    /// A shadow without offset.
    fn glow(self, color: impl IntoPaint, blur: f64) -> Self {
        self.shadow(color, blur, Vec2::ZERO)
    }
    /// Clip children to this node's outline (or box).
    fn clip(mut self, v: bool) -> Self {
        self.base().clip = v;
        self
    }
    /// How the node is composited onto what is below it.
    fn blend(mut self, v: BlendMode) -> Self {
        self.base().blend = v;
        self
    }
    /// Paint order among siblings; higher is on top.
    fn z_index(mut self, v: f64) -> Self {
        self.base().z_index = v;
        self
    }
    /// Pivot and placement point: (-1, -1) top-left … (1, 1) bottom-right.
    fn anchor(mut self, v: impl Into<Vec2>) -> Self {
        self.base().anchor = v.into();
        self
    }
    /// Inside a `Layout`: share of the free space this node takes.
    fn grow(mut self, v: f64) -> Self {
        self.base().grow = v;
        self
    }
    /// Inside a `Layout`: how much this node gives up when space is short.
    fn shrink(mut self, v: f64) -> Self {
        self.base().shrink = v;
        self
    }
    /// Inside a `Layout`: main-axis size before growing/shrinking.
    fn basis(mut self, v: f64) -> Self {
        self.base().basis = Some(v);
        self
    }
    /// Inside a `Layout`: space around this node, in pixels or a theme step (`.margin(Md)`).
    fn margin(mut self, v: impl Len) -> Self {
        self.base().margin = v.px();
        self
    }
    /// Inside a `Layout`: overrides the layout's cross-axis alignment.
    fn align_self(mut self, v: LayoutAlign) -> Self {
        self.base().align_self = Some(v);
        self
    }
    /// Sits just outside `other`, on `side`. The gap is the theme's `Md`
    /// step. Recomputed every frame, so this node follows `other`.
    fn next_to(mut self, other: impl Into<crate::Node>, side: Side) -> Self {
        self.base().place = Some(Place::next_to(other.into().id(), side));
        self
    }
    /// Lines this node's `edge` up with the same edge of `other`.
    fn align_to(mut self, other: impl Into<crate::Node>, edge: Edge) -> Self {
        self.base().place = Some(Place::align_to(other.into().id(), edge));
        self
    }
    /// Pins one edge to the safe area (not the raw frame edge). The other
    /// axis stays at `x` or `y`.
    fn to_edge(mut self, edge: Edge) -> Self {
        self.base().place = Some(Place::to_edge(edge));
        self
    }
    /// Same center as `other`. Pass the view to center in the frame.
    fn center_in(mut self, other: impl Into<crate::Node>) -> Self {
        self.base().place = Some(Place::center_in(other.into().id()));
        self
    }
    /// Adds a child created together with this node. Use `Node::add` when
    /// you need the child's handle.
    fn child<C: NodeBuilder>(mut self, child: C) -> Self {
        self.base()
            .children
            .push(ChildBuilder(Box::new(move |g, parent| {
                child.build(g, parent);
            })));
        self
    }
}

pub(crate) fn add<B: NodeBuilder>(parent: NodeId, builder: B) -> B::Handle {
    with_graph(|g| builder.build(g, parent))
}

/// Inserts the node and then its declared children.
fn insert(graph: &SceneGraph, parent: NodeId, kind: NodeKind, mut base: BaseProps) -> NodeId {
    let children = std::mem::take(&mut base.children);
    let id = graph.insert(Some(parent), kind, base);
    for c in children {
        (c.0)(graph, id);
    }
    id
}

fn sig<T: Clone + 'static>(graph: &SceneGraph, kind: &str, field: &str, v: T) -> Signal<T> {
    Signal::new(v).with_name(format!("{kind}#{}.{field}", graph.next_id()))
}

macro_rules! builder {
    (
        $(#[$doc:meta])*
        $builder:ident => $handle:ident, kind $kind:literal, defaults { $($bf:ident: $bv:expr),* $(,)? },
        fields { $($field:ident: $ty:ty = $default:expr),* $(,)? }
    ) => {
        $(#[$doc])*
        #[doc = ""]
        #[doc = concat!("Builder for a [`", stringify!($handle), "`] node: set the initial values, then")]
        #[doc = "pass it to `add`."]
        #[derive(Debug)]
        #[must_use = "a builder does nothing until it is passed to `add`"]
        pub struct $builder {
            base: BaseProps,
            $(pub(crate) $field: $ty,)*
        }

        impl Default for $builder {
            fn default() -> Self {
                Self {
                    base: BaseProps { $($bf: $bv,)* ..Default::default() },
                    $($field: $default,)*
                }
            }
        }

        impl $builder {
            #[allow(dead_code)]
            const KIND: &'static str = $kind;
        }
    };
}

macro_rules! setters {
    ($builder:ident { $($(#[$doc:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        impl $builder {
            $(
                $(#[$doc])*
                #[doc = ""]
                #[doc = concat!("Sets the initial `", stringify!($field), "`.")]
                pub fn $field(mut self, v: impl Into<$ty>) -> Self {
                    self.$field = v.into();
                    self
                }
            )*
        }
    };
}

macro_rules! base_accessor {
    () => {
        fn base(&mut self) -> &mut BaseProps {
            &mut self.base
        }
    };
}

// ------------------------------------------------------------------ Group

builder!(GroupBuilder => Group, kind "Group", defaults {}, fields {});

impl NodeBuilder for GroupBuilder {
    type Handle = Group;
    base_accessor!();
    fn build(self, graph: &SceneGraph, parent: NodeId) -> Group {
        Group::from_id(insert(graph, parent, NodeKind::Group, self.base))
    }
}

// ----------------------------------------------------------------- Camera

builder!(
    /// A camera: everything added to it is seen through it, so animating its
    /// `x`/`y` (the point it looks at), `zoom` and `rotation` pans, zooms
    /// and turns the view. Nodes added to the view itself stay on screen
    /// (titles, captions, a HUD).
    CameraBuilder => Camera, kind "Camera", defaults {}, fields { zoom: f64 = 1.0 });
setters!(CameraBuilder {
    /// 1 shows the world at its size, 2 twice as big.
    zoom: f64
});

impl NodeBuilder for CameraBuilder {
    type Handle = Camera;
    base_accessor!();
    fn build(self, g: &SceneGraph, parent: NodeId) -> Camera {
        let kind = NodeKind::Camera {
            zoom: sig(g, Self::KIND, "zoom", self.zoom),
        };
        Camera::from_id(insert(g, parent, kind, self.base))
    }
}

// ------------------------------------------------------------------- Rect

builder!(RectBuilder => Rect, kind "Rect", defaults { fill: Color::WHITE, fill_explicit: true },
    fields { width: Span = Span::Px(100.0), height: Span = Span::Px(100.0), radius: f64 = 0.0 });

impl RectBuilder {
    /// Width in pixels, or [`pct`](crate::pct)`(80)` for a fraction of the frame.
    pub fn width(mut self, v: impl Into<Span>) -> Self {
        self.width = v.into();
        self
    }
    /// Height in pixels, or a percentage of the frame.
    pub fn height(mut self, v: impl Into<Span>) -> Self {
        self.height = v.into();
        self
    }
    /// Corner radius, in pixels or a theme step (`.radius(Md)`).
    /// Clamped to half of the shorter side when drawn.
    pub fn radius(mut self, v: impl Corner) -> Self {
        self.radius = v.radius();
        self
    }
    /// Sets width and height to the same pixel size.
    pub fn size(mut self, v: f64) -> Self {
        (self.width, self.height) = (Span::Px(v), Span::Px(v));
        self
    }
}

impl NodeBuilder for RectBuilder {
    type Handle = Rect;
    base_accessor!();
    fn build(self, g: &SceneGraph, parent: NodeId) -> Rect {
        let k = Self::KIND;
        let n = g.next_id();
        let kind = NodeKind::Rect {
            width: place::span_signal(g, parent, Axis::X, self.width, format!("{k}#{n}.width")),
            height: place::span_signal(g, parent, Axis::Y, self.height, format!("{k}#{n}.height")),
            radius: sig(g, k, "radius", self.radius),
        };
        Rect::from_id(insert(g, parent, kind, self.base))
    }
}

// ----------------------------------------------------------------- Circle

builder!(CircleBuilder => Circle, kind "Circle", defaults { fill: Color::WHITE, fill_explicit: true },
    fields { size: f64 = 100.0, start_angle: f64 = 0.0, end_angle: f64 = 360.0, pie: bool = false });
setters!(CircleBuilder {
    /// Diameter.
    size: f64,
    /// Degrees clockwise from 12 o'clock.
    start_angle: f64,
    end_angle: f64,
    /// Close the arc through the center (pie slice).
    pie: bool
});

impl NodeBuilder for CircleBuilder {
    type Handle = Circle;
    base_accessor!();
    fn build(self, g: &SceneGraph, parent: NodeId) -> Circle {
        let k = Self::KIND;
        let kind = NodeKind::Circle {
            size: sig(g, k, "size", self.size),
            start_angle: sig(g, k, "start_angle", self.start_angle),
            end_angle: sig(g, k, "end_angle", self.end_angle),
            pie: self.pie,
        };
        Circle::from_id(insert(g, parent, kind, self.base))
    }
}

// ---------------------------------------------------------------- Polygon

builder!(PolygonBuilder => Polygon, kind "Polygon", defaults { fill: Color::WHITE, fill_explicit: true },
    fields { sides: f64 = 6.0, size: f64 = 100.0 });
setters!(PolygonBuilder {
    /// Number of sides (rounded; animatable as a number).
    sides: f64,
    /// Diameter of the circumscribed circle.
    size: f64
});

impl NodeBuilder for PolygonBuilder {
    type Handle = Polygon;
    base_accessor!();
    fn build(self, g: &SceneGraph, parent: NodeId) -> Polygon {
        let k = Self::KIND;
        let kind = NodeKind::Polygon {
            sides: sig(g, k, "sides", self.sides),
            size: sig(g, k, "size", self.size),
        };
        Polygon::from_id(insert(g, parent, kind, self.base))
    }
}

// ------------------------------------------------------------------- Star

builder!(StarBuilder => Star, kind "Star", defaults { fill: Color::WHITE, fill_explicit: true },
    fields { points: f64 = 5.0, size: f64 = 100.0, inner: f64 = 0.45 });
setters!(StarBuilder {
    points: f64,
    /// Outer diameter.
    size: f64,
    /// Inner radius as a fraction of the outer one.
    inner: f64
});

impl NodeBuilder for StarBuilder {
    type Handle = Star;
    base_accessor!();
    fn build(self, g: &SceneGraph, parent: NodeId) -> Star {
        let k = Self::KIND;
        let kind = NodeKind::Star {
            points: sig(g, k, "points", self.points),
            size: sig(g, k, "size", self.size),
            inner: sig(g, k, "inner", self.inner),
        };
        Star::from_id(insert(g, parent, kind, self.base))
    }
}

// ------------------------------------------------------------------- Path

builder!(PathBuilder => Path, kind "Path", defaults { fill: Color::WHITE, fill_explicit: true },
    fields { data: BezPath = BezPath::new() });
setters!(PathBuilder { data: BezPath });

impl PathBuilder {
    /// Path from SVG path syntax (`"M0 0 L10 0 Z"`).
    ///
    /// # Panics
    ///
    /// If `d` is not valid SVG path data.
    pub fn svg(mut self, d: &str) -> Self {
        self.data = BezPath::from_svg(d).unwrap_or_else(|e| panic!("invalid SVG path {d:?}: {e}"));
        self
    }
}

impl NodeBuilder for PathBuilder {
    type Handle = Path;
    base_accessor!();
    fn build(self, g: &SceneGraph, parent: NodeId) -> Path {
        let kind = NodeKind::Path {
            data: sig(g, Self::KIND, "data", self.data),
        };
        Path::from_id(insert(g, parent, kind, self.base))
    }
}

// ------------------------------------------------------------------- Line

builder!(LineBuilder => Line, kind "Line",
    defaults { stroke: Color::WHITE, line_width: 4.0 },
    fields { points: Vec<Vec2> = Vec::new(), closed: bool = false, smooth: bool = false,
             start_arrow: bool = false, end_arrow: bool = false, arrow_size: f64 = 24.0 });
setters!(LineBuilder {
    closed: bool,
    /// Smooth curve through the points instead of straight segments.
    smooth: bool,
    start_arrow: bool,
    end_arrow: bool,
    arrow_size: f64
});

impl LineBuilder {
    /// Points the line passes through, in local coordinates.
    pub fn points<P: Into<Vec2>>(mut self, pts: impl IntoIterator<Item = P>) -> Self {
        self.points = pts.into_iter().map(Into::into).collect();
        self
    }
}

impl NodeBuilder for LineBuilder {
    type Handle = Line;
    base_accessor!();
    fn build(self, g: &SceneGraph, parent: NodeId) -> Line {
        let kind = NodeKind::Line {
            points: sig(g, Self::KIND, "points", self.points),
            closed: self.closed,
            smooth: self.smooth,
            start_arrow: self.start_arrow,
            end_arrow: self.end_arrow,
            arrow_size: sig(g, Self::KIND, "arrow_size", self.arrow_size),
        };
        Line::from_id(insert(g, parent, kind, self.base))
    }
}

// -------------------------------------------------------------------- Txt

builder!(TxtBuilder => Txt, kind "Txt", defaults { fill: Color::WHITE },
    fields { text: String = String::new(), spans: Vec<RichSpan> = Vec::new(),
             style: TextStyle = TextStyle::default() });

impl TxtBuilder {
    /// The text to show.
    pub fn text(mut self, v: impl Into<String>) -> Self {
        self.text = v.into();
        self
    }
    /// Styled runs instead of plain text: `[span("Hello "), span("world").bold()]`.
    pub fn rich(mut self, spans: impl IntoIterator<Item = RichSpan>) -> Self {
        self.spans = spans.into_iter().collect();
        self
    }
    /// Font size in pixels. Default: 48.
    pub fn font_size(mut self, v: f64) -> Self {
        self.style.size = v;
        self
    }
    /// 100..=900.
    pub fn weight(mut self, v: f64) -> Self {
        self.style.weight = v;
        self
    }
    /// [`SANS`](crate::SANS), [`MONO`](crate::MONO) or a family registered
    /// with `oxframe_text::register_font`.
    pub fn family(mut self, v: impl Into<String>) -> Self {
        self.style.family = v.into();
        self
    }
    /// Italic style.
    pub fn italic(mut self, v: bool) -> Self {
        self.style.italic = v;
        self
    }
    /// Multiple of the font size.
    pub fn line_height(mut self, v: f64) -> Self {
        self.style.line_height = v;
        self
    }
    /// Extra space between letters, in pixels.
    pub fn letter_spacing(mut self, v: f64) -> Self {
        self.style.letter_spacing = v;
        self
    }
    /// Alignment of lines within the text block.
    pub fn align(mut self, v: crate::TextAlign) -> Self {
        self.style.align = v;
        self
    }
    /// Wrap width.
    pub fn max_width(mut self, v: f64) -> Self {
        self.style.max_width = Some(v);
        self
    }
    /// Largest title from the theme.
    pub fn display(self) -> Self {
        self.role(TypeRole::Display)
    }
    /// Page title from the theme.
    pub fn h1(self) -> Self {
        self.role(TypeRole::H1)
    }
    /// Section title from the theme.
    pub fn h2(self) -> Self {
        self.role(TypeRole::H2)
    }
    /// Running text from the theme.
    pub fn body(self) -> Self {
        self.role(TypeRole::Body)
    }
    /// Secondary text from the theme.
    pub fn caption(self) -> Self {
        self.role(TypeRole::Caption)
    }
    /// Monospace text from the theme.
    pub fn code(self) -> Self {
        self.family(crate::MONO).role(TypeRole::Code)
    }

    fn role(mut self, role: TypeRole) -> Self {
        let r = theme::current().role(role);
        self.style.size = r.size;
        self.style.weight = r.weight;
        self.style.line_height = r.line_height;
        self
    }
}

impl NodeBuilder for TxtBuilder {
    type Handle = Txt;
    base_accessor!();
    fn build(mut self, g: &SceneGraph, parent: NodeId) -> Txt {
        if !self.base.fill_explicit {
            self.base.fill = theme::current().text;
        }
        let k = Self::KIND;
        let kind = NodeKind::Txt {
            text: sig(g, k, "text", self.text),
            spans: sig(g, k, "spans", self.spans),
            font_size: sig(g, k, "font_size", self.style.size),
            weight: sig(g, k, "weight", self.style.weight),
            style: self.style,
            morph: sig(g, k, "morph", None),
            morph_t: sig(g, k, "morph_t", 0.0),
        };
        Txt::from_id(insert(g, parent, kind, self.base))
    }
}

// ------------------------------------------------------------------- Code

builder!(CodeBuilder => Code, kind "Code", defaults {},
    fields { code: String = String::new(), lang: oxframe_text::Lang = oxframe_text::Lang::Rust,
             theme: CodeTheme = CodeTheme::dark(), theme_set: bool = false, style: TextStyle = TextStyle {
                 family: oxframe_text::MONO.to_owned(), size: 32.0, line_height: 1.45, ..TextStyle::default()
             }, focus: Vec2 = Vec2::ZERO, dim: f64 = 0.3 });

impl CodeBuilder {
    /// The source code to show.
    pub fn code(mut self, v: impl Into<String>) -> Self {
        self.code = v.into();
        self
    }
    /// "rust", "js", "ts", "python", "json", "toml", "shell"; anything else
    /// is plain text.
    pub fn language(mut self, name: &str) -> Self {
        self.lang = oxframe_text::Lang::parse(name);
        self
    }
    /// Syntax highlighting colors. Without this, a light project theme uses
    /// [`CodeTheme::light`] and a dark one uses [`CodeTheme::dark`].
    pub fn theme(mut self, t: CodeTheme) -> Self {
        self.theme = t;
        self.theme_set = true;
        self
    }
    /// Font size in pixels.
    pub fn font_size(mut self, v: f64) -> Self {
        self.style.size = v;
        self
    }
    /// Line height as a multiple of the font size.
    pub fn line_height(mut self, v: f64) -> Self {
        self.style.line_height = v;
        self
    }
    /// Highlight lines `first..=last` (1-based); others are dimmed.
    pub fn focus(mut self, first: usize, last: usize) -> Self {
        self.focus = Vec2::new(first as f64, last as f64);
        self
    }
    /// Alpha of unfocused lines (default 0.3).
    pub fn dim(mut self, v: f64) -> Self {
        self.dim = v;
        self
    }
}

impl NodeBuilder for CodeBuilder {
    type Handle = Code;
    base_accessor!();
    fn build(self, g: &SceneGraph, parent: NodeId) -> Code {
        let k = Self::KIND;
        let theme = if self.theme_set {
            self.theme
        } else if theme::current().is_light() {
            CodeTheme::light()
        } else {
            CodeTheme::dark()
        };
        let kind = NodeKind::Code {
            code: sig(g, k, "code", self.code),
            edit_from: sig(g, k, "edit_from", String::new()),
            edit_to: sig(g, k, "edit_to", String::new()),
            editing: sig(g, k, "editing", false),
            edit_t: sig(g, k, "edit_t", 0.0),
            edit_flash: sig(g, k, "edit_flash", false),
            lang: self.lang,
            theme,
            font_size: sig(g, k, "font_size", self.style.size),
            style: self.style,
            focus: sig(g, k, "focus", self.focus),
            dim: sig(g, k, "dim", self.dim),
        };
        Code::from_id(insert(g, parent, kind, self.base))
    }
}

// -------------------------------------------------------------------- Img

builder!(ImgBuilder => Img, kind "Img", defaults {},
    fields { source: Option<ImageSource> = None, width: Option<f64> = None, height: Option<f64> = None });

impl ImgBuilder {
    /// PNG, JPEG or SVG bytes, typically `include_bytes!("../assets/x.png")`
    /// — the only option under wasm.
    ///
    /// # Panics
    ///
    /// If the bytes are not a PNG, JPEG or SVG image.
    pub fn src(mut self, bytes: &[u8]) -> Self {
        self.source =
            Some(ImageSource::decode(bytes).unwrap_or_else(|e| panic!("cannot decode image: {e}")));
        self
    }

    /// Reads an image file (native only).
    ///
    /// # Panics
    ///
    /// If the file cannot be read or decoded.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn file(self, path: impl AsRef<std::path::Path>) -> Self {
        let path = path.as_ref();
        let bytes =
            std::fs::read(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        self.src(&bytes)
    }

    /// Width; height follows the aspect ratio unless also set.
    pub fn width(mut self, v: f64) -> Self {
        self.width = Some(v);
        self
    }

    /// Height; width follows the aspect ratio unless also set.
    pub fn height(mut self, v: f64) -> Self {
        self.height = Some(v);
        self
    }
}

impl NodeBuilder for ImgBuilder {
    type Handle = Img;
    base_accessor!();
    fn build(self, g: &SceneGraph, parent: NodeId) -> Img {
        let source = self
            .source
            .expect("Img needs a source: call .src(bytes) or .file(path)");
        let (nw, nh) = source.natural_size();
        let aspect = if nh > 0.0 { nw / nh } else { 1.0 };
        let (w, h) = match (self.width, self.height) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, w / aspect),
            (None, Some(h)) => (h * aspect, h),
            (None, None) => (nw, nh),
        };
        let k = Self::KIND;
        let kind = NodeKind::Img {
            source,
            width: sig(g, k, "width", w),
            height: sig(g, k, "height", h),
        };
        Img::from_id(insert(g, parent, kind, self.base))
    }
}

// ----------------------------------------------------------------- Layout

builder!(
    /// Places children in a row or column. Children's `x`/`y` become offsets
    /// from their layout position.
    LayoutBuilder => Layout, kind "Layout", defaults {},
    fields { direction: Direction = Direction::Row, align: LayoutAlign = LayoutAlign::Center,
             justify: Justify = Justify::Center, wrap: bool = false, columns: Option<u16> = None,
             gap: f64 = 0.0, padding: f64 = 0.0,
             width: Option<Span> = None, height: Option<Span> = None, radius: f64 = 0.0 });
setters!(LayoutBuilder {
    direction: Direction,
    align: LayoutAlign,
    justify: Justify
});

impl LayoutBuilder {
    /// Space between children, in pixels or a theme step (`.gap(Sm)`).
    pub fn gap(mut self, v: impl Len) -> Self {
        self.gap = v.px();
        self
    }
    /// Space inside the edges, in pixels or a theme step (`.padding(Md)`).
    pub fn padding(mut self, v: impl Len) -> Self {
        self.padding = v.px();
        self
    }
    /// Corner radius, in pixels or a theme step.
    pub fn radius(mut self, v: impl Corner) -> Self {
        self.radius = v.radius();
        self
    }
}

impl LayoutBuilder {
    /// Wrap children onto several lines when they do not fit.
    pub fn wrap(mut self, v: bool) -> Self {
        self.wrap = v;
        self
    }
    /// CSS grid with `n` equal columns instead of a row/column.
    pub fn grid(mut self, columns: u16) -> Self {
        self.columns = Some(columns.max(1));
        self
    }
    /// Children side by side (the default).
    pub fn row(self) -> Self {
        self.direction(Direction::Row)
    }
    /// Children stacked top to bottom.
    pub fn column(self) -> Self {
        self.direction(Direction::Column)
    }
    /// Fixed width in pixels or [`pct`](crate::pct)`(80)` (otherwise sized to content).
    pub fn width(mut self, v: impl Into<Span>) -> Self {
        self.width = Some(v.into());
        self
    }
    /// Fixed height in pixels or a percentage (otherwise sized to content).
    pub fn height(mut self, v: impl Into<Span>) -> Self {
        self.height = Some(v.into());
        self
    }
}

impl NodeBuilder for LayoutBuilder {
    type Handle = Layout;
    base_accessor!();
    fn build(self, g: &SceneGraph, parent: NodeId) -> Layout {
        let k = Self::KIND;
        let kind = NodeKind::Layout {
            direction: self.direction,
            align: self.align,
            justify: self.justify,
            wrap: self.wrap,
            columns: self.columns,
            gap: sig(g, k, "gap", self.gap),
            padding: sig(g, k, "padding", self.padding),
            width: place::span_option(
                g,
                parent,
                Axis::X,
                self.width,
                format!("{k}#{}.width", g.next_id()),
            ),
            height: place::span_option(
                g,
                parent,
                Axis::Y,
                self.height,
                format!("{k}#{}.height", g.next_id()),
            ),
            radius: sig(g, k, "radius", self.radius),
        };
        Layout::from_id(insert(g, parent, kind, self.base))
    }
}

// ------------------------------------------------------------------- Grid

builder!(GridBuilder => Grid, kind "Grid",
    defaults { stroke: Color::rgba(1.0, 1.0, 1.0, 0.15), line_width: 1.0 },
    fields { width: f64 = 1920.0, height: f64 = 1080.0, spacing: f64 = 80.0 });
setters!(GridBuilder {
    width: f64,
    height: f64,
    spacing: f64
});

impl NodeBuilder for GridBuilder {
    type Handle = Grid;
    base_accessor!();
    fn build(self, g: &SceneGraph, parent: NodeId) -> Grid {
        let k = Self::KIND;
        let kind = NodeKind::Grid {
            width: sig(g, k, "width", self.width),
            height: sig(g, k, "height", self.height),
            spacing: sig(g, k, "spacing", self.spacing),
        };
        Grid::from_id(insert(g, parent, kind, self.base))
    }
}
