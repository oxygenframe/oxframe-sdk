//! Preview protocol for oxframe.
//!
//! > **Status:** oxframe is under very active development. The API is not stable yet and
//! > may change from version to version. Pin the exact version in your Cargo.toml and check
//! > the [changelog](https://github.com/oxygenframe/oxframe-sdk/blob/main/CHANGELOG.md)
//! > before upgrading.
//!
//! [`Session`] implements the editor protocol (`oxframe-protocol`) on top of
//! any [`PreviewEngine`] and is independent of the transport:
//!
//! - natively, [`server`] exposes it over a WebSocket on `127.0.0.1`
//!   (frames are JPEG-encoded and cached);
//! - in the browser, the same session runs inside wasm and frames go to a
//!   canvas as raw RGBA.

#![warn(missing_docs)]

use std::collections::VecDeque;

use oxframe_core::{FrameSource, Image};
use oxframe_protocol::{
    AudioClipInfo, Command, ErrorKind, Event, FrameFormat, FrameHeader, MarkerInfo, Meta, NodeInfo,
    PROTOCOL_VERSION, SceneInfo, SignalValue, SlideInfo,
};

#[cfg(all(feature = "server", not(target_arch = "wasm32")))]
pub mod server;

/// An edit of a scene's `meta.json` requested by the editor.
#[derive(Debug, Clone, PartialEq)]
pub enum MetaEdit {
    /// Move or create a marker.
    Marker {
        /// Marker name.
        name: String,
        /// Seconds from the start of the scene.
        time: f64,
        /// Scene to create it in if it does not exist.
        scene: Option<String>,
    },
    /// Change a named ease.
    Ease {
        /// Ease name.
        name: String,
        /// Control points `[x1, y1, x2, y2]`.
        value: [f64; 4],
    },
    /// Change a tunable.
    Tunable {
        /// Tunable name.
        name: String,
        /// New value.
        value: f64,
    },
    /// Change how long a scene's incoming transition lasts.
    Transition {
        /// Scene the transition belongs to.
        scene: String,
        /// Blend length in seconds. Zero removes it.
        seconds: f64,
    },
}

/// What the preview needs from an engine, on top of drawing frames
/// ([`FrameSource`]). Implemented by `oxframe::PreviewAdapter`.
pub trait PreviewEngine: FrameSource {
    /// Version string sent in `Ready`.
    fn engine_version(&self) -> String;
    /// Scenes with their first frame and length.
    fn scenes(&self) -> Vec<SceneInfo>;
    /// Markers of all scenes.
    fn markers(&self) -> Vec<MarkerInfo>;
    /// Meta of every scene (sent on `Ready` so the editor knows eases and
    /// tunables).
    fn metas(&self) -> Vec<(String, Meta)>;
    /// Applies an edit, re-simulates, persists the meta; returns changed
    /// metas as `(scene, meta)`.
    ///
    /// # Errors
    ///
    /// A message for the editor if the edit is invalid or saving failed.
    fn edit(&mut self, edit: MetaEdit) -> Result<Vec<(String, Meta)>, String>;
    /// Hit-test at frame pixels (top-left origin, project size).
    fn pick(&mut self, frame: u32, x: f64, y: f64) -> Option<u32>;
    /// Node tree of the scene at `frame`.
    fn tree(&mut self, frame: u32) -> Vec<NodeInfo>;
    /// Signal values and selection box (frame pixels) of a node.
    fn inspect(&mut self, frame: u32, node: u32) -> Option<(Vec<SignalValue>, [f64; 8])>;
    /// Audio peak envelope for the timeline, if the project has audio.
    fn waveform(&mut self, buckets: usize) -> Option<Vec<f32>>;
    /// Sound effects, for the timeline bars.
    fn audio_clips(&self) -> Vec<AudioClipInfo> {
        Vec::new()
    }
    /// Stops for `oxframe present`.
    fn slides(&self) -> Vec<SlideInfo> {
        Vec::new()
    }
    /// Mixed preview audio: sample rate and mono s16le samples.
    fn audio_pcm(&mut self) -> Option<(u32, Vec<i16>)> {
        None
    }
}

/// A message for the UI: JSON event or binary frame.
#[derive(Debug, Clone)]
pub enum Output {
    /// A JSON event.
    Event(Event),
    /// Binary WS message: 16-byte header followed by the body.
    Frame(Vec<u8>),
}

