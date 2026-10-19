//! Browser API (feature `wasm`): the engine and the preview protocol
//! compiled to wasm, driven from a Web Worker by `@oxframe/backends`.
//!
//! A playground crate registers its project and links this module:
//!
//! ```ignore
//! #[wasm_bindgen(start)]
//! pub fn start() { oxframe::wasm::register(scene::project); }
//! ```
//!
//! JS then calls `init(config)` and talks the same protocol as over the
//! WebSocket: `command(json)`, `tick(now)`, `poll_events()`, `poll_frame()`.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

use oxframe_preview::{Output, PreviewEngine, Session, rgba_encoder};
#[cfg(feature = "gpu")]
use oxframe_protocol::FrameFormat;
use oxframe_protocol::{Command, Meta};
use oxframe_render::{CpuRenderer, Renderer};
#[cfg(feature = "gpu")]
use oxframe_render::{Image, RenderOptions};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::{Engine, PreviewAdapter, Project};

thread_local! {
    static PROJECT: RefCell<Option<fn() -> Project>> = const { RefCell::new(None) };
}

/// Registers the project the page will run. Call once from the playground
/// crate's `#[wasm_bindgen(start)]`.
pub fn register(project: fn() -> Project) {
    console_error_panic_hook::set_once();
    PROJECT.with(|p| *p.borrow_mut() = Some(project));
}

#[derive(Deserialize, Default)]
struct Config {
    /// Metas by scene name (the playground keeps them next to the source).
    #[serde(default)]
    metas: BTreeMap<String, Meta>,
}

/// Engine handle exported to JS.
#[wasm_bindgen]
pub struct WasmEngine {
    session: Session<PreviewAdapter>,
    events: VecDeque<String>,
    frames: VecDeque<Vec<u8>>,
}

fn project_engine(config_json: &str) -> Result<Engine, JsError> {
    let project = PROJECT
        .with(|p| *p.borrow())
        .ok_or_else(|| JsError::new("no project registered: call oxframe::wasm::register()"))?;
    let config: Config = if config_json.trim().is_empty() {
        Config::default()
    } else {
        serde_json::from_str(config_json).map_err(|e| JsError::new(&format!("bad config: {e}")))?
    };
    Engine::new(project().inline_meta(config.metas)).map_err(|e| JsError::new(&e.to_string()))
}

/// `init(config_json)`: creates the engine for the registered project,
/// rendering on the CPU; frames are RGBA messages.
#[wasm_bindgen]
pub fn init(config_json: &str) -> Result<WasmEngine, JsError> {
    let engine = project_engine(config_json)?;
    let renderer: Box<dyn Renderer> = Box::new(CpuRenderer::new());
    Ok(WasmEngine {
        session: Session::new(PreviewAdapter::new(engine, renderer), rgba_encoder(), 128),
        events: VecDeque::new(),
        frames: VecDeque::new(),
    })
}

/// Draws into an OffscreenCanvas with WebGPU (vello); frames are presented
/// there and messages carry only a `Surface` header.
#[cfg(feature = "gpu")]
struct CanvasRenderer(oxframe_render::GpuRenderer);

#[cfg(feature = "gpu")]
impl Renderer for CanvasRenderer {
    fn render(&mut self, graph: &oxframe_2d::SceneGraph, options: &RenderOptions) -> Image {
        self.0.render_to_canvas(graph, options);
        Image {
            width: options.width,
            height: options.height,
            data: Vec::new(),
        }
    }
}

/// `init_gpu(config_json, canvas)`: like [`init`], but renders with
/// WebGPU into `canvas` (call `canvas.transferToImageBitmap()` after each
/// `Surface` frame). Rejects if WebGPU is unavailable — fall back to `init`.
#[cfg(feature = "gpu")]
#[wasm_bindgen]
pub async fn init_gpu(
    config_json: String,
    canvas: web_sys::OffscreenCanvas,
) -> Result<WasmEngine, JsError> {
    let engine = project_engine(&config_json)?;
    let gpu = oxframe_render::GpuRenderer::new_for_canvas(canvas)
        .await
        .map_err(|e| JsError::new(&e.to_string()))?;
    let renderer: Box<dyn Renderer> = Box::new(CanvasRenderer(gpu));
    let encoder: oxframe_preview::Encoder =
        Box::new(|_img: &Image| (FrameFormat::Surface, Vec::new()));
    Ok(WasmEngine {
        session: Session::new(PreviewAdapter::new(engine, renderer), encoder, 0),
        events: VecDeque::new(),
        frames: VecDeque::new(),
    })
}

#[wasm_bindgen]
impl WasmEngine {
    fn push(&mut self, outputs: Vec<Output>) {
        for o in outputs {
            match o {
                Output::Event(e) => self
                    .events
                    .push_back(serde_json::to_string(&e).expect("event json")),
                Output::Frame(f) => {
                    // Only the newest frame matters to the UI.
                    self.frames.clear();
                    self.frames.push_back(f);
                }
            }
        }
    }

    /// Handles one protocol command (JSON). `now` is seconds from
    /// `performance.now()`.
    pub fn command(&mut self, json: &str) -> Result<(), JsError> {
        let cmd: Command =
            serde_json::from_str(json).map_err(|e| JsError::new(&format!("bad command: {e}")))?;
        let now = js_now();
        let out = self.session.handle(cmd, now);
        self.push(out);
        Ok(())
    }

    /// Advances playback.
    pub fn tick(&mut self, now: f64) {
        let out = self.session.tick(now);
        self.push(out);
    }

    /// Pending protocol events as JSON strings.
    pub fn poll_events(&mut self) -> Vec<String> {
        self.events.drain(..).collect()
    }

    /// Next binary frame message (16-byte header + RGBA), if any.
    pub fn poll_frame(&mut self) -> Option<Vec<u8>> {
        self.frames.pop_front()
    }

    /// Renders `frame` at project size as straight-alpha RGBA (CPU path).
    pub fn render_frame(&mut self, frame: u32) -> Vec<u8> {
        let size = self.session.engine().size();
        self.session.engine().render(frame, size).data
    }

    /// `{"fps", "size": [w, h], "frames"}` as JSON (for in-browser export).
    pub fn info(&mut self) -> String {
        let e = self.session.engine();
        let (w, h) = e.size();
        serde_json::json!({ "fps": e.fps(), "size": [w, h], "frames": e.total_frames() })
            .to_string()
    }

    /// Current metas of all scenes as JSON (`{scene: meta}`).
    pub fn meta(&mut self) -> String {
        let metas: BTreeMap<String, Meta> = self.session.engine().metas().into_iter().collect();
        serde_json::to_string(&metas).expect("meta json")
    }
}

fn js_now() -> f64 {
    // Commands that start playback use the same clock as `tick`.
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = performance, js_name = now)]
        fn performance_now() -> f64;
    }
    performance_now() / 1000.0
}
