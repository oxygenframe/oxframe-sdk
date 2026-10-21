use std::fmt;
use std::path::PathBuf;
use std::rc::Rc;

use oxframe_2d::{SceneGraph, View};
use oxframe_core::{Color, Executor, Recorded};
use oxframe_protocol::{MarkerInfo, Meta, MetaError, SceneInfo};
use oxframe_render::{Image, RenderOptions, Renderer};

use crate::{Project, SceneDef};

/// Why a project could not be loaded.
#[derive(Debug)]
pub enum EngineError {
    /// A `meta.json` file is invalid.
    Meta {
        /// The file.
        path: PathBuf,
        /// What is wrong with it.
        error: MetaError,
    },
    /// A file could not be read or written.
    Io {
        /// The file.
        path: PathBuf,
        /// The I/O error.
        error: std::io::Error,
    },
    /// `Project::scene` was never called.
    NoScenes,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Meta { path, error } => write!(f, "{}: {error}", path.display()),
            Self::Io { path, error } => write!(f, "{}: {error}", path.display()),
            Self::NoScenes => write!(f, "the project has no scenes"),
        }
    }
}

impl std::error::Error for EngineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Meta { error, .. } => Some(error),
            Self::Io { error, .. } => Some(error),
            Self::NoScenes => None,
        }
    }
}

/// A section of `meta.json`, for [`Engine::edit_meta`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaSection {
    /// `markers`.
    Markers,
    /// `eases`.
    Eases,
    /// `tunables`.
    Tunables,
}

impl MetaSection {
    fn singular(self) -> &'static str {
        match self {
            Self::Markers => "marker",
            Self::Eases => "ease",
            Self::Tunables => "tunable",
        }
    }
}

/// One execution of a scene: its graph and executor.
struct SceneRun {
    graph: Rc<SceneGraph>,
    exec: Executor,
    theme: oxframe_2d::Theme,
    has_previous: bool,
}

impl SceneRun {
    fn new(def: &SceneDef, project: &Project, meta: Meta, has_previous: bool) -> Self {
        let graph = SceneGraph::new(project.size.0, project.size.1);
        let fut = (def.factory)(View::current());
        Self {
            graph,
            exec: Executor::new(
                f64::from(project.fps),
                meta,
                scene_seed(project.seed, def.name),
                fut,
            ),
            theme: project.theme.clone(),
            has_previous,
        }
    }

    fn step(&mut self) -> bool {
        self.exec.set_has_previous(self.has_previous);
        let theme = self.theme.clone();
        let done = oxframe_2d::enter_theme(&theme, || {
            let Self { graph, exec, .. } = self;
            graph.enter(|| exec.step())
        });
        let frame = self.exec.frames_done().saturating_sub(1);
        self.graph.set_time(f64::from(frame) / self.exec.fps());
        done
    }

    /// Index of the last computed frame, if any.
    fn last_frame(&self) -> Option<u32> {
        self.exec.frames_done().checked_sub(1)
    }
}

struct SceneState {
    def: SceneDef,
    meta: Meta,
    meta_path: PathBuf,
    start: u32,
    frames: u32,
    /// Transition declared while measuring, if the scene starts one.
    incoming: Option<Recorded>,
    /// Last frame of this scene, transparent, reused while the next scene blends.
    held: Option<(u32, u32, Image)>,
    run: Option<SceneRun>,
}

/// Runs a [`Project`]: knows the length of every scene and produces the
/// state of any global frame.
///
/// Seeking backwards re-simulates the scene from frame 0 without rendering;
/// seeking forwards continues from the current state.
pub struct Engine {
    project: Project,
    scenes: Vec<SceneState>,
    /// `render --scene` draws that scene alone, with no incoming blend.
    skip_transitions: bool,
}