/// Encodes a rendered frame for the wire.
pub type Encoder = Box<dyn FnMut(&Image) -> (FrameFormat, Vec<u8>)>;

/// (frame, width, height, meta version).
type CacheKey = (u32, u32, u32, u64);

/// Frame cache bounded by total bytes.
struct FrameCache {
    limit: usize,
    used: usize,
    entries: VecDeque<(CacheKey, Vec<u8>)>,
}

impl FrameCache {
    fn get(&mut self, key: CacheKey) -> Option<Vec<u8>> {
        let pos = self.entries.iter().position(|(k, _)| *k == key)?;
        let entry = self.entries.remove(pos)?;
        let bytes = entry.1.clone();
        self.entries.push_back(entry);
        Some(bytes)
    }

    fn put(&mut self, key: CacheKey, bytes: Vec<u8>) {
        self.used += bytes.len();
        self.entries.push_back((key, bytes));
        while self.used > self.limit {
            let Some((_, old)) = self.entries.pop_front() else {
                break;
            };
            self.used -= old.len();
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.used = 0;
    }
}

/// Transport-independent preview state machine: feed it [`Command`]s, send its [`Output`]s to the UI.
pub struct Session<E> {
    engine: E,
    encode: Encoder,
    frame: u32,
    playing: Option<(f64, u32)>,
    looping: bool,
    viewport: (u32, u32),
    selected: Option<u32>,
    /// Bumped on every meta edit; part of the cache key.
    version: u64,
    cache: FrameCache,
    last_tree: Vec<NodeInfo>,
}

fn encode_pcm(rate: u32, samples: &[i16]) -> Vec<u8> {
    let mut body = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        body.extend_from_slice(&sample.to_le_bytes());
    }
    let header = FrameHeader {
        frame: 0,
        width: u16::try_from(rate).unwrap_or(u16::MAX),
        height: 1,
        format: FrameFormat::Pcm,
        len: u32::try_from(body.len()).unwrap_or(u32::MAX),
    };
    let mut bytes = header.encode().to_vec();
    bytes.extend(body);
    bytes
}

impl<E: PreviewEngine> Session<E> {
    /// `cache_mb` bounds the frame cache.
    pub fn new(engine: E, encode: Encoder, cache_mb: usize) -> Self {
        let size = engine.size();
        Self {
            engine,
            encode,
            frame: 0,
            playing: None,
            looping: true,
            viewport: (size.0.min(960), size.1.min(540)),
            selected: None,
            version: 0,
            cache: FrameCache {
                limit: cache_mb * 1024 * 1024,
                used: 0,
                entries: VecDeque::new(),
            },
            last_tree: Vec::new(),
        }
    }

    /// The wrapped engine.
    pub fn engine(&mut self) -> &mut E {
        &mut self.engine
    }

    /// Current frame.
    pub fn frame(&self) -> u32 {
        self.frame
    }

    /// Whether playback is running.
    pub fn is_playing(&self) -> bool {
        self.playing.is_some()
    }

    /// Size of preview frames: the viewport, fitted to the project aspect
    /// and never larger than the project.
    pub fn render_size(&self) -> (u32, u32) {
        let (w, h) = self.engine.size();
        let (vw, vh) = self.viewport;
        let k = (f64::from(vw) / f64::from(w))
            .min(f64::from(vh) / f64::from(h))
            .min(1.0);
        (
            ((f64::from(w) * k).round() as u32).max(1),
            ((f64::from(h) * k).round() as u32).max(1),
        )
    }

    fn ready(&mut self) -> Vec<Output> {
        let (w, h) = self.engine.size();
        let mut out = vec![Output::Event(Event::Ready {
            protocol_version: PROTOCOL_VERSION,
            engine_version: self.engine.engine_version(),
            fps: self.engine.fps(),
            size: [w, h],
            scenes: self.engine.scenes(),
        })];
        out.extend(
            self.engine
                .metas()
                .into_iter()
                .map(|(scene, meta)| Output::Event(Event::MetaChanged { scene, meta })),
        );
        out.extend(self.timeline(true));
        out.extend(self.show());
        out
    }

