//! `View3d`: a 2D node that shows a 3D scene (feature `3d`).

use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;

use oxframe_2d::{
    BaseProps, DrawCtx, Node, NodeBuilder, NodeId, RasterImage, SceneGraph, node_api,
};
use oxframe_3d::Scene3d;
use oxframe_core::Signal;

/// The last picture and the fingerprint of the scene it shows.
type Cache = Rc<RefCell<Option<(u64, Rc<RasterImage>)>>>;

struct State {
    scene: Scene3d,
    width: Signal<f64>,
    height: Signal<f64>,
}

struct Paint {
    scene: Scene3d,
    width: f64,
    height: f64,
    samples: u32,
    resolution: f64,
    cache: Cache,
}

impl node_api::Draw for Paint {
    fn draw(&self, ctx: &mut DrawCtx) {
        let (w, h) = (self.width.max(1.0), self.height.max(1.0));
        let pw = (w * self.resolution).round().max(1.0) as u32;
        let ph = (h * self.resolution).round().max(1.0) as u32;
        let key = self.scene.fingerprint(pw, ph, self.samples);
        let image = {
            let mut cache = self.cache.borrow_mut();
            match &*cache {
                Some((k, img)) if *k == key => img.clone(),
                _ => {
                    let frame = self.scene.render(pw, ph, self.samples);
                    let img = Rc::new(RasterImage {
                        width: frame.width,
                        height: frame.height,
                        rgba: frame.rgba,
                        id: key,
                    });
                    *cache = Some((key, img.clone()));
                    img
                }
            }
        };
        let rect = oxframe_core::kurbo::Rect::new(-w / 2.0, -h / 2.0, w / 2.0, h / 2.0);
        ctx.image(rect, image);
    }

    fn size(&self) -> (f64, f64) {
        (self.width, self.height)
    }
}

/// Builder for a [`View3d`].
#[must_use = "a builder does nothing until it is passed to `add`"]
pub struct View3dBuilder {
    base: BaseProps,
    scene: Scene3d,
    width: f64,
    height: f64,
    samples: u32,
    resolution: f64,
}

impl View3dBuilder {
    /// Width and height of the picture in the 2D scene. Default 640 × 360.
    pub fn size(mut self, width: f64, height: f64) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Supersampling per side: 2 (the default) smooths edges, 1 is faster.
    pub fn samples(mut self, n: u32) -> Self {
        self.samples = n.clamp(1, 4);
        self
    }

    /// Pixels rendered per unit of size. 1 matches the project size; 2
    /// stays sharp when the view is scaled up or rendered at twice the
    /// size.
    pub fn resolution(mut self, k: f64) -> Self {
        self.resolution = k.clamp(0.1, 4.0);
        self
    }
}

impl NodeBuilder for View3dBuilder {
    type Handle = View3d;

    fn base(&mut self) -> &mut BaseProps {
        &mut self.base
    }

    fn build(self, graph: &SceneGraph, parent: NodeId) -> View3d {
        let state = Rc::new(State {
            scene: self.scene.clone(),
            width: Signal::new(self.width).with_name("View3d.width"),
            height: Signal::new(self.height).with_name("View3d.height"),
        });
        let (scene, samples, resolution) = (self.scene, self.samples, self.resolution);
        let cache: Cache = Rc::new(RefCell::new(None));
        let s = state.clone();
        let snapshot = Rc::new(move || Paint {
            scene: scene.clone(),
            width: s.width.get(),
            height: s.height.get(),
            samples,
            resolution,
            cache: cache.clone(),
        });
        let id = node_api::add(graph, parent, self.base, "View3d", snapshot, state);
        View3d(Node::from_id(id))
    }
}

/// A 2D node showing a [`Scene3d`]. It is a picture in the 2D scene, so
/// position, opacity, effects, the 2D camera and layout work as for any
/// node. Derefs to [`Node`].
///
/// ```ignore
/// let world = Scene3d::new();
/// let cube = world.add(Mesh::cube().color(Accent));
/// view.add(View3d::new(&world).size(640.0, 400.0));
/// cube.rotation().to(Vec3::new(0.0, 360.0, 0.0), 3.0).await;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct View3d(Node);

impl View3d {
    /// A view of `scene`, 640 × 360.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(scene: &Scene3d) -> View3dBuilder {
        View3dBuilder {
            base: BaseProps::default(),
            scene: scene.clone(),
            width: 640.0,
            height: 360.0,
            samples: 2,
            resolution: 1.0,
        }
    }

    fn state(self) -> Rc<State> {
        node_api::state::<State>(self.0)
    }

    /// Width in the 2D scene; the picture is rendered at this size.
    pub fn width(self) -> Signal<f64> {
        self.state().width.clone()
    }

    /// Height in the 2D scene.
    pub fn height(self) -> Signal<f64> {
        self.state().height.clone()
    }

    /// Where world point `p` of the scene appears on this view now, in the
    /// view's own coordinates (origin at its center, y down). Behind the
    /// camera, the point is far off the view.
    pub fn project(self, p: oxframe_3d::Vec3) -> oxframe_core::Vec2 {
        self.projector()(p)
    }

    /// [`project`](Self::project) as a function that can live in a
    /// binding, so a 2D label follows a 3D point every frame:
    ///
    /// ```ignore
    /// let to_view = frame.projector();
    /// label.x().bind(move || to_view(tip).x);
    /// ```
    pub fn projector(self) -> impl Fn(oxframe_3d::Vec3) -> oxframe_core::Vec2 + Clone + 'static {
        let s = self.state();
        move |p| {
            let (w, h) = (s.width.get(), s.height.get());
            match s.scene.project(p, w / h.max(1e-6)) {
                Some((x, y)) => oxframe_core::Vec2::new(x * w / 2.0, -y * h / 2.0),
                None => oxframe_core::Vec2::new(f64::MAX / 4.0, f64::MAX / 4.0),
            }
        }
    }

    /// The untyped node handle.
    pub fn node(self) -> Node {
        self.0
    }
}

impl Deref for View3d {
    type Target = Node;
    fn deref(&self) -> &Node {
        &self.0
    }
}

impl From<View3d> for Node {
    fn from(v: View3d) -> Node {
        v.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxframe_2d::View;
    use oxframe_3d::{Mesh, Vec3};

    #[test]
    fn the_view_draws_the_scene_and_reuses_an_unchanged_picture() {
        let graph = SceneGraph::new(320, 180);
        let world = Scene3d::new();
        let cube = world.add(Mesh::cube());
        let (view, first, again) = graph.enter(|| {
            let v = View::current().add(View3d::new(&world).size(160.0, 90.0).samples(1));
            let images = |g: &SceneGraph| {
                g.draw_list()
                    .into_iter()
                    .filter_map(|item| match item.prim {
                        oxframe_2d::Primitive::Image { image, .. } => Some(image),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
            let first = images(&graph);
            let again = images(&graph);
            (v, first, again)
        });
        assert_eq!(first.len(), 1);
        assert_eq!((first[0].width, first[0].height), (160, 90));
        assert!(
            Rc::ptr_eq(&first[0], &again[0]),
            "an unchanged scene is not rendered again"
        );
        assert_eq!(graph.enter(|| view.size()), (160.0, 90.0));
        cube.rotation().set(Vec3::new(0.0, 45.0, 0.0));
        let turned = graph.enter(|| {
            graph
                .draw_list()
                .into_iter()
                .find_map(|item| match item.prim {
                    oxframe_2d::Primitive::Image { image, .. } => Some(image),
                    _ => None,
                })
                .unwrap()
        });
        assert_ne!(turned.id, first[0].id);
    }
}