impl Engine {
    /// Loads the metas and runs every scene once to find its length.
    ///
    /// # Errors
    ///
    /// If a `meta.json` cannot be read or is invalid, or there are no scenes.
    pub fn new(project: Project) -> Result<Self, EngineError> {
        if project.scenes.is_empty() {
            return Err(EngineError::NoScenes);
        }
        let mut scenes = Vec::with_capacity(project.scenes.len());
        for def in &project.scenes {
            let meta_path = match &project.meta_dir {
                Some(dir) => dir.join(format!("{}.meta.json", def.name)),
                None => def.meta_path(),
            };
            let meta = match &project.inline_meta {
                Some(m) => m.get(def.name).cloned().unwrap_or_default(),
                None => load_meta(&meta_path)?,
            };
            scenes.push(SceneState {
                def: *def,
                meta,
                meta_path,
                start: 0,
                frames: 0,
                incoming: None,
                held: None,
                run: None,
            });
        }
        // The soundtrack is project-wide; its offset lives in the first
        // scene's meta.
        if let Some(file) = &project.audio
            && let Some(first) = scenes.first_mut()
            && first.meta.audio.as_ref().is_none_or(|a| &a.file != file)
        {
            let offset = first.meta.audio.as_ref().map_or(0.0, |a| a.offset);
            first.meta.audio = Some(oxframe_protocol::AudioMeta {
                file: file.clone(),
                offset,
                extra: Default::default(),
            });
            if project.write_meta && project.inline_meta.is_none() {
                save_meta(&first.meta_path, &first.meta)?;
            }
        }
        let mut engine = Self {
            project,
            scenes,
            skip_transitions: false,
        };
        for i in 0..engine.scenes.len() {
            engine.measure(i)?;
        }
        engine.layout();
        Ok(engine)
    }

    /// The project definition.
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Runs scene `i` to completion to learn its length and to record
    /// defaults (markers, eases, tunables) into its meta.
    fn measure(&mut self, i: usize) -> Result<(), EngineError> {
        let max = (self.project.max_scene_seconds * f64::from(self.project.fps)).ceil() as u32;
        let scene = &mut self.scenes[i];
        scene.held = None;
        let mut run = SceneRun::new(&scene.def, &self.project, scene.meta.clone(), i > 0);
        while !run.step() && run.exec.frames_done() < max.max(1) {}
        scene.frames = run.exec.frames_done();
        scene.incoming = run.exec.transition();
        if run.exec.take_meta_changed() {
            scene.meta = run.exec.meta();
            if self.project.write_meta && self.project.inline_meta.is_none() {
                save_meta(&scene.meta_path, &scene.meta)?;
            }
        }
        // Reuse the run: it is positioned at the last frame.
        scene.run = Some(run);
        Ok(())
    }

    fn layout(&mut self) {
        let mut start = 0;
        for s in &mut self.scenes {
            s.start = start;
            start += s.frames;
        }
    }

    /// Frames per second.
    pub fn fps(&self) -> u32 {
        self.project.fps
    }

    /// Project size in pixels.
    pub fn size(&self) -> (u32, u32) {
        self.project.size
    }

    /// Length of all scenes together, in frames.
    pub fn total_frames(&self) -> u32 {
        self.scenes.last().map_or(0, |s| s.start + s.frames)
    }

    /// Scenes with their first frame and length.
    pub fn scenes(&self) -> Vec<SceneInfo> {
        self.scenes
            .iter()
            .map(|s| {
                let (at, frames) = blend_span(s, self.project.fps);
                SceneInfo {
                    name: s.def.name.to_owned(),
                    start: s.start,
                    frames: s.frames,
                    transition: frames,
                    transition_at: at,
                }
            })
            .collect()
    }

    /// Markers of all scenes.
    pub fn markers(&self) -> Vec<MarkerInfo> {
        self.scenes
            .iter()
            .flat_map(|s| {
                s.meta.markers.iter().map(|(name, &time)| MarkerInfo {
                    scene: s.def.name.to_owned(),
                    name: name.clone(),
                    time,
                })
            })
            .collect()
    }