    fn timeline(&mut self, with_audio: bool) -> Vec<Output> {
        let frames = self.engine.total_frames();
        let mut out = vec![Output::Event(Event::Timeline {
            frames,
            markers: self.engine.markers(),
            scenes: self.engine.scenes(),
            audio_waveform: if with_audio {
                self.engine.waveform((frames as usize).clamp(1, 4096))
            } else {
                None
            },
            audio_clips: self.engine.audio_clips(),
            slides: self.engine.slides(),
        })];
        if with_audio && let Some((rate, samples)) = self.engine.audio_pcm() {
            out.push(Output::Frame(encode_pcm(rate, &samples)));
        }
        out
    }

    /// Frame plus everything that depends on it.
    fn show(&mut self) -> Vec<Output> {
        let total = self.engine.total_frames();
        self.frame = self.frame.min(total.saturating_sub(1));
        let size = self.render_size();
        let key = (self.frame, size.0, size.1, self.version);
        let body = match self.cache.get(key) {
            Some(b) => b,
            None => {
                let img = self.engine.render(self.frame, size);
                let (format, bytes) = (self.encode)(&img);
                let header = FrameHeader {
                    frame: self.frame,
                    width: img.width as u16,
                    height: img.height as u16,
                    format,
                    len: bytes.len() as u32,
                };
                let mut msg = Vec::with_capacity(FrameHeader::SIZE + bytes.len());
                msg.extend_from_slice(&header.encode());
                msg.extend_from_slice(&bytes);
                // Surface frames live in the canvas, not in the message.
                if format != FrameFormat::Surface {
                    self.cache.put(key, msg.clone());
                }
                msg
            }
        };
        let mut out = vec![
            Output::Event(Event::Frame { index: self.frame }),
            Output::Frame(body),
        ];
        let tree = self.engine.tree(self.frame);
        if tree != self.last_tree {
            self.last_tree = tree.clone();
            out.push(Output::Event(Event::Tree { nodes: tree }));
        }
        if let Some(id) = self.selected {
            match self.engine.inspect(self.frame, id) {
                Some((values, bounds)) => out.push(Output::Event(Event::Signals {
                    node_id: id,
                    values,
                    bounds: Some(bounds),
                })),
                None => {
                    self.selected = None;
                    out.push(Output::Event(Event::Selected { node_id: None }));
                }
            }
        }
        out
    }

    fn error(kind: ErrorKind, message: impl Into<String>) -> Output {
        Output::Event(Event::Error {
            kind,
            message: message.into(),
            spans: Vec::new(),
        })
    }

    /// Handles one command. `now` is a monotonic clock in seconds, used to
    /// pace playback.
    pub fn handle(&mut self, cmd: Command, now: f64) -> Vec<Output> {
        match cmd {
            Command::Hello {
                protocol_version, ..
            } => {
                if protocol_version != PROTOCOL_VERSION {
                    return vec![Self::error(
                        ErrorKind::Incompatible,
                        format!(
                            "editor speaks protocol v{protocol_version}, engine speaks v{PROTOCOL_VERSION}; \
                             update oxframe-cli or the SDK so they match"
                        ),
                    )];
                }
                // A new page (a reload) shows the player paused; it must
                // not get the frames of a playback it did not start.
                self.playing = None;
                self.ready()
            }
            Command::Seek { frame } => {
                self.frame = frame;
                if let Some((_, _)) = self.playing {
                    self.playing = Some((now, frame));
                }
                self.show()
            }
            Command::Step { delta } => {
                self.playing = None;
                self.frame = self.frame.saturating_add_signed(delta);
                let mut out = vec![self.playback()];
                out.extend(self.show());
                out
            }
            Command::Play => {
                if self.frame + 1 >= self.engine.total_frames() {
                    self.frame = 0;
                }
                self.playing = Some((now, self.frame));
                let mut out = vec![self.playback()];
                out.extend(self.show());
                out
            }
            Command::Pause => {
                self.playing = None;
                vec![self.playback()]
            }
            Command::SetViewport { w, h, dpr } => {
                let dpr = if dpr.is_finite() && dpr > 0.0 {
                    dpr
                } else {
                    1.0
                };
                self.viewport = (
                    ((f64::from(w) * dpr) as u32).max(1),
                    ((f64::from(h) * dpr) as u32).max(1),
                );
                self.show()
            }
            Command::SetMarker { name, time, scene } => {
                self.edit(MetaEdit::Marker { name, time, scene })
            }
            Command::SetEase { name, value } => self.edit(MetaEdit::Ease { name, value }),
            Command::SetTunable { name, value } => self.edit(MetaEdit::Tunable { name, value }),
            Command::SetTransition { scene, seconds } => {
                self.edit(MetaEdit::Transition { scene, seconds })
            }
            Command::Pick { x, y } => {
                self.selected = self.engine.pick(self.frame, x, y);
                let mut out = vec![Output::Event(Event::Selected {
                    node_id: self.selected,
                })];
                out.extend(self.show());
                out
            }
            Command::Select { node_id } => {
                self.selected = node_id;
                let mut out = vec![Output::Event(Event::Selected { node_id })];
                out.extend(self.show());
                out
            }
            // Handled by the transport (process exit natively).
            Command::Shutdown => Vec::new(),
        }
    }

