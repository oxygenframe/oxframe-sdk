//! Seek: re-simulating 10 000 frames of the benchmark scene without
//! rendering. Target: ≤ 100 ms (spec §8).

use criterion::{Criterion, criterion_group, criterion_main};
use oxframe::Engine;

fn seek(c: &mut Criterion) {
    let mut engine = Engine::new(oxframe_examples::benchmark::project().write_meta(false)).unwrap();
    let last = engine.total_frames() - 1;
    // The scene is 3601 frames; 10 000 frames of logic = repeated full
    // re-simulations (seeking back to 0 forces one).
    c.bench_function("simulate 10k frames (200 nodes)", |b| {
        b.iter(|| {
            let mut done = 0;
            while done < 10_000 {
                engine.seek(0);
                let target = last.min(10_000 - done);
                engine.seek(target);
                done += target + 1;
            }
        });
    });
}

fn render(c: &mut Criterion) {
    let mut engine = Engine::new(oxframe_examples::benchmark::project().write_meta(false)).unwrap();
    let mut renderer = oxframe::render::CpuRenderer::new();
    let mut frame = 0;
    c.bench_function("render 1080p frame, tiny-skia", |b| {
        b.iter(|| {
            frame = (frame + 1) % 600;
            engine.render(frame, &mut renderer, (1920, 1080))
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(20);
    targets = seek, render
}
criterion_main!(benches);