    /// Soundtrack file and offset (seconds), if the project has one.
    pub fn audio(&self) -> Option<(String, f64)> {
        let a = self.scenes.first()?.meta.audio.as_ref()?;
        Some((a.file.clone(), a.offset))
    }

    /// Meta of every scene, as `(scene, meta)`.
    pub fn metas(&self) -> Vec<(String, Meta)> {
        self.scenes
            .iter()
            .map(|s| (s.def.name.to_owned(), s.meta.clone()))
            .collect()
    }

    /// Meta of one scene.
    pub fn meta(&self, scene: &str) -> Option<&Meta> {
        self.scenes
            .iter()
            .find(|s| s.def.name == scene)
            .map(|s| &s.meta)
    }

    /// Scene index and local frame of a global frame (clamped to the end).
    pub fn locate(&self, frame: u32) -> (usize, u32) {
        let total = self.total_frames();
        let frame = frame.min(total.saturating_sub(1));
        let i = self
            .scenes
            .iter()
            .rposition(|s| s.start <= frame)
            .unwrap_or(0);
        (i, frame - self.scenes[i].start)
    }

    /// Simulates up to global `frame` and returns the scene graph in that
    /// state. Does not render.
    pub fn seek(&mut self, frame: u32) -> &SceneGraph {
        let (i, local) = self.locate(frame);
        let project = &self.project;
        let scene = &mut self.scenes[i];
        let reusable = scene
            .run
            .as_ref()
            .is_some_and(|r| r.last_frame().is_some_and(|last| last <= local));
        if !reusable {
            scene.run = Some(SceneRun::new(
                &scene.def,
                project,
                scene.meta.clone(),
                i > 0,
            ));
        }
        let run = scene.run.as_mut().expect("run was just ensured");
        while run.last_frame().is_none_or(|last| last < local) {
            if run.step() {
                break;
            }
        }
        // Drop runs of other scenes; only one graph is alive at a time.
        for (j, s) in self.scenes.iter_mut().enumerate() {
            if j != i {
                s.run = None;
            }
        }
        &self.scenes[i].run.as_ref().expect("run exists").graph
    }

    /// Applies a meta edit to the scene that owns `name` (markers, eases and
    /// tunables are looked up across scenes), re-measures it and saves the
    /// meta. Returns the scene name and its new meta.
    ///
    /// # Errors
    ///
    /// If the scene or name is unknown, the name is invalid, or the meta
    /// cannot be saved.
    pub fn edit_meta(
        &mut self,
        section: MetaSection,
        name: &str,
        scene: Option<&str>,
        apply: impl FnOnce(&mut Meta),
    ) -> Result<(String, Meta), String> {
        let i = match scene {
            Some(scene) => self
                .scenes
                .iter()
                .position(|s| s.def.name == scene)
                .ok_or_else(|| format!("no scene named `{scene}`"))?,
            None => self
                .scenes
                .iter()
                .position(|s| match section {
                    MetaSection::Markers => s.meta.markers.contains_key(name),
                    MetaSection::Eases => s.meta.eases.contains_key(name),
                    MetaSection::Tunables => s.meta.tunables.contains_key(name),
                })
                .ok_or_else(|| format!("no {} named `{name}` in any scene", section.singular()))?,
        };
        let mut meta = self.scenes[i].meta.clone();
        apply(&mut meta);
        self.write_scene_meta(i, meta)
    }

    /// Converts frame pixels (top-left origin) to scene coordinates.
    pub fn pixels_to_scene(&self, x: f64, y: f64) -> oxframe_core::Point {
        let (w, h) = self.project.size;
        oxframe_core::Point::new(x - f64::from(w) / 2.0, y - f64::from(h) / 2.0)
    }

