//! Renders a few frames of every example into `out/snapshots/` for a quick
//! visual check: `cargo run --release -p oxframe-examples --example snapshots [name]`.
use oxframe::Engine;
use oxframe::render::CpuRenderer;

fn main() {
    let only = std::env::args().nth(1);
    let dir = std::path::Path::new("out/snapshots");
    std::fs::create_dir_all(dir).unwrap();
    let mut r = CpuRenderer::new();
    for (name, project) in oxframe_examples::all() {
        if only.as_deref().is_some_and(|o| !name.starts_with(o)) {
            continue;
        }
        let mut e = Engine::new(project().write_meta(false)).unwrap();
        let last = e.total_frames() - 1;
        for (i, f) in [last / 4, last / 2, 3 * last / 4, last]
            .into_iter()
            .enumerate()
        {
            let img = e.render(f, &mut r, (640, 360));
            std::fs::write(dir.join(format!("{name}-{i}.png")), img.encode_png()).unwrap();
        }
        println!("{name}: {} frames", e.total_frames());
    }
}
