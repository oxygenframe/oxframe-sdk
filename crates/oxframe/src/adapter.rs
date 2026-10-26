//! [`PreviewEngine`] implementation shared by the native preview server
//! and the in-browser (wasm) engine.

use oxframe_2d::NodeId;
use oxframe_core::{FrameSource, Image, Point};
use oxframe_preview::{MetaEdit, PreviewEngine};
use oxframe_protocol::{AudioClipInfo, MarkerInfo, Meta, NodeInfo, SceneInfo, SignalValue};
use oxframe_render::Renderer;

use crate::Engine;
use crate::engine::MetaSection;

/// An [`Engine`] plus a renderer, as seen by the preview session.
pub struct PreviewAdapter {
    engine: Engine,
    renderer: Box<dyn Renderer>,
    waveform: Option<Option<Vec<f32>>>,
    pcm: Option<Option<(u32, Vec<i16>)>>,
}

impl PreviewAdapter {
    /// Wraps an engine; frames are drawn with `renderer`.
    pub fn new(engine: Engine, renderer: Box<dyn Renderer>) -> Self {
        Self {
            engine,
            renderer,
            waveform: None,
            pcm: None,
        }
    }
}

impl FrameSource for PreviewAdapter {
    fn fps(&self) -> u32 {
        self.engine.fps()
    }

    fn size(&self) -> (u32, u32) {
        self.engine.size()
    }

    fn total_frames(&self) -> u32 {
        self.engine.total_frames()
    }

    fn render(&mut self, frame: u32, size: (u32, u32)) -> Image {
        self.engine.render(frame, self.renderer.as_mut(), size)
    }
}

impl PreviewEngine for PreviewAdapter {
    fn engine_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    fn scenes(&self) -> Vec<SceneInfo> {
        self.engine.scenes()
    }

    fn markers(&self) -> Vec<MarkerInfo> {
        self.engine.markers()
    }

    fn metas(&self) -> Vec<(String, Meta)> {
        self.engine.metas()
    }

    fn edit(&mut self, edit: MetaEdit) -> Result<Vec<(String, Meta)>, String> {
        let changed = match edit {
            MetaEdit::Marker { name, time, scene } => {
                let t = time.max(0.0);
                if !oxframe_protocol::is_valid_name(&name) {
                    return Err(format!("invalid marker name `{name}`: use [a-z0-9_-]"));
                }
                self.engine.edit_meta(
                    MetaSection::Markers,
                    &name.clone(),
                    scene.as_deref(),
                    |m| {
                        m.markers.insert(name, t);
                    },
                )?
            }
            MetaEdit::Ease { name, value } => {
                self.engine
                    .edit_meta(MetaSection::Eases, &name.clone(), None, |m| {
                        m.eases.insert(name, value);
                    })?
            }
            MetaEdit::Tunable { name, value } => {
                self.engine
                    .edit_meta(MetaSection::Tunables, &name.clone(), None, |m| {
                        m.tunables.insert(name, value);
                    })?
            }
            MetaEdit::Transition { scene, seconds } => {
                self.engine.set_transition_seconds(&scene, seconds)?
            }
        };
        Ok(vec![changed])
    }

    fn pick(&mut self, frame: u32, x: f64, y: f64) -> Option<u32> {
        let p = self.engine.pixels_to_scene(x, y);
        self.engine.seek(frame).pick(p).map(|id| id.0)
    }

    fn tree(&mut self, frame: u32) -> Vec<NodeInfo> {
        let graph = self.engine.seek(frame);
        graph
            .tree()
            .into_iter()
            .map(|(id, parent, kind, name)| NodeInfo {
                id: id.0,
                parent: parent.map(|p| p.0),
                kind: kind.to_owned(),
                name,
                source: graph.node(id).source.clone(),
            })
            .collect()
    }

    fn inspect(&mut self, frame: u32, node: u32) -> Option<(Vec<SignalValue>, [f64; 8])> {
        let (w, h) = self.engine.size();
        let graph = self.engine.seek(frame);
        let id = NodeId(node);
        if node as usize >= graph.len() {
            return None;
        }
        let values = graph
            .describe(id)
            .into_iter()
            .map(|(name, value)| SignalValue { name, value })
            .collect();
        let tf = graph.world_transform(id)?;
        let (bw, bh) = graph.measure(id);
        let corners = [
            (-bw / 2.0, -bh / 2.0),
            (bw / 2.0, -bh / 2.0),
            (bw / 2.0, bh / 2.0),
            (-bw / 2.0, bh / 2.0),
        ];
        let mut bounds = [0.0; 8];
        for (i, (x, y)) in corners.into_iter().enumerate() {
            let p = tf * Point::new(x, y);
            bounds[2 * i] = p.x + f64::from(w) / 2.0;
            bounds[2 * i + 1] = p.y + f64::from(h) / 2.0;
        }
        Some((values, bounds))
    }

    fn waveform(&mut self, buckets: usize) -> Option<Vec<f32>> {
        self.ensure_audio(buckets);
        self.waveform.clone().flatten()
    }

    fn slides(&self) -> Vec<oxframe_protocol::SlideInfo> {
        self.engine.slides()
    }

    fn audio_clips(&self) -> Vec<AudioClipInfo> {
        let mut clips: Vec<AudioClipInfo> = self
            .engine
            .scheduled()
            .into_iter()
            .filter(|sound| !sound.speech)
            .map(|sound| AudioClipInfo {
                name: std::path::Path::new(&sound.file)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(sound.file.as_str())
                    .to_owned(),
                start: sound.start,
                seconds: sound.known_seconds().unwrap_or(0.0),
                priority: sound.priority,
                text: None,
            })
            .collect();
        for line in self.engine.voices() {
            clips.push(AudioClipInfo {
                name: line.text.clone(),
                start: line.start,
                seconds: (line.end - line.start).max(0.0),
                priority: true,
                text: Some(line.text),
            });
        }
        clips
    }

    fn audio_pcm(&mut self) -> Option<(u32, Vec<i16>)> {
        self.ensure_audio(1);
        self.pcm.clone().flatten()
    }
}

impl PreviewAdapter {
    fn ensure_audio(&mut self, buckets: usize) {
        if self.waveform.is_some() {
            return;
        }
        let buckets = buckets.clamp(1, 4096);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let seconds =
                f64::from(self.engine.total_frames()) / f64::from(self.engine.fps().max(1));
            match crate::audio::mix_range(&self.engine, 0.0, seconds) {
                Ok(Some(mixed)) => {
                    self.waveform = Some(Some(oxframe_export::envelope(&mixed.samples, buckets)));
                    let preview = oxframe_export::downsample(
                        &mixed.samples,
                        mixed.rate,
                        oxframe_export::PREVIEW_RATE,
                    );
                    self.pcm = Some(Some((oxframe_export::PREVIEW_RATE, preview)));
                    return;
                }
                Ok(None) => {}
                Err(e) => eprintln!("warning: mix: {e}"),
            }
            let wf = self.engine.audio().and_then(|(file, _)| {
                oxframe_export::waveform(std::path::Path::new(&file), buckets, None)
                    .map_err(|e| eprintln!("warning: waveform: {e}"))
                    .ok()
            });
            self.waveform = Some(wf);
            self.pcm = Some(None);
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = buckets;
            self.waveform = Some(None);
            self.pcm = Some(None);
        }
    }
}