    /// `render --scene` draws that scene by itself. The blend back to the
    /// previous scene is left out; a full render keeps every transition.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn skip_transitions(&mut self, skip: bool) {
        self.skip_transitions = skip;
    }

    /// Where `frame` sits in an incoming blend, if it does.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn transition_at(&self, frame: u32) -> Option<TransitionAt> {
        let (_i, rec, now) = self.blend_at(frame)?;
        Some(TransitionAt {
            kind: rec.kind_name(),
            dir: rec.dir_name(),
            progress: rec.progress(now),
            overlap: rec.overlap(),
        })
    }

    /// One entry per scene that starts with a blend.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn transition_spans(&self) -> Vec<TransitionSpan> {
        self.scenes
            .iter()
            .filter_map(|s| {
                let (at, frames) = blend_span(s, self.project.fps);
                let rec = s.incoming?;
                (frames > 0).then(|| TransitionSpan {
                    scene: s.def.name.to_owned(),
                    start: s.start + at,
                    frames,
                    kind: rec.kind_name(),
                })
            })
            .collect()
    }

    /// Writes the transition length into `meta.json` and re-measures the scene.
    ///
    /// # Errors
    ///
    /// If there is no scene of that name, or the meta cannot be saved.
    pub(crate) fn set_transition_seconds(
        &mut self,
        scene: &str,
        seconds: f64,
    ) -> Result<(String, Meta), String> {
        let i = self
            .scenes
            .iter()
            .position(|s| s.def.name == scene)
            .ok_or_else(|| format!("no scene named `{scene}`"))?;
        let seconds = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        let mut meta = self.scenes[i].meta.clone();
        meta.transition = Some(oxframe_protocol::TransitionMeta { seconds });
        self.write_scene_meta(i, meta)
    }

    fn blend_at(&self, frame: u32) -> Option<(usize, Recorded, f64)> {
        if self.skip_transitions {
            return None;
        }
        let (i, local) = self.locate(frame);
        if i == 0 {
            return None;
        }
        let rec = self.scenes[i].incoming?;
        let now = f64::from(local) / f64::from(self.project.fps);
        rec.active(now).then_some((i, rec, now))
    }

    fn outgoing_frame(&self, i: usize, rec: Recorded, now: f64) -> u32 {
        let prev = &self.scenes[i - 1];
        let last_local = prev.frames.saturating_sub(1);
        if !rec.overlap() {
            return prev.start + last_local;
        }
        let fps = f64::from(self.project.fps);
        let elapsed = (now - rec.start()).max(0.0);
        let prev_end = f64::from(last_local) / fps;
        let prev_t = (prev_end - rec.seconds() + elapsed).max(0.0);
        let local = ((prev_t * fps).round() as u32).min(last_local);
        prev.start + local
    }

    /// Simulates up to `frame` and draws it at `size` pixels.
    ///
    /// A frame inside a transition is the previous scene and this one,
    /// each drawn on a clear layer and then blended.
    pub fn render(&mut self, frame: u32, renderer: &mut dyn Renderer, size: (u32, u32)) -> Image {
        let background = self.project.background;
        if let Some((i, rec, now)) = self.blend_at(frame) {
            let hold = !rec.overlap();
            let from_frame = self.outgoing_frame(i, rec, now);
            let blend = rec.blend_at(now);
            let t = rec.progress(now);
            let prev = i - 1;
            let from = if hold {
                self.held_layer(prev, renderer, size)
            } else {
                self.layer(from_frame, renderer, size)
            };
            let to = self.layer(frame, renderer, size);
            let image = crate::transition::composite(&from, &to, &blend, t, background);
            renderer.present_image(&image);
            return image;
        }
        self.layer_on(frame, renderer, size, background)
    }

    fn held_layer(&mut self, scene: usize, renderer: &mut dyn Renderer, size: (u32, u32)) -> Image {
        if let Some((w, h, img)) = &self.scenes[scene].held
            && *w == size.0
            && *h == size.1
        {
            return img.clone();
        }
        let last = self.scenes[scene].start + self.scenes[scene].frames.saturating_sub(1);
        let img = self.layer(last, renderer, size);
        self.scenes[scene].held = Some((size.0, size.1, img.clone()));
        img
    }

    fn layer(&mut self, frame: u32, renderer: &mut dyn Renderer, size: (u32, u32)) -> Image {
        self.draw(frame, renderer, size, Color::TRANSPARENT, false)
    }

    fn layer_on(
        &mut self,
        frame: u32,
        renderer: &mut dyn Renderer,
        size: (u32, u32),
        background: Color,
    ) -> Image {
        self.draw(frame, renderer, size, background, true)
    }

    fn draw(
        &mut self,
        frame: u32,
        renderer: &mut dyn Renderer,
        size: (u32, u32),
        background: Color,
        present: bool,
    ) -> Image {
        let theme = self.project.theme.clone();
        let graph = self.seek(frame);
        let options = RenderOptions {
            width: size.0,
            height: size.1,
            background,
        };
        // Custom resolves theme colors while painting, after the scene step
        // has already returned.
        oxframe_2d::enter_theme(&theme, || {
            if present {
                renderer.render(graph, &options)
            } else {
                renderer.render_pixels(graph, &options)
            }
        })
    }

    fn write_scene_meta(&mut self, i: usize, meta: Meta) -> Result<(String, Meta), String> {
        meta.validate().map_err(|e| e.to_string())?;
        let scene = &mut self.scenes[i];
        scene.meta = meta;
        scene.held = None;
        if self.project.write_meta && self.project.inline_meta.is_none() {
            save_meta(&scene.meta_path, &scene.meta).map_err(|e| e.to_string())?;
        }
        scene.run = None;
        self.measure(i).map_err(|e| e.to_string())?;
        self.layout();
        let s = &self.scenes[i];
        Ok((s.def.name.to_owned(), s.meta.clone()))
    }
}

