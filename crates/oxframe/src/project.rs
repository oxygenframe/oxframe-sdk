use std::collections::BTreeMap;
use std::path::PathBuf;

use oxframe_2d::Theme;
use oxframe_core::Color;
use oxframe_protocol::Meta;

use crate::SceneDef;
use crate::audio::Audio;

/// A sequence of scenes plus output settings. Call `main()`
/// from your binary's `main` to get the `render`/`info` commands.
#[derive(Debug, Clone)]
pub struct Project {
    pub(crate) name: String,
    pub(crate) scenes: Vec<SceneDef>,
    pub(crate) size: (u32, u32),
    pub(crate) fps: u32,
    pub(crate) background: Color,
    pub(crate) theme: Theme,
    pub(crate) seed: u64,
    pub(crate) max_scene_seconds: f64,
    pub(crate) write_meta: bool,
    pub(crate) meta_dir: Option<PathBuf>,
    pub(crate) audio: Option<Audio>,
    pub(crate) inline_meta: Option<BTreeMap<String, Meta>>,
    /// Formats `check` renders, and `render --aspect all` exports.
    pub(crate) aspects: Vec<Aspect>,
}

/// A frame shape. The project's longer side stays the long side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aspect {
    /// 16:9.
    Landscape,
    /// 9:16.
    Portrait,
    /// 1:1.
    Square,
}

impl Aspect {
    /// `16:9`, `9:16` or `1:1`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Landscape => "16:9",
            Self::Portrait => "9:16",
            Self::Square => "1:1",
        }
    }

    /// Parses `16:9`, `9:16`, `1:1`, or the names `landscape`, `portrait`, `square`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "16:9" | "landscape" => Some(Self::Landscape),
            "9:16" | "portrait" => Some(Self::Portrait),
            "1:1" | "square" => Some(Self::Square),
            _ => None,
        }
    }

    /// Pixel size for this shape, using the longer side of `base`.
    pub fn frame(self, base: (u32, u32)) -> (u32, u32) {
        let long = base.0.max(base.1).max(1);
        let short = (f64::from(long) * 9.0 / 16.0).round().max(1.0) as u32;
        match self {
            Self::Landscape => (long, short),
            Self::Portrait => (short, long),
            Self::Square => (short, short),
        }
    }
}

impl Project {
    /// A project with defaults: 1920×1080, 60 fps, dark background, seed 0.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            scenes: Vec::new(),
            size: (1920, 1080),
            fps: 60,
            background: Theme::dark().background,
            theme: Theme::dark(),
            seed: 0,
            max_scene_seconds: 600.0,
            write_meta: true,
            meta_dir: None,
            audio: None,
            // No file system in the browser: metas live in memory there.
            inline_meta: cfg!(target_arch = "wasm32").then(BTreeMap::new),
            aspects: Vec::new(),
        }
    }

    /// Formats to check, and to export with `render --aspect all`.
    ///
    /// The project's longer side is the long side of each format: a
    /// 1920×1080 project becomes 1920×1080, 1080×1920 and 1080×1080.
    pub fn aspects(mut self, aspects: impl IntoIterator<Item = Aspect>) -> Self {
        self.aspects = aspects.into_iter().collect();
        self
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
    ///
    /// [`theme`](Self::theme) sets this to the theme's background. Call
    /// `background` after `theme` to override just the page color.
    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    /// Design tokens for every node that does not set its own color or size.
    /// Also sets the background to the theme's background.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.background = theme.background;
        self.theme = theme;
        self
    }

    // `oxframe.toml` [theme] overrides, applied by the native CLI only.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn apply_preset(&mut self, name: &str) -> Result<(), String> {
        let theme = Theme::preset(name).ok_or_else(|| {
            format!("unknown theme `{name}` (dark, light, midnight, paper, vivid)")
        })?;
        self.background = theme.background;
        self.theme = theme;
        Ok(())
    }

    // `oxframe.toml` [theme] overrides, applied by the native CLI only.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn apply_accent(&mut self, hex: &str) -> Result<(), String> {
        let color = Color::from_hex(hex).ok_or_else(|| format!("bad accent color `{hex}`"))?;
        self.theme = self.theme.clone().accent(color);
        Ok(())
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
    /// `meta.json`. A path is the same as [`Audio::track`](crate::Audio::track).
    pub fn audio(mut self, track: impl Into<Audio>) -> Self {
        self.audio = Some(track.into());
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