    fn playback(&self) -> Output {
        Output::Event(Event::Playback {
            playing: self.playing.is_some(),
            frame: self.frame,
        })
    }

    fn edit(&mut self, edit: MetaEdit) -> Vec<Output> {
        match self.engine.edit(edit) {
            Ok(changed) => {
                self.version += 1;
                self.cache.clear();
                let mut out: Vec<Output> = changed
                    .into_iter()
                    .map(|(scene, meta)| Output::Event(Event::MetaChanged { scene, meta }))
                    .collect();
                out.extend(self.timeline(false));
                out.extend(self.show());
                out
            }
            Err(e) => vec![Self::error(ErrorKind::Meta, e)],
        }
    }

    /// Advances playback to wall-clock `now`; returns the new frame's
    /// outputs, or nothing if the frame did not change.
    pub fn tick(&mut self, now: f64) -> Vec<Output> {
        let Some((start, from)) = self.playing else {
            return Vec::new();
        };
        let total = self.engine.total_frames();
        let elapsed = ((now - start) * f64::from(self.engine.fps()))
            .floor()
            .max(0.0) as u32;
        let mut target = from.saturating_add(elapsed);
        if target >= total {
            if self.looping && total > 0 {
                target %= total;
                self.playing = Some((now, target));
            } else {
                self.playing = None;
                self.frame = total.saturating_sub(1);
                let mut out = vec![self.playback()];
                out.extend(self.show());
                return out;
            }
        }
        if target == self.frame {
            return Vec::new();
        }
        self.frame = target;
        self.show()
    }

    /// Seconds until the next playback frame is due, if playing.
    pub fn next_tick_in(&self, now: f64) -> Option<f64> {
        let (start, _) = self.playing?;
        let dt = 1.0 / f64::from(self.engine.fps());
        let elapsed = now - start;
        Some(dt - elapsed.rem_euclid(dt))
    }
}

