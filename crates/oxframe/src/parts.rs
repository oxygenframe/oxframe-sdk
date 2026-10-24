//! Frames where a shader reads drawn pixels.
//!
//! The draw list carries markers (see [`oxframe_2d::post`]). Everything
//! before a `ReadFrame` is drawn, the shader runs on those pixels, and the
//! rest is drawn over the result. A node between `BeginEffect` and
//! `EndEffect` is drawn alone, shaded, and laid over the frame. Parts are
//! drawn on a clear target and laid over each other, which matches drawing
//! the list at once for nodes with the normal blend. A shader that reads
//! an offscreen node gets that node drawn alone first.

use std::borrow::Cow;
use std::rc::Rc;

use oxframe_2d::RasterImage;
use oxframe_2d::post::over;
use oxframe_2d::{DrawItem, Primitive, SceneGraph, apply_shade, apply_shade_with};
use oxframe_core::kurbo::{Affine, Rect as KRect};
use oxframe_core::{Color, Image};
use oxframe_render::{RenderOptions, Renderer, scene_to_pixels};

/// Draws `graph` around its markers and returns the pixels, not presented.
pub(crate) fn draw_in_parts(
    graph: &SceneGraph,
    renderer: &mut dyn Renderer,
    options: &RenderOptions,
) -> Image {
    draw_items_in_parts(graph, &graph.draw_list(), renderer, options)
}

/// [`draw_in_parts`] for a given draw list, such as one with some nodes left
/// out.
pub(crate) fn draw_items_in_parts(
    graph: &SceneGraph,
    items: &[DrawItem],
    renderer: &mut dyn Renderer,
    options: &RenderOptions,
) -> Image {
    let view = scene_to_pixels(graph.size(), (options.width.max(1), options.height.max(1)));
    let clear = RenderOptions {
        background: Color::TRANSPARENT,
        ..*options
    };
    let flat = flatten_layers(graph, items, renderer, &clear, view);
    let items = &flat[..];
    let mut frame: Option<Image> = None;
    let mut start = 0;
    let mut i = 0;
    while i < items.len() {
        match &items[i].prim {
            Primitive::ReadFrame(shade) => {
                let mut img = lay(graph, renderer, &items[start..i], options, frame.take());
                // Nodes the shader reads, each drawn alone.
                let drawn: Vec<Image> = shade
                    .offscreen_lists()
                    .into_iter()
                    .map(|list| draw_items_in_parts(graph, list, renderer, &clear))
                    .collect();
                apply_shade_with(shade, &mut img, view, &drawn);
                frame = Some(img);
                start = i + 1;
            }
            Primitive::BeginEffect(shade) => {
                let end = matching_end(items, i);
                let mut img = lay(graph, renderer, &items[start..i], options, frame.take());
                let mut node = draw_items_in_parts(graph, &items[i + 1..end], renderer, &clear);
                apply_shade(shade, &mut node, view);
                over(&mut img, &node);
                frame = Some(img);
                start = (end + 1).min(items.len());
                i = end;
            }
            _ => {}
        }
        i += 1;
    }
    lay(graph, renderer, &items[start..], options, frame)
}

/// A layer (`PushLayer` … `PopLayer`) that holds a marker is drawn in parts
/// on its own and put back as one image inside the same layer, so the
/// renderer still applies the layer's opacity, blur, clip, blend and
/// shadow. A shader in there reads what the layer drew before it.
fn flatten_layers<'a>(
    graph: &SceneGraph,
    items: &'a [DrawItem],
    renderer: &mut dyn Renderer,
    clear: &RenderOptions,
    view: Affine,
) -> Cow<'a, [DrawItem]> {
    let mut out: Option<Vec<DrawItem>> = None;
    let mut i = 0;
    while i < items.len() {
        if matches!(items[i].prim, Primitive::PushLayer(_)) {
            let end = matching_pop(items, i);
            let inner = &items[i + 1..end];
            if inner.iter().any(|item| item.prim.is_marker()) {
                let out = out.get_or_insert_with(|| items[..i].to_vec());
                let img = draw_items_in_parts(graph, inner, renderer, clear);
                let (w, h) = (f64::from(img.width), f64::from(img.height));
                out.push(items[i].clone());
                out.push(DrawItem {
                    node: items[i].node,
                    transform: view.inverse(),
                    prim: Primitive::Image {
                        image: Rc::new(RasterImage::new(img.width, img.height, img.data)),
                        rect: KRect::new(0.0, 0.0, w, h),
                        opacity: 1.0,
                    },
                });
                if let Some(pop) = items.get(end) {
                    out.push(pop.clone());
                }
                i = end + 1;
                continue;
            }
        }
        if let Some(out) = out.as_mut() {
            out.push(items[i].clone());
        }
        i += 1;
    }
    out.map_or(Cow::Borrowed(items), Cow::Owned)
}

