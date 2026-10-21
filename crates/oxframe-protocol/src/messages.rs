use serde::{Deserialize, Serialize};

use crate::Meta;

/// UI → engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type")]
pub enum Command {
    /// First message after connecting; answered with `Ready`.
    Hello {
        /// [`PROTOCOL_VERSION`](crate::PROTOCOL_VERSION) of the UI.
        protocol_version: u32,
        /// Version of the editor build, for error messages.
        ui_version: String,
    },
    /// Show this frame.
    Seek {
        /// Global frame index.
        frame: u32,
    },
    /// Start playback from the current frame.
    Play,
    /// Stop playback.
    Pause,
    /// Move by a number of frames.
    Step {
        /// Frames to move; negative goes back.
        delta: i32,
    },
    /// Size the preview is shown at; frames are rendered at `w·dpr × h·dpr`,
    /// capped at the project size.
    SetViewport {
        /// Width in CSS pixels (including the editor's zoom).
        w: u32,
        /// Height in CSS pixels (including the editor's zoom).
        h: u32,
        /// Device pixel ratio.
        dpr: f64,
    },
    /// Moves a marker (seconds from the start of its scene). With `scene`,
    /// creates the marker there if it does not exist yet.
    SetMarker {
        /// Marker name.
        name: String,
        /// Seconds from the start of the scene.
        time: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// Scene to create the marker in.
        scene: Option<String>,
    },
    /// Changes a named ease.
    SetEase {
        /// Ease name.
        name: String,
        /// Control points `[x1, y1, x2, y2]`.
        value: [f64; 4],
    },
    /// Changes a tunable.
    SetTunable {
        /// Tunable name.
        name: String,
        /// New value.
        value: f64,
    },
    /// Sets how long the transition into `scene` lasts, in seconds.
    SetTransition {
        /// Scene the transition belongs to.
        scene: String,
        /// Blend length. Zero removes it.
        seconds: f64,
    },
    /// Hit-test at a point in frame pixels of the project size (origin
    /// top-left). Answered with `Selected`.
    Pick {
        /// Horizontal position.
        x: f64,
        /// Vertical position.
        y: f64,
    },
    /// Selects a node in the inspector, or clears the selection.
    Select {
        /// Node to select; `None` clears.
        node_id: Option<u32>,
    },
    /// Asks the engine to exit (native preview server).
    Shutdown,
}

/// Engine → UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type")]
pub enum Event {
    /// Answer to `Hello`: the engine and the project.
    Ready {
        /// [`PROTOCOL_VERSION`](crate::PROTOCOL_VERSION) of the engine.
        protocol_version: u32,
        /// Version of the oxframe crate.
        engine_version: String,
        /// Frames per second.
        fps: u32,
        /// Project size in pixels, `[width, height]`.
        size: [u32; 2],
        /// Scenes in order.
        scenes: Vec<SceneInfo>,
    },
    /// A new build is running (sent after a hot reload).
    Reloaded,
    /// The CLI started rebuilding the project.
    Building,
    /// Something failed; the last good state stays visible.
    Error {
        /// What failed.
        kind: ErrorKind,
        /// Human-readable message.
        message: String,
        #[serde(default)]
        /// Source locations, for compile errors.
        spans: Vec<Span>,
    },
    /// Followed by a binary frame (see [`FrameHeader`]).
    Frame {
        /// Global frame index.
        index: u32,
    },
    /// Timeline layout; sent on start and when it changes.
    Timeline {
        /// Total number of frames.
        frames: u32,
        /// All markers of all scenes.
        markers: Vec<MarkerInfo>,
        /// Scenes in order.
        scenes: Vec<SceneInfo>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// Peak amplitudes of the soundtrack in `0..=1`, one per frame (at most 4096).
        audio_waveform: Option<Vec<f32>>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        /// Sound effects on the timeline.
        audio_clips: Vec<AudioClipInfo>,
    },
    /// The node tree of the current frame's scene.
    Tree {
        /// Nodes in tree order (parents before children).
        nodes: Vec<NodeInfo>,
    },
    /// Current values of the selected node; sent after each frame while
    /// something is selected.
    Signals {
        /// The selected node.
        node_id: u32,
        /// One entry per property.
        values: Vec<SignalValue>,
        /// Selection box as four corners `[x0, y0, x1, y1, x2, y2, x3, y3]`
        /// in frame pixels (rotated nodes give a rotated box).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bounds: Option<[f64; 8]>,
    },
    /// Selection changed (answer to `Pick` and `Select`).
    Selected {
        /// Selected node, if any.
        node_id: Option<u32>,
    },
    /// Playback state changed (also sent while playing).
    Playback {
        /// Whether playback is running.
        playing: bool,
        /// Current global frame.
        frame: u32,
    },
    /// A scene's meta changed and was saved.
    MetaChanged {
        /// Scene name.
        scene: String,
        /// New contents.
        meta: Meta,
    },
}

/// Category of an [`Event::Error`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ErrorKind {
    /// Protocol versions of the UI and engine differ.
    Incompatible,
    /// The project does not compile.
    Build,
    /// The scene panicked or failed.
    Runtime,
    /// A `meta.json` file is invalid.
    Meta,
}

