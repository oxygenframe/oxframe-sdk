//! The CPU renderer draws the same bytes on every platform: one frame of
//! several examples, hashed, against hashes recorded on Linux x86_64. CI
//! runs this on Linux, macOS (Apple Silicon) and Windows.
//!
//! After an intended change to what a frame looks like:
//! `OXFRAME_UPDATE_HASHES=1 cargo test -p oxframe-examples --test frame_hashes`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use oxframe::Engine;
use oxframe::render::CpuRenderer;

/// Text, shapes, gradients, blur and shadows, charts, SVG, 3D, particles and
/// a CPU shader: the parts that could round differently per platform.
const EXAMPLES: &[&str] = &[
    "intro",
    "typography",
    "shapes",
    "effects",
    "charts",
    "svg",
    "cube_3d",
    "particles",
    "code_steps",
    "waves",
];

const SIZE: (u32, u32) = (480, 270);

/// FNV-1a, 64 bit: stable across platforms and Rust versions, unlike
/// `DefaultHasher`.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn hashes() -> BTreeMap<String, String> {
    let all: BTreeMap<_, _> = oxframe_examples::all().into_iter().collect();
    let mut out = BTreeMap::new();
    for name in EXAMPLES {
        let make = all.get(name).unwrap_or_else(|| panic!("no example {name}"));
        let mut engine = Engine::new(make().write_meta(false)).expect("engine");
        let frame = engine.total_frames() / 2;
        let image = engine.render(frame, &mut CpuRenderer::new(), SIZE);
        out.insert((*name).to_owned(), format!("{:016x}", fnv1a(&image.data)));
    }
    out
}

#[test]
fn cpu_frames_match_the_recorded_hashes() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/frame-hashes.json");
    let now = hashes();
    if std::env::var_os("OXFRAME_UPDATE_HASHES").is_some() {
        let text = serde_json::to_string_pretty(&now).expect("json") + "\n";
        std::fs::write(&path, text).expect("write frame-hashes.json");
        return;
    }
    let recorded: BTreeMap<String, String> =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("frame-hashes.json"))
            .expect("frame-hashes.json is a map");
    let differ: Vec<String> = now
        .iter()
        .filter(|(name, hash)| recorded.get(*name) != Some(*hash))
        .map(|(name, hash)| format!("{name}: {hash}, recorded {:?}", recorded.get(name)))
        .collect();
    assert!(
        differ.is_empty(),
        "frames differ from the hashes recorded on Linux x86_64 ({} {}):\n{}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        differ.join("\n")
    );
}