/// Index of the `PopLayer` that closes the `PushLayer` at `start`, or the
/// end of the list.
fn matching_pop(items: &[DrawItem], start: usize) -> usize {
    let mut depth = 0usize;
    for (i, item) in items.iter().enumerate().skip(start) {
        match item.prim {
            Primitive::PushLayer(_) => depth += 1,
            Primitive::PopLayer => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    items.len()
}

/// Draws `part` over `below`, or on the background when nothing is drawn yet.
fn lay(
    graph: &SceneGraph,
    renderer: &mut dyn Renderer,
    part: &[DrawItem],
    options: &RenderOptions,
    below: Option<Image>,
) -> Image {
    let Some(mut img) = below else {
        return renderer.render_list(graph, part, options);
    };
    if part.iter().all(|item| item.prim.is_marker()) {
        return img;
    }
    let clear = RenderOptions {
        background: Color::TRANSPARENT,
        ..*options
    };
    let top = renderer.render_list(graph, part, &clear);
    over(&mut img, &top);
    img
}

/// Index of the `EndEffect` that closes the `BeginEffect` at `start`, or
/// the end of the list.
fn matching_end(items: &[DrawItem], start: usize) -> usize {
    let mut depth = 0usize;
    for (i, item) in items.iter().enumerate().skip(start) {
        match item.prim {
            Primitive::BeginEffect(_) => depth += 1,
            Primitive::EndEffect => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    items.len()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use oxframe_core::{BoxFuture, Color, Image};
    use oxframe_protocol::Meta;
    use oxframe_render::CpuRenderer;

    use crate::{
        Engine, Node, NodeBuilder, Project, Rect, SceneDef, SourceLocation, View, shaders,
    };

    type SceneFn = fn(View) -> BoxFuture;

    fn draw(factory: SceneFn) -> Image {
        let project = Project::new("parts")
            .size(160, 90)
            .fps(10)
            .background(Color::BLACK)
            .write_meta(false)
            .inline_meta(BTreeMap::<String, Meta>::new())
            .scene(SceneDef::new(
                "main",
                SourceLocation {
                    file: "src/parts.rs",
                    manifest_dir: env!("CARGO_MANIFEST_DIR"),
                },
                factory,
            ));
        let mut engine = Engine::new(project).unwrap();
        let size = engine.size();
        engine.render(0, &mut CpuRenderer::new(), size)
    }

    /// Stripes, so a shader that moves pixels changes them.
    fn stripes(view: &View) -> Node {
        stripes_in(**view)
    }

    fn stripes_in(parent: Node) -> Node {
        let card = parent.add(crate::Group::new());
        for i in 0..8 {
            let c = f64::from(i) / 7.0;
            card.add(
                Rect::new()
                    .width(20.0)
                    .height(90.0)
                    .x(-70.0 + 20.0 * f64::from(i))
                    .fill(Color::rgb(c as f32, 0.4, (1.0 - c) as f32)),
            );
        }
        *card
    }

    fn plain(view: View) -> BoxFuture {
        Box::pin(async move {
            stripes(&view);
            view.add(Rect::new().size(16.0).fill(Color::WHITE));
            crate::wait_for(0.2).await;
        })
    }

    fn left_lens(view: View) -> BoxFuture {
        Box::pin(async move {
            stripes(&view);
            view.add(
                shaders::lens_drops()
                    .wetness(1.0)
                    .seed(3)
                    .width(80.0)
                    .height(90.0)
                    .x(-40.0),
            );
            crate::wait_for(0.2).await;
        })
    }

    fn dry_lens(view: View) -> BoxFuture {
        Box::pin(async move {
            stripes(&view);
            view.add(shaders::lens_drops().wetness(0.0).fullscreen());
            view.add(Rect::new().size(16.0).fill(Color::WHITE));
            crate::wait_for(0.2).await;
        })
    }

    fn still_ripple(view: View) -> BoxFuture {
        Box::pin(async move {
            let card = stripes(&view);
            card.effect(shaders::ripple().strength(0.0));
            view.add(Rect::new().size(16.0).fill(Color::WHITE));
            crate::wait_for(0.2).await;
        })
    }

    fn ripple(view: View) -> BoxFuture {
        Box::pin(async move {
            let card = stripes(&view);
            card.effect(shaders::ripple().speed(0.0));
            view.add(Rect::new().size(16.0).fill(Color::WHITE));
            crate::wait_for(0.2).await;
        })
    }

    /// Mirrors the frame left to right inside its box (the right half).
    fn mirror(view: View) -> BoxFuture {
        Box::pin(async move {
            stripes(&view);
            view.add(
                crate::Shader::source(
                    "struct Params { k: f32 }
@group(1) @binding(0) var<uniform> p: Params;
fn shade(i: ShadeIn) -> vec4<f32> {
    let s = i.screen / ox.resolution;
    return sample_scene(vec2<f32>(1.0 - s.x, s.y));
}",
                )
                .uniform("k", 0.0)
                .input("scene", crate::Input::frame())
                .width(80.0)
                .height(90.0)
                .x(40.0),
            );
            crate::wait_for(0.2).await;
        })
    }

    fn max_diff(a: &Image, b: &Image) -> u8 {
        a.data
            .iter()
            .zip(&b.data)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0)
    }

    fn column(img: &Image, x: u32) -> Vec<u8> {
        (0..img.height)
            .flat_map(|y| {
                let i = ((y * img.width + x) * 4) as usize;
                img.data[i..i + 4].to_vec()
            })
            .collect()
    }

    #[test]
    fn a_custom_shader_reads_the_frame_in_a_render() {
        let img = draw(mirror);
        let base = draw(plain);
        let w = img.width;
        // The left half is the stripes; the right half shows them mirrored.
        assert_eq!(column(&img, 10), column(&base, 10));
        let right = column(&img, w - 11);
        let mirrored = column(&base, 10);
        let worst = right
            .iter()
            .zip(&mirrored)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        assert!(
            worst <= 2,
            "right edge mirrors the left edge, off by {worst}"
        );
        assert_ne!(column(&img, w - 11), column(&base, w - 11));
    }

    /// A white square drawn only for a shader, which shows it on the
    /// right, tinted red.
    fn offscreen_square(view: View) -> BoxFuture {
        Box::pin(async move {
            let square = view.add(
                Rect::new()
                    .size(20.0)
                    .x(-50.0)
                    .fill(Color::WHITE)
                    .offscreen(),
            );
            view.add(
                crate::Shader::source(
                    "struct Params { k: f32 }
@group(1) @binding(0) var<uniform> p: Params;
fn shade(i: ShadeIn) -> vec4<f32> {
    let c = sample_square(i.uv);
    return vec4<f32>(c.r, 0.0, 0.0, c.a);
}",
                )
                .uniform("k", 0.0)
                .input("square", crate::Input::node(square))
                .size(40.0, 40.0)
                .x(50.0),
            );
            crate::wait_for(0.2).await;
        })
    }

    #[test]
    fn a_shader_reads_an_offscreen_node() {
        let img = draw(offscreen_square);
        let px = |x: u32, y: u32| {
            let i = ((y * img.width + x) * 4) as usize;
            [img.data[i], img.data[i + 1], img.data[i + 2]]
        };
        // Scene 160x90: the square would sit at x = 30, the shader at 130.
        assert_eq!(px(30, 45), [0, 0, 0], "an offscreen node is not drawn");
        assert_eq!(px(130, 45), [255, 0, 0], "the shader shows it");
        assert_eq!(px(5, 5), [0, 0, 0]);
    }

    fn faded(ripple: bool) -> Image {
        let scene = if ripple {
            (|view: View| -> BoxFuture {
                Box::pin(async move {
                    let group = view.add(crate::Group::new().opacity(0.5));
                    let card = stripes_in(*group);
                    card.effect(shaders::ripple().speed(0.0));
                    crate::wait_for(0.2).await;
                })
            }) as SceneFn
        } else {
            |view: View| -> BoxFuture {
                Box::pin(async move {
                    let group = view.add(crate::Group::new().opacity(0.5));
                    stripes_in(*group);
                    crate::wait_for(0.2).await;
                })
            }
        };
        draw(scene)
    }

    #[test]
    fn an_effect_inside_a_fading_group_runs_and_fades() {
        let (rippled, plain) = (faded(true), faded(false));
        assert!(max_diff(&rippled, &plain) > 10, "the ripple moved pixels");
        let full = draw(super::tests::plain);
        // Away from the ripple's reach the group is just faded.
        let at = |img: &Image| img.data[(45 * img.width as usize + 12) * 4 + 2];
        assert!(at(&plain) < at(&full), "the group is faded");
    }

    #[test]
    fn lens_drops_change_only_their_box() {
        let lens = draw(left_lens);
        let base = draw(|view| {
            Box::pin(async move {
                stripes(&view);
                crate::wait_for(0.2).await;
            })
        });
        for x in 80..160 {
            assert_eq!(
                column(&lens, x),
                column(&base, x),
                "column {x} is outside the box"
            );
        }
        assert!((0..80).any(|x| column(&lens, x) != column(&base, x)));
    }

    #[test]
    fn drawing_in_parts_matches_drawing_at_once() {
        assert!(max_diff(&draw(dry_lens), &draw(plain)) <= 1);
    }

    #[test]
    fn a_ripple_with_no_strength_draws_the_node_as_it_is() {
        assert!(max_diff(&draw(still_ripple), &draw(plain)) <= 1);
    }

    #[test]
    fn a_ripple_moves_the_node_and_not_what_is_drawn_after_it() {
        let rippled = draw(ripple);
        let base = draw(plain);
        assert!(max_diff(&rippled, &base) > 20);
        let px = |img: &Image, x: u32, y: u32| {
            let i = ((y * img.width + x) * 4) as usize;
            img.data[i..i + 4].to_vec()
        };
        // The white square is added after the card, so the rings do not move it.
        assert_eq!(px(&rippled, 80, 45), px(&base, 80, 45));
    }

    #[test]
    fn effects_match_on_cpu_and_gpu() {
        for factory in [left_lens as SceneFn, ripple] {
            let project = Project::new("parts")
                .size(160, 90)
                .fps(10)
                .background(Color::BLACK)
                .write_meta(false)
                .inline_meta(BTreeMap::<String, Meta>::new())
                .scene(SceneDef::new(
                    "main",
                    SourceLocation {
                        file: "src/parts.rs",
                        manifest_dir: env!("CARGO_MANIFEST_DIR"),
                    },
                    factory,
                ));
            let mut engine = Engine::new(project).unwrap();
            let size = engine.size();
            let cpu = engine.render(0, &mut CpuRenderer::new(), size);
            let mut gpu = match crate::render::GpuRenderer::new() {
                Ok(g) if g.is_software() && std::env::var_os("OXFRAME_GPU_TESTS").is_none() => {
                    return;
                }
                Ok(g) => g,
                Err(_) => return,
            };
            let gpu_img = engine.render(0, &mut gpu, size);
            let total: u64 = cpu
                .data
                .iter()
                .zip(&gpu_img.data)
                .map(|(a, b)| u64::from(a.abs_diff(*b)))
                .sum();
            let mean = total as f64 / cpu.data.len() as f64;
            assert!(mean < 1.0, "CPU and GPU differ by {mean:.2} on average");
        }
    }
}