/// A source location in an error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Span {
    /// Path relative to the project.
    pub file: String,
    /// 1-based line.
    pub line: u32,
    /// 1-based column.
    pub column: u32,
    #[serde(default)]
    /// Label for this location, may be empty.
    pub message: String,
}

/// A scene on the timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SceneInfo {
    /// Scene name (its function name).
    pub name: String,
    /// First global frame of the scene.
    pub start: u32,
    /// Length in frames.
    pub frames: u32,
    #[serde(default)]
    /// Incoming blend length in frames. 0 if this scene has no transition.
    pub transition: u32,
    #[serde(default)]
    /// Frame inside the scene where that blend starts.
    pub transition_at: u32,
}

/// A sound effect on the timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AudioClipInfo {
    /// File name.
    pub name: String,
    /// Start on the project timeline, in seconds.
    pub start: f64,
    /// Length in seconds. `0` when the file was not measured.
    pub seconds: f64,
    /// This clip ducks the soundtrack.
    pub priority: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Spoken text, when this clip is a line.
    pub text: Option<String>,
}

/// A marker on the timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MarkerInfo {
    /// Scene the marker belongs to.
    pub scene: String,
    /// Marker name.
    pub name: String,
    /// Seconds from the start of its scene.
    pub time: f64,
}

/// A node in [`Event::Tree`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct NodeInfo {
    /// Node index.
    pub id: u32,
    /// Parent index; `None` for the root.
    pub parent: Option<u32>,
    /// Node type, e.g. `"Rect"`.
    pub kind: String,
    /// Name set with `.name(..)`.
    pub name: Option<String>,
}

/// A property value in [`Event::Signals`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SignalValue {
    /// Property name.
    pub name: String,
    /// Human-readable value, e.g. `"120.5"` or `"#ff0000ff"`.
    pub value: String,
}

/// Image encoding of a binary preview frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameFormat {
    /// WebP image.
    Webp = 1,
    /// JPEG image.
    Jpeg = 2,
    /// Raw RGBA8, `width * height * 4` bytes.
    Rgba = 3,
    /// No body: the frame was drawn directly into the transport's surface
    /// (an OffscreenCanvas in the browser).
    Surface = 4,
    /// Signed 16-bit little-endian mono PCM. `width` is the sample rate,
    /// `height` is 1, and the body is the samples.
    Pcm = 5,
}

impl FrameFormat {
    /// Parses the header byte; `None` for unknown values.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Webp),
            2 => Some(Self::Jpeg),
            3 => Some(Self::Rgba),
            4 => Some(Self::Surface),
            5 => Some(Self::Pcm),
            _ => None,
        }
    }
}

/// 16-byte little-endian header of a binary WS frame:
///
/// | bytes  | field    |
/// |--------|----------|
/// | 0..4   | `frame`  |
/// | 4..6   | `width`  |
/// | 6..8   | `height` |
/// | 8      | `format` |
/// | 9..12  | reserved, zero |
/// | 12..16 | `len` of the body that follows |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    /// Global frame index.
    pub frame: u32,
    /// Image width in pixels.
    pub width: u16,
    /// Image height in pixels.
    pub height: u16,
    /// Encoding of the body.
    pub format: FrameFormat,
    /// Body length in bytes.
    pub len: u32,
}

impl FrameHeader {
    /// Header length in bytes.
    pub const SIZE: usize = 16;

    /// Serializes the header.
    pub fn encode(&self) -> [u8; Self::SIZE] {
        let mut out = [0u8; Self::SIZE];
        out[0..4].copy_from_slice(&self.frame.to_le_bytes());
        out[4..6].copy_from_slice(&self.width.to_le_bytes());
        out[6..8].copy_from_slice(&self.height.to_le_bytes());
        out[8] = self.format as u8;
        out[12..16].copy_from_slice(&self.len.to_le_bytes());
        out
    }

    /// Parses a header from the start of `bytes`; `None` if too short or unknown format.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let b: &[u8; Self::SIZE] = bytes.get(..Self::SIZE)?.try_into().ok()?;
        Some(Self {
            frame: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            width: u16::from_le_bytes([b[4], b[5]]),
            height: u16::from_le_bytes([b[6], b[7]]),
            format: FrameFormat::from_u8(b[8])?,
            len: u32::from_le_bytes([b[12], b[13], b[14], b[15]]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_json_shape() {
        let json = serde_json::to_string(&Command::Seek { frame: 42 }).unwrap();
        assert_eq!(json, r#"{"type":"Seek","frame":42}"#);
        let back: Command = serde_json::from_str(r#"{"type":"Play"}"#).unwrap();
        assert_eq!(back, Command::Play);
    }

    #[test]
    fn frame_header_roundtrip() {
        let h = FrameHeader {
            frame: 1234,
            width: 960,
            height: 540,
            format: FrameFormat::Webp,
            len: 65_000,
        };
        assert_eq!(FrameHeader::decode(&h.encode()), Some(h));
    }
}
