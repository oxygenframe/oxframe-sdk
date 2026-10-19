//! Profiles the stages of rendering the benchmark scene at 1080p.
use std::time::Instant;

use oxframe::Engine;
use oxframe::render::{GpuRenderer, RenderOptions, Renderer};

fn main() {
    let mut e = Engine::new(oxframe_examples::benchmark::project().write_meta(false)).unwrap();
    let mut gpu = GpuRenderer::new().unwrap();
    println!("adapter: {}", gpu.adapter_name());
    let (mut sim, mut list, mut render) = (0.0, 0.0, 0.0);
    let n = 60;
    for f in 0..n {
        let t = Instant::now();
        let g = e.seek(600 + f);
        sim += t.elapsed().as_secs_f64();
        let t = Instant::now();
        let items = g.draw_list();
        list += t.elapsed().as_secs_f64();
        std::hint::black_box(items);
        let t = Instant::now();
        gpu.render(
            g,
            &RenderOptions {
                width: 1920,
                height: 1080,
                background: oxframe::Color::BLACK,
            },
        );
        render += t.elapsed().as_secs_f64();
    }
    let ms = |v: f64| 1000.0 * v / n as f64;
    println!(
        "per frame: simulate {:.2} ms, draw_list {:.2} ms, gpu render+readback {:.2} ms",
        ms(sim),
        ms(list),
        ms(render)
    );
}
