//! Frames where a shader reads drawn pixels.
//!
//! The draw list carries markers (see [`oxframe_2d::post`]). Everything
//! before a `ReadFrame` is drawn, the shader runs on those pixels, and the
//! rest is drawn over the result. A node between `BeginEffect` and
//! `EndEffect` is drawn alone, shaded, and laid over the frame. Parts are
//! drawn on a clear target and laid over each other, which matches drawing
//! the list at once for nodes with the normal blend.

use oxframe_2d::post::over;
use oxframe_2d::{DrawItem, Primitive, SceneGraph, apply_shade};
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
    let mut frame: Option<Image> = None;
    let mut start = 0;
    let mut i = 0;
    while i < items.len() {
        match &items[i].prim {
            Primitive::ReadFrame(shade) => {
                let mut img = lay(graph, renderer, &items[start..i], options, frame.take());
                apply_shade(shade, &mut img, view);
                frame = Some(img);
                start = i + 1;
            }
            Primitive::BeginEffect(shade) => {
                let end = matching_end(items, i);
                let mut img = lay(graph, renderer, &items[start..i], options, frame.take());
                let mut node = renderer.render_list(graph, &items[i + 1..end], &clear);
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
        let card = view.add(crate::Group::new());
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