/// A scene's incoming blend, for `inspect`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct TransitionAt {
    /// `"fade"`, `"slide"`, `"wipe"`, `"zoom_in"`, `"zoom_out"` or `"custom"`.
    pub kind: &'static str,
    /// Direction of a slide or a wipe.
    pub dir: Option<&'static str>,
    /// Eased progress, 0 at the first blend frame.
    pub progress: f64,
    /// The previous scene keeps playing.
    pub overlap: bool,
}

/// Frames a storyboard should sample inside a blend.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct TransitionSpan {
    /// Scene the blend belongs to.
    pub scene: String,
    /// First global frame of the blend.
    pub start: u32,
    /// Blend length in frames.
    pub frames: u32,
    /// Transition type, same names as [`TransitionAt::kind`].
    pub kind: &'static str,
}

fn blend_span(scene: &SceneState, fps: u32) -> (u32, u32) {
    let Some(rec) = scene.incoming else {
        return (0, 0);
    };
    let fps = f64::from(fps.max(1));
    let mut at = 0;
    let mut frames = 0;
    let mut seen = false;
    for local in 0..scene.frames {
        if rec.active(f64::from(local) / fps) {
            if !seen {
                at = local;
                seen = true;
            }
            frames += 1;
        }
    }
    (at, frames)
}

/// Per-scene seed: the project seed mixed with an FNV-1a hash of the name,
/// so scenes get independent but reproducible streams.
fn scene_seed(seed: u64, name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    seed ^ h
}

fn load_meta(path: &PathBuf) -> Result<Meta, EngineError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Meta::parse(&text).map_err(|error| EngineError::Meta {
            path: path.clone(),
            error,
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Meta::default()),
        Err(error) => Err(EngineError::Io {
            path: path.clone(),
            error,
        }),
    }
}

fn save_meta(path: &PathBuf, meta: &Meta) -> Result<(), EngineError> {
    let text = meta.to_pretty_string();
    if std::fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    // Write through a temp file so an interrupted write never leaves a
    // truncated meta.json behind.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &text)
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|error| EngineError::Io {
            path: path.clone(),
            error,
        })
}