/// Raw RGBA "encoding" (in-process transports, tests).
pub fn rgba_encoder() -> Encoder {
    Box::new(|img: &Image| (FrameFormat::Rgba, img.data.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        renders: u32,
        markers: Vec<MarkerInfo>,
    }

    impl FrameSource for Fake {
        fn fps(&self) -> u32 {
            10
        }
        fn size(&self) -> (u32, u32) {
            (200, 100)
        }
        fn total_frames(&self) -> u32 {
            20
        }
        fn render(&mut self, frame: u32, size: (u32, u32)) -> Image {
            self.renders += 1;
            Image {
                width: size.0,
                height: size.1,
                data: vec![frame as u8; (size.0 * size.1 * 4) as usize],
            }
        }
    }

    impl PreviewEngine for Fake {
        fn engine_version(&self) -> String {
            "test".into()
        }
        fn scenes(&self) -> Vec<SceneInfo> {
            vec![SceneInfo {
                name: "a".into(),
                start: 0,
                frames: 20,
                transition: 0,
                transition_at: 0,
            }]
        }
        fn markers(&self) -> Vec<MarkerInfo> {
            self.markers.clone()
        }
        fn metas(&self) -> Vec<(String, Meta)> {
            vec![("a".into(), Meta::default())]
        }
        fn edit(&mut self, edit: MetaEdit) -> Result<Vec<(String, Meta)>, String> {
            match edit {
                MetaEdit::Marker { name, time, .. } => {
                    self.markers = vec![MarkerInfo {
                        scene: "a".into(),
                        name,
                        time,
                    }];
                    Ok(vec![("a".into(), Meta::default())])
                }
                _ => Err("unsupported".into()),
            }
        }
        fn pick(&mut self, _: u32, x: f64, _: f64) -> Option<u32> {
            (x < 100.0).then_some(1)
        }
        fn tree(&mut self, _: u32) -> Vec<NodeInfo> {
            Vec::new()
        }
        fn inspect(&mut self, _: u32, node: u32) -> Option<(Vec<SignalValue>, [f64; 8])> {
            Some((
                vec![SignalValue {
                    name: "id".into(),
                    value: node.to_string(),
                }],
                [0.0; 8],
            ))
        }
        fn waveform(&mut self, _: usize) -> Option<Vec<f32>> {
            None
        }
    }

    fn session() -> Session<Fake> {
        Session::new(
            Fake {
                renders: 0,
                markers: Vec::new(),
            },
            rgba_encoder(),
            16,
        )
    }

    fn events(out: &[Output]) -> Vec<String> {
        out.iter()
            .map(|o| match o {
                Output::Event(e) => serde_json::to_value(e).unwrap()["type"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
                Output::Frame(b) => format!("bin{}", FrameHeader::decode(b).unwrap().frame),
            })
            .collect()
    }

    #[test]
    fn handshake_and_version_mismatch() {
        let mut s = session();
        let out = s.handle(
            Command::Hello {
                protocol_version: PROTOCOL_VERSION,
                ui_version: "x".into(),
            },
            0.0,
        );
        assert_eq!(
            &events(&out)[..5],
            ["Ready", "MetaChanged", "Timeline", "Frame", "bin0"]
        );
        let bad = s.handle(
            Command::Hello {
                protocol_version: 999,
                ui_version: "x".into(),
            },
            0.0,
        );
        assert!(matches!(
            &bad[0],
            Output::Event(Event::Error {
                kind: ErrorKind::Incompatible,
                ..
            })
        ));
    }

    #[test]
    fn a_new_editor_stops_playback() {
        let mut s = session();
        s.handle(Command::Play, 10.0);
        assert!(s.is_playing());
        s.handle(
            Command::Hello {
                protocol_version: PROTOCOL_VERSION,
                ui_version: "x".into(),
            },
            11.0,
        );
        assert!(!s.is_playing());
        assert!(s.tick(12.0).is_empty());
    }

    #[test]
    fn viewport_fits_aspect_and_cache_hits() {
        let mut s = session();
        s.handle(
            Command::SetViewport {
                w: 50,
                h: 50,
                dpr: 2.0,
            },
            0.0,
        );
        assert_eq!(s.render_size(), (100, 50));
        let renders = s.engine().renders;
        s.handle(Command::Seek { frame: 0 }, 0.0);
        assert_eq!(s.engine().renders, renders, "served from cache");
        s.handle(Command::Seek { frame: 5 }, 0.0);
        assert_eq!(s.engine().renders, renders + 1);
    }

    #[test]
    fn playback_follows_the_clock_and_loops() {
        let mut s = session();
        s.handle(Command::Seek { frame: 0 }, 0.0);
        s.handle(Command::Play, 10.0);
        assert!(s.tick(10.05).is_empty(), "half a frame later");
        assert_eq!(events(&s.tick(10.35))[..2], ["Frame", "bin3"]);
        s.tick(12.15); // 21.5 frames in: loops to 1
        assert_eq!(s.frame(), 1);
        s.handle(Command::Pause, 13.0);
        assert!(!s.is_playing());
    }

    #[test]
    fn edits_invalidate_cache_and_report_meta() {
        let mut s = session();
        s.handle(Command::Seek { frame: 2 }, 0.0);
        let out = s.handle(
            Command::SetMarker {
                name: "beat".into(),
                time: 1.0,
                scene: None,
            },
            0.0,
        );
        assert_eq!(
            &events(&out)[..4],
            ["MetaChanged", "Timeline", "Frame", "bin2"]
        );
        let err = s.handle(
            Command::SetTunable {
                name: "x".into(),
                value: 1.0,
            },
            0.0,
        );
        assert!(matches!(
            &err[0],
            Output::Event(Event::Error {
                kind: ErrorKind::Meta,
                ..
            })
        ));
    }

    #[test]
    fn pick_selects_and_streams_signals() {
        let mut s = session();
        let out = s.handle(Command::Pick { x: 10.0, y: 10.0 }, 0.0);
        let names = events(&out);
        assert_eq!(names[0], "Selected");
        assert!(names.contains(&"Signals".to_string()));
    }
}
