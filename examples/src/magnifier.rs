//! A magnifying glass written in WGSL: the shader reads the frame drawn
//! before it (`Input::frame()`) and bends it inside a moving circle.
//!
//! A custom WGSL shader runs on the CPU in `oxframe dev`, `check` and
//! `render`; the playground does not paint it, so this example is on the
//! gallery as a video.

use oxframe::prelude::*;

/// `cx`, `cy`: the lens center (0..1 across the frame); `radius` in pixels;
/// `zoom`: how much the middle is magnified.
const LENS: &str = r#"
struct Params { cx: f32, cy: f32, radius: f32, zoom: f32 }
@group(1) @binding(0) var<uniform> p: Params;

fn shade(i: ShadeIn) -> vec4<f32> {
    let res = ox.resolution;
    let center = vec2<f32>(p.cx, p.cy) * res;
    let d = i.screen - center;
    let r = length(d) / p.radius;
    if (r > 1.0) {
        return sample_scene(i.screen / res);
    }
    // A lens bends the middle most and the rim not at all.
    let at = center + d * mix(1.0 / p.zoom, 1.0, r * r);
    // Glass splits colors a little toward the rim.
    let fringe = d * 0.015 * r;
    let color = vec3<f32>(
        sample_scene((at + fringe) / res).r,
        sample_scene(at / res).g,
        sample_scene((at - fringe) / res).b,
    );
    let rim = smoothstep(0.9, 1.0, r);
    return vec4<f32>(mix(color, vec3<f32>(1.0), rim * 0.7), 1.0);
}
"#;

#[oxframe::scene]
pub async fn magnifier(view: View) {
    let title = view.add(Txt::new().text("Read the fine print").h1().y(-330.0));
    let print = view.add(
        Txt::new()
            .text(
                "oxframe draws every frame on the CPU, so a render is the same \
                 bytes on every machine. Your own WGSL runs there too, a tile \
                 of pixels at a time, and can read what was drawn before it.",
            )
            .font_size(32)
            .max_width(1200.0)
            .fill(Muted)
            .y(40.0),
    );
    // Added last, so it reads the title and the text.
    let lens = view.add(
        Shader::source(LENS)
            .uniform("cx", 0.27)
            .uniform("cy", 0.53)
            .uniform("radius", 0.0)
            .uniform("zoom", 2.2)
            .input("scene", Input::frame())
            .fullscreen(),
    );

    all((title.fade_in(), print.fade_in())).await;
    lens.uniform("radius").to(170.0, 0.5).await;
    lens.uniform("cx").to(0.73, 3.0).await;
    all((
        lens.uniform("cy").to(0.2, 1.0),
        lens.uniform("cx").to(0.5, 1.0),
    ))
    .await;
    lens.uniform("zoom").to(1.4, 0.8).await;
    lens.uniform("radius").to(0.0, 0.4).await;
}

pub fn project() -> Project {
    Project::new("magnifier").scene(magnifier())
}
