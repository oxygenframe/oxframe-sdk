use std::collections::BTreeMap;
use std::path::PathBuf;

use oxframe_core::Color;
use oxframe_protocol::Meta;

use crate::SceneDef;

/// A sequence of scenes plus output settings. Call `main()`
/// from your binary's `main` to get the `render`/`info` commands.
#[derive(Debug, Clone)]
pub struct Project {
    pub(crate) name: String,
    pub(crate) scenes: Vec<SceneDef>,
    pub(crate) size: (u32, u32),
    pub(crate) fps: u32,
    pub(crate) background: Color,
    pub(crate) seed: u64,
    pub(crate) max_scene_seconds: f64,
    pub(crate) write_meta: bool,
    pub(crate) meta_dir: Option<PathBuf>,
    pub(crate) audio: Option<String>,
    pub(crate) inline_meta: Option<BTreeMap<String, Meta>>,
}

impl Project {
    /// A project with defaults: 1920×1080, 60 fps, dark background, seed 0.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            scenes: Vec::new(),
            size: (1920, 1080),
            fps: 60,
            background: Color::from_rgba8(0x14, 0x14, 0x14, 0xff),
            seed: 0,
            max_scene_seconds: 600.0,
            write_meta: true,
            meta_dir: None,
            audio: None,
            // No file system in the browser: metas live in memory there.
            inline_meta: cfg!(target_arch = "wasm32").then(BTreeMap::new),
        }
    }

    /// Appends a scene; scenes play in the order they are added.
    pub fn scene(mut self, scene: SceneDef) -> Self {
        self.scenes.push(scene);
        self
    }

    /// Frame size in pixels.
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.size = (width.max(1), height.max(1));
        self
    }

    /// Frames per second.
    pub fn fps(mut self, fps: u32) -> Self {
        self.fps = fps.max(1);
        self
    }

    /// Background color; [`Color::TRANSPARENT`] keeps alpha in webm, ProRes and PNG.
    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    /// Seed for scene randomness; part of the deterministic input.
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Scenes that have not finished after this long are cut (default 10 min).
    pub fn max_scene_seconds(mut self, seconds: f64) -> Self {
        self.max_scene_seconds = seconds.max(0.0);
        self
    }

    /// Whether defaults recorded by scene code are written back to
    /// `meta.json` (default: true).
    pub fn write_meta(mut self, write: bool) -> Self {
        self.write_meta = write;
        self
    }

    /// Reads and writes meta files in `dir` instead of next to the sources.
    pub fn meta_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.meta_dir = Some(dir.into());
        self
    }

    /// Keeps metas in memory (scene name → meta) instead of reading and
    /// writing `<scene>.meta.json` files. Used in the browser and in tests.
    pub fn inline_meta(mut self, metas: BTreeMap<String, Meta>) -> Self {
        self.inline_meta = Some(metas);
        self
    }

    /// Soundtrack, relative to the project root (where `oxframe` runs).
    /// Its offset is edited in the timeline and stored in the first scene's
    /// `meta.json`.
    pub fn audio(mut self, path: impl Into<String>) -> Self {
        self.audio = Some(path.into());
        self
    }

    /// Project name, used for output file names.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Parses `std::env::args()` and runs the requested command; exits the
    /// process with a non-zero code on failure.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn main(self) {
        let code = crate::cli::run(self, std::env::args().skip(1).collect());
        std::process::exit(code);
    }
}
