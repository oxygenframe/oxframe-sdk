//! M0 acceptance tests (spec §10): the intro example renders, output is
//! deterministic, seek meets its budget.

use std::time::{Duration, Instant};

use oxframe::Engine;
use oxframe::render::{CpuRenderer, Image};

#[test]
fn gallery_has_no_check_errors() {
    use std::collections::BTreeMap;
    let mut bad = Vec::new();
    for (name, make) in oxframe_examples::all() {
        let project = make()
            .write_meta(false)
            .inline_meta(BTreeMap::<String, oxframe::protocol::Meta>::new());
        let report = oxframe::feedback::report(project, &oxframe::feedback::Options::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let errors: Vec<_> = report
            .issues
            .iter()
            .filter(|i| i.severity == oxframe::lint::Severity::Error)
            .map(|i| {
                format!(
                    "{name} {} {:.2}..{:.2} {}: {}",
                    i.rule, i.time[0], i.time[1], i.node, i.hint
                )
            })
            .collect();
        if !errors.is_empty() {
            bad.extend(errors);
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

fn engine(project: oxframe::Project) -> Engine {
    Engine::new(project.write_meta(false)).expect("engine")
}

fn render_all(size: (u32, u32)) -> Vec<Image> {
    let mut e = engine(oxframe_examples::intro::project());
    let mut r = CpuRenderer::new();
    (0..e.total_frames())
        .map(|f| e.render(f, &mut r, size))
        .collect()
}

fn pixel(img: &Image, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * img.width + x) * 4) as usize;
    img.data[i..i + 4].try_into().unwrap()
}

#[test]
fn square_becomes_circle_left_to_right_in_one_second() {
    let mut e = engine(oxframe_examples::intro::project());
    assert_eq!(e.fps(), 60);
    // 1 s at 60 fps: frames 0..=60.
    assert_eq!(e.total_frames(), 61);

    let mut r = CpuRenderer::new();
    // Work at 1/4 of 1920x1080: the square is 50 px, x from -75 to +75.
    let size = (480, 270);
    let first = e.render(0, &mut r, size);
    let last = e.render(60, &mut r, size);
    let (cx, cy) = (240, 135);
    let red = [0xe1, 0x32, 0x38, 0xff];
    let blue = [0x68, 0xab, 0xdf, 0xff];

    // Frame 0: square on the left — its corner is filled.
    assert_eq!(pixel(&first, cx - 75, cy), red);
    assert_eq!(pixel(&first, cx - 75 - 24, cy - 24), red);
    assert_ne!(pixel(&first, cx + 75, cy), red);

    // Last frame: circle on the right — the bounding-box corner is empty,
    // the center is filled.
    assert_eq!(pixel(&last, cx + 75, cy), blue);
    assert_ne!(pixel(&last, cx + 75 - 24, cy - 24), blue);
    assert_ne!(pixel(&last, cx - 75, cy), blue);

    // Midway it is in the middle.
    let mid = e.render(30, &mut r, size);
    assert_ne!(pixel(&mid, cx, cy), pixel(&first, cx, cy));
}

#[test]
fn rendering_is_deterministic() {
    let a = render_all((320, 180));
    let b = render_all((320, 180));
    assert_eq!(a.len(), b.len());
    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        assert!(x.data == y.data, "frame {i} differs between runs");
    }
}

#[test]
fn seek_backwards_matches_linear_playback() {
    let mut e = engine(oxframe_examples::intro::project());
    let mut r = CpuRenderer::new();
    let forward: Vec<_> = (0..61).map(|f| e.render(f, &mut r, (160, 90))).collect();
    for f in [45, 3, 60, 0, 30] {
        assert!(
            e.render(f, &mut r, (160, 90)) == forward[f as usize],
            "frame {f}"
        );
    }
}

/// The reference scene is 60 s = 3600 frames, so "10 000 frames of logic"
/// is measured as repeated full re-simulations until 10 000 frames are run.
fn simulate_10k(e: &mut Engine) -> Duration {
    let last = e.total_frames() - 1;
    let mut done = 0;
    let t = Instant::now();
    while done < 10_000 {
        e.seek(0);
        let target = last.min(10_000 - done);
        e.seek(target);
        done += target + 1;
    }
    t.elapsed()
}

#[test]
fn seek_10k_frames_within_budget() {
    let mut e = engine(oxframe_examples::benchmark::project());
    assert_eq!(e.total_frames(), 60 * 60 + 1);
    let best = (0..3).map(|_| simulate_10k(&mut e)).min().unwrap();
    eprintln!("seek 10k frames: {best:?}");
    // The 100 ms target applies to optimized builds on a developer machine;
    // debug builds only get a sanity bound. Shared CI runners are slower, so
    // CI raises it with OXFRAME_SEEK_BUDGET_MS (the bench job tracks the
    // real number).
    let budget = std::env::var("OXFRAME_SEEK_BUDGET_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(if cfg!(debug_assertions) { 5_000 } else { 100 });
    assert!(best <= Duration::from_millis(budget), "seek took {best:?}");
}

#[test]
fn every_example_builds_and_renders() {
    let mut r = CpuRenderer::new();
    for (name, project) in oxframe_examples::all() {
        let mut e = engine(project());
        assert!(e.total_frames() > 1, "{name} is empty");
        let last = e.total_frames() - 1;
        for f in [0, last / 2, last] {
            let img = e.render(f, &mut r, (160, 90));
            assert_eq!(img.data.len(), 160 * 90 * 4, "{name}");
        }
    }
}
