//! Spoken lines. A line waits out its words, and the same words mix into
//! the video when a provider or a committed cache has audio for them.
//!
//! Without a provider and without a cache, the length is a speech-rate
//! estimate. The playground and a CI run with no API key still build.

use std::cell::RefCell;
use std::future::Future;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use oxframe_core::{
    SoundClip, VoiceLine, WaitFor, now, record_sound, record_voice, time, wait_for,
};
use serde::{Deserialize, Serialize};

use crate::project::Project;
use crate::scene::SceneDef;

/// Words per second when no recording gives a real length.
pub const WORDS_PER_SECOND: f64 = 2.5;

const MIN_LINE: f64 = 0.3;

/// One word and where it sits inside the line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimedWord {
    /// The word, without surrounding punctuation.
    pub text: String,
    /// Start, in seconds from the beginning of the line.
    pub start: f64,
    /// End, in seconds from the beginning of the line.
    pub end: f64,
}

/// What a provider is asked to speak.
#[derive(Debug, Clone)]
pub struct TtsRequest<'a> {
    /// The line.
    pub text: &'a str,
    /// Voice name. Providers that ignore it still see it in the cache key.
    pub voice: &'a str,
}

/// Audio and word timings from a [`TtsProvider`].
#[derive(Debug, Clone)]
pub struct Synthesis {
    /// Samples per second of `samples`.
    pub rate: u32,
    /// Mono PCM.
    pub samples: Vec<i16>,
    /// Words in order. Times are seconds from the start of `samples`.
    pub words: Vec<TimedWord>,
}

/// Turns a line into audio and word timings.
///
/// Implement this for a service, then pass it to [`Project::tts`](crate::Project::tts).
/// The API key comes from the environment inside the provider, not from scene code.
/// A line is cached under `tts/` in the project, so the next render does not call
/// the provider again. Commit that folder to render in CI without a key.
pub trait TtsProvider: Send + Sync {
    /// Stable id. Part of the cache key, so two providers never share a file.
    fn id(&self) -> &str;
    /// Speak `req`.
    ///
    /// # Errors
    ///
    /// A message for the warning oxframe prints before it falls back to a
    /// speech-rate estimate. The scene still runs.
    fn synthesize(&self, req: &TtsRequest<'_>) -> Result<Synthesis, String>;
}

#[derive(Clone)]
pub(crate) struct Provider(pub Arc<dyn TtsProvider + Send + Sync>);

impl std::fmt::Debug for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.id())
    }
}

struct Ctx {
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    dir: PathBuf,
    voice: String,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    provider: Option<Provider>,
}

thread_local! {
    static CTX: RefCell<Option<Ctx>> = const { RefCell::new(None) };
}

pub(crate) fn install(project: &Project, def: &SceneDef) {
    let dir = project
        .tts_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from(def.location.manifest_dir).join("tts"));
    CTX.with(|slot| {
        *slot.borrow_mut() = Some(Ctx {
            dir,
            voice: project.speaker.clone(),
            provider: project.tts.clone(),
        });
    });
}

/// A line that has not started yet.
///
/// Awaiting it speaks and waits until the end. [`start`](Voice::start) speaks
/// and returns a handle, so the scene can wait for one word.
#[must_use = "a line does nothing until you call start() or await it"]
pub struct Voice {
    text: String,
    voice: Option<String>,
    caption: bool,
    recording: Option<String>,
    line: Option<Line>,
}

/// A spoken line, already on the timeline.
#[must_use = "a line does nothing unless awaited"]
pub struct Line {
    text: String,
    end: f64,
    words: Vec<TimedWord>,
    origin: f64,
    wait: Option<WaitFor>,
}

/// A line of speech, starting at the current frame.
///
/// The voice name comes from [`Project::speaker`](crate::Project::speaker)
/// unless `.speaker()` sets one.
pub fn voice(text: impl Into<String>) -> Voice {
    Voice {
        text: text.into(),
        voice: None,
        caption: false,
        recording: None,
        line: None,
    }
}

impl Voice {
    /// Speak with this voice instead of the project's.
    pub fn speaker(mut self, name: impl Into<String>) -> Self {
        self.voice = Some(name.into());
        self
    }

    /// Also show the line at the bottom of the frame while it is spoken, a
    /// few words at a time, each part on screen while its words are said.
    /// The caption is a node named `caption`, on top of the scene, removed
    /// when the line ends.
    pub fn caption(mut self) -> Self {
        self.caption = true;
        self
    }

    /// Play a recording of the line instead of calling the speech provider.
    ///
    /// The words are timed by `<file stem>.words.json` next to the file, which
    /// `oxframe transcribe` writes. Without it they are spread over the
    /// file's length. `text` is still what captions and subtitles show.
    ///
    /// # Panics
    ///
    /// If the file cannot be read and has no words file. Wasm and the
    /// playground estimate the length from the text and never open files.
    pub fn recording(mut self, path: impl Into<String>) -> Self {
        self.recording = Some(path.into());
        self
    }

    /// Start the line now and keep going. Await [`Line::at_word`] or the line itself.
    pub fn start(self) -> Line {
        self.begin()
    }

    fn begin(&self) -> Line {
        let voice_name = self.voice.clone().unwrap_or_else(|| {
            CTX.with(|slot| {
                slot.borrow()
                    .as_ref()
                    .map(|ctx| ctx.voice.clone())
                    .unwrap_or_else(|| "default".into())
            })
        });
        let spoken = match &self.recording {
            Some(path) => recorded(&self.text, path),
            None => resolve(&self.text, &voice_name),
        };
        let origin = time();
        record_voice(VoiceLine {
            text: self.text.clone(),
            at: now(),
            seconds: spoken.seconds,
        });
        if let Some(file) = spoken.file {
            record_sound(SoundClip {
                file,
                at: now(),
                volume: 1.0,
                fade_in: 0.0,
                fade_out: 0.0,
                trim_start: 0.0,
                trim_end: None,
                priority: true,
                speech: true,
            });
        }
        if self.caption {
            show_caption(&self.text, &spoken.words, spoken.seconds);
        }
        Line {
            text: self.text.clone(),
            end: origin + spoken.seconds,
            words: spoken.words,
            origin,
            wait: None,
        }
    }
}

impl Future for Voice {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if this.line.is_none() {
            this.line = Some(this.begin());
        }
        Pin::new(this.line.as_mut().expect("line was just set")).poll(cx)
    }
}

impl Line {
    /// Wait until `word` is spoken. The match ignores surrounding punctuation.
    ///
    /// # Panics
    ///
    /// If the line has no such word. The message lists the words it does have.
    pub fn at_word(&self, word: &str) -> WaitFor {
        let needle = bare(word);
        let found = self.words.iter().find(|w| w.text == needle);
        let Some(found) = found else {
            let have = self
                .words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            panic!(
                "oxframe: voice({:?}) has no word {word:?}. Words: {have}.",
                self.text
            );
        };
        let at = self.origin + found.start;
        wait_for((at - time()).max(0.0))
    }
}

impl Future for Line {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if this.wait.is_none() {
            this.wait = Some(wait_for((this.end - time()).max(0.0)));
        }
        Pin::new(this.wait.as_mut().expect("wait was just set")).poll(cx)
    }
}

/// Most words on screen at once.
const CAPTION_WORDS: usize = 7;
/// A shorter last part joins the one before it.
const CAPTION_MIN_WORDS: usize = 3;

/// The parts a caption shows in turn: (text, start, end) in seconds from
/// the start of the line. The words come from `text` when it splits into as
/// many words as were timed (keeping punctuation), else from the timings.
fn caption_parts(text: &str, words: &[TimedWord], seconds: f64) -> Vec<(String, f64, f64)> {
    let tokens: Vec<&str> = text
        .split_whitespace()
        .filter(|t| !bare(t).is_empty())
        .collect();
    let shown: Vec<&str> = if tokens.len() == words.len() {
        tokens
    } else {
        words.iter().map(|w| w.text.as_str()).collect()
    };
    let mut parts: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for (i, word) in shown.iter().enumerate() {
        let len = i + 1 - start;
        let sentence_end = word.ends_with(['.', '!', '?', ';', ':']);
        if len == CAPTION_WORDS || (sentence_end && len >= CAPTION_MIN_WORDS) {
            parts.push((start, i + 1));
            start = i + 1;
        }
    }
    if start < words.len() {
        match parts.last_mut() {
            Some(last) if words.len() - start < CAPTION_MIN_WORDS => last.1 = words.len(),
            _ => parts.push((start, words.len())),
        }
    }
    let count = parts.len();
    parts
        .into_iter()
        .enumerate()
        .map(|(n, (a, b))| {
            let begin = if n == 0 { 0.0 } else { words[a].start };
            // Each part stays until the next one starts; the last until the line ends.
            let end = if n + 1 == count {
                seconds
            } else {
                words[b].start
            };
            (shown[a..b].join(" "), begin, end)
        })
        .collect()
}

fn show_caption(text: &str, words: &[TimedWord], seconds: f64) {
    use oxframe_2d::{Background, Edge, Group, NodeBuilder, Rect, Txt, View};
    let parts = caption_parts(text, words, seconds);
    if parts.is_empty() {
        return;
    }
    let group = View::current().add(Group::new().name("caption").to_edge(Edge::Bottom));
    let back = group.add(Rect::new().radius(16.0).fill(Background).opacity(0.85));
    let label = group.add(Txt::new().text("").h2().max_width(1500.0));
    // Runs on its own; the scene does not wait for it.
    drop(oxframe_core::spawn(async move {
        let mut at = 0.0;
        for (part, begin, end) in parts {
            wait_for((begin - at).max(0.0)).await;
            label.text().set(part);
            let (w, h) = label.size();
            back.width().set(w + 48.0);
            back.height().set(h + 24.0);
            wait_for((end - begin).max(0.0)).await;
            at = end;
        }
        group.remove();
    }));
}

struct Spoken {
    seconds: f64,
    words: Vec<TimedWord>,
    file: Option<String>,
}

fn resolve(text: &str, voice: &str) -> Spoken {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = voice;
        estimate(text)
    }
    #[cfg(not(target_arch = "wasm32"))]
    CTX.with(|slot| {
        let ctx = slot.borrow();
        let Some(ctx) = ctx.as_ref() else {
            return estimate(text);
        };
        if let Some(provider) = &ctx.provider {
            let key = cache_key(provider.0.id(), voice, text);
            if let Some(hit) = read_cache(&ctx.dir, &key) {
                return hit;
            }
            // Parallel render jobs run the same scene at once: one of them
            // calls the provider, the others wait for its cache entry.
            let _lock = SynthLock::acquire(&ctx.dir, &key);
            if let Some(hit) = read_cache(&ctx.dir, &key) {
                return hit;
            }
            match provider.0.synthesize(&TtsRequest { text, voice }) {
                Ok(audio) => store(&ctx.dir, &key, provider.0.id(), voice, text, audio),
                Err(e) => {
                    eprintln!("warning: tts: {e}");
                    estimate(text)
                }
            }
        } else if let Some(hit) = find_committed(&ctx.dir, voice, text) {
            hit
        } else {
            estimate(text)
        }
    })
}

/// The words of a recording, as `oxframe transcribe` writes them.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Deserialize)]
struct WordsFile {
    seconds: f64,
    words: Vec<TimedWord>,
}

/// A line spoken by a recorded file.
fn recorded(text: &str, path: &str) -> Spoken {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        estimate(text)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let words_path = Path::new(path).with_extension("words.json");
        if let Ok(raw) = std::fs::read_to_string(&words_path) {
            match serde_json::from_str::<WordsFile>(&raw) {
                Ok(file) => {
                    return Spoken {
                        seconds: file.seconds.max(0.0),
                        words: file.words,
                        file: Some(path.to_owned()),
                    };
                }
                Err(e) => eprintln!("warning: {}: {e}", words_path.display()),
            }
        }
        let seconds = match oxframe_export::probe_duration(Path::new(path)) {
            Ok(seconds) => seconds.max(0.0),
            Err(e) => panic!("oxframe: voice(…).recording(\"{path}\") — {e}."),
        };
        let mut spoken = spread(text, seconds);
        spoken.file = Some(path.to_owned());
        spoken
    }
}

/// Spread the words of `text` across [`WORDS_PER_SECOND`].
fn estimate(text: &str) -> Spoken {
    let n = text
        .split_whitespace()
        .filter(|w| !bare(w).is_empty())
        .count();
    if n == 0 {
        return spread(text, 0.0);
    }
    spread(text, (n as f64 / WORDS_PER_SECOND).max(MIN_LINE))
}

/// Spread the words of `text` over `seconds`, each word by its length.
fn spread(text: &str, seconds: f64) -> Spoken {
    let words: Vec<String> = text
        .split_whitespace()
        .map(bare)
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        return Spoken {
            seconds,
            words: Vec::new(),
            file: None,
        };
    }
    let weight: usize = words.iter().map(|w| w.chars().count().max(1)).sum();
    let mut t = 0.0;
    let words = words
        .into_iter()
        .map(|word| {
            let share = word.chars().count().max(1) as f64 / weight as f64 * seconds;
            let start = t;
            t += share;
            TimedWord {
                text: word,
                start,
                end: t,
            }
        })
        .collect();
    Spoken {
        seconds,
        words,
        file: None,
    }
}

#[cfg(test)]
mod caption_tests {
    use super::*;

    #[test]
    fn parts_keep_punctuation_and_follow_the_words() {
        let text = "Rust compiles to fast code. Scenes are async functions that wait for words.";
        let spoken = estimate(text);
        let parts = caption_parts(text, &spoken.words, spoken.seconds);
        let texts: Vec<&str> = parts.iter().map(|p| p.0.as_str()).collect();
        assert_eq!(texts[0], "Rust compiles to fast code.");
        assert_eq!(texts[1], "Scenes are async functions that wait for words.");
        assert_eq!(parts[0].1, 0.0);
        assert_eq!(parts[1].1, parts[0].2);
        assert!((parts[1].2 - spoken.seconds).abs() < 1e-9);
    }

    #[test]
    fn long_lines_split_every_seven_words() {
        let text = "one two three four five six seven eight nine ten eleven twelve";
        let spoken = estimate(text);
        let parts = caption_parts(text, &spoken.words, spoken.seconds);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].0, "one two three four five six seven");
        assert_eq!(parts[1].0, "eight nine ten eleven twelve");
    }
}

fn bare(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric()).to_owned()
}

#[cfg(not(target_arch = "wasm32"))]
fn cache_key(provider: &str, voice: &str, text: &str) -> String {
    let mut h: u128 = 0xcbf29ce484222325;
    const PRIME: u128 = 0x100000001b3;
    for part in [provider, voice, text] {
        for byte in part.as_bytes() {
            h ^= u128::from(*byte);
            h = h.wrapping_mul(PRIME);
        }
        h ^= 0xff;
        h = h.wrapping_mul(PRIME);
    }
    format!("{h:032x}")
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Serialize, Deserialize)]
struct CacheFile {
    text: String,
    voice: String,
    provider: String,
    seconds: f64,
    words: Vec<TimedWord>,
}

/// `<key>.lock` in the cache directory while a process synthesizes `key`;
/// removed on drop.
#[cfg(not(target_arch = "wasm32"))]
struct SynthLock(std::path::PathBuf);

#[cfg(not(target_arch = "wasm32"))]
impl SynthLock {
    /// A lock older than this is left over from a process that died.
    const STALE: std::time::Duration = std::time::Duration::from_secs(120);

    /// Takes the lock, or waits until the process holding it has written
    /// the entry (then returns `None`; so does giving up after `STALE`).
    fn acquire(dir: &Path, key: &str) -> Option<Self> {
        let _ = std::fs::create_dir_all(dir);
        let path = dir.join(format!("{key}.lock"));
        let json = dir.join(format!("{key}.json"));
        let since = std::time::Instant::now();
        loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Some(Self(path)),
                Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => return None,
                Err(_) => {}
            }
            if json.is_file() || since.elapsed() > Self::STALE {
                return None;
            }
            let stale = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > Self::STALE);
            if stale {
                let _ = std::fs::remove_file(&path);
                continue;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for SynthLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read_cache(dir: &Path, key: &str) -> Option<Spoken> {
    let json = std::fs::read_to_string(dir.join(format!("{key}.json"))).ok()?;
    let file: CacheFile = serde_json::from_str(&json).ok()?;
    let wav = dir.join(format!("{key}.wav"));
    Some(Spoken {
        seconds: file.seconds,
        words: file.words,
        file: wav.is_file().then(|| wav.display().to_string()),
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn find_committed(dir: &Path, voice: &str, text: &str) -> Option<Spoken> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(file) = serde_json::from_str::<CacheFile>(&raw) else {
            continue;
        };
        if file.text == text && file.voice == voice {
            let wav = path.with_extension("wav");
            return Some(Spoken {
                seconds: file.seconds,
                words: file.words,
                file: wav.is_file().then(|| wav.display().to_string()),
            });
        }
    }
    None
}

#[cfg(not(target_arch = "wasm32"))]
fn store(
    dir: &Path,
    key: &str,
    provider: &str,
    voice: &str,
    text: &str,
    audio: Synthesis,
) -> Spoken {
    let seconds = audio
        .words
        .last()
        .map(|w| w.end)
        .unwrap_or_else(|| audio.samples.len() as f64 / f64::from(audio.rate.max(1)));
    let _ = std::fs::create_dir_all(dir);
    let json = CacheFile {
        text: text.to_owned(),
        voice: voice.to_owned(),
        provider: provider.to_owned(),
        seconds,
        words: audio.words.clone(),
    };
    let wav = dir.join(format!("{key}.wav"));
    let file = if audio.samples.is_empty() {
        None
    } else if oxframe_export::write_wav(
        &wav,
        &oxframe_export::Mixed {
            rate: audio.rate,
            samples: audio.samples,
        },
    )
    .is_ok()
    {
        Some(wav.display().to_string())
    } else {
        None
    };
    // The JSON marks the entry as complete, so it goes last, and whole:
    // another process may be waiting for it (see `SynthLock`).
    if let Ok(body) = serde_json::to_string_pretty(&json) {
        let path = dir.join(format!("{key}.json"));
        let tmp = dir.join(format!("{key}.json.{}.tmp", std::process::id()));
        if std::fs::write(&tmp, body).is_ok() && std::fs::rename(&tmp, &path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }
    Spoken {
        seconds,
        words: audio.words,
        file,
    }
}

/// One cue for a subtitle file, in project time.
#[derive(Debug, Clone)]
pub(crate) struct Cue {
    /// The line.
    pub text: String,
    /// Start, in seconds.
    pub start: f64,
    /// End, in seconds.
    pub end: f64,
    /// When the scene that spoke this line ends.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub scene_end: f64,
}

/// `srt` or `vtt` text for `cues`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn subtitles(cues: &[Cue], vtt: bool) -> String {
    let mut out = String::new();
    if vtt {
        out.push_str("WEBVTT\n\n");
    }
    for (i, cue) in cues.iter().enumerate() {
        if !vtt {
            out.push_str(&format!("{}\n", i + 1));
        }
        let sep = if vtt { '.' } else { ',' };
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            stamp(cue.start, sep),
            stamp(cue.end, sep),
            cue.text
        ));
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn stamp(seconds: f64, sep: char) -> String {
    let ms = (seconds.max(0.0) * 1000.0).round() as u64;
    let h = ms / 3_600_000;
    let m = (ms / 60_000) % 60;
    let s = (ms / 1000) % 60;
    let milli = ms % 1000;
    format!("{h:02}:{m:02}:{s:02}{sep}{milli:03}")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use oxframe_2d::View;
    use oxframe_core::BoxFuture;
    use oxframe_protocol::Meta;

    use super::*;
    use crate::engine::Engine;
    use crate::lint;
    use crate::project::Project;
    use crate::scene::{SceneDef, SourceLocation};

    fn scene(name: &'static str, factory: fn(View) -> BoxFuture) -> SceneDef {
        SceneDef::new(
            name,
            SourceLocation {
                file: "src/voice.rs",
                manifest_dir: env!("CARGO_MANIFEST_DIR"),
            },
            factory,
        )
    }

    fn project(factory: fn(View) -> BoxFuture) -> Project {
        let mut p = Project::new("voice")
            .size(32, 18)
            .fps(60)
            .write_meta(false)
            .inline_meta(BTreeMap::<String, Meta>::new());
        p = p.scene(scene("main", factory));
        p
    }

    #[test]
    fn an_estimate_gives_each_word_a_share() {
        let spoken = estimate("one two");
        assert!((spoken.seconds - 0.8).abs() < 1e-9);
        assert_eq!(spoken.words.len(), 2);
        assert!((spoken.words[1].start - 0.4).abs() < 1e-9);
        assert!(spoken.file.is_none());
    }

    #[test]
    fn a_recording_is_timed_by_its_words_file() {
        let dir = std::env::temp_dir().join(format!("oxframe-rec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("intro.wav");
        std::fs::write(
            dir.join("intro.words.json"),
            r#"{"text":"Hello there.","seconds":1.75,"words":[{"text":"Hello","start":0.1,"end":0.5},{"text":"there","start":0.6,"end":1.2}]}"#,
        )
        .unwrap();
        let spoken = recorded("Hello there.", wav.to_str().unwrap());
        assert!((spoken.seconds - 1.75).abs() < 1e-9);
        assert_eq!(spoken.words[1].text, "there");
        assert!((spoken.words[1].start - 0.6).abs() < 1e-9);
        assert_eq!(spoken.file.as_deref(), wav.to_str());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn spread_shares_the_given_length() {
        let spoken = spread("ab abcd", 3.0);
        assert!((spoken.seconds - 3.0).abs() < 1e-9);
        assert!((spoken.words[1].start - 1.0).abs() < 1e-9);
        assert!(estimate("").seconds.abs() < 1e-9);
    }

    #[test]
    fn punctuation_is_not_part_of_the_word() {
        let spoken = estimate("квадрат, потом");
        assert_eq!(spoken.words[0].text, "квадрат");
    }

    #[test]
    fn subtitles_use_a_comma_for_srt_and_a_dot_for_vtt() {
        let cues = [Cue {
            text: "hello".into(),
            start: 1.5,
            end: 2.25,
            scene_end: 3.0,
        }];
        let srt = subtitles(&cues, false);
        assert!(srt.contains("1\n00:00:01,500 --> 00:00:02,250\nhello"));
        let vtt = subtitles(&cues, true);
        assert!(vtt.starts_with("WEBVTT\n"));
        assert!(vtt.contains("00:00:01.500 --> 00:00:02.250"));
    }

    struct Count(Arc<AtomicUsize>);

    impl TtsProvider for Count {
        fn id(&self) -> &str {
            "count"
        }
        fn synthesize(&self, req: &TtsRequest<'_>) -> Result<Synthesis, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            let spoken = estimate(req.text);
            let n = (spoken.seconds * 48_000.0).round() as usize;
            Ok(Synthesis {
                rate: 48_000,
                samples: vec![0; n],
                words: spoken.words,
            })
        }
    }

    #[test]
    fn a_cached_line_is_not_spoken_again() {
        let dir = std::env::temp_dir().join(format!("oxframe-tts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let calls = Arc::new(AtomicUsize::new(0));
        CTX.with(|slot| {
            *slot.borrow_mut() = Some(Ctx {
                dir: dir.clone(),
                voice: "default".into(),
                provider: Some(Provider(Arc::new(Count(calls.clone())))),
            });
        });
        let _ = resolve("hello there", "default");
        let _ = resolve("hello there", "default");
        let _ = resolve("hello again", "default");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Like `Count`, but slow, as a real service is.
    struct Slow(Arc<AtomicUsize>);

    impl TtsProvider for Slow {
        fn id(&self) -> &str {
            "slow"
        }
        fn synthesize(&self, req: &TtsRequest<'_>) -> Result<Synthesis, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(300));
            Count(Arc::new(AtomicUsize::new(0))).synthesize(req)
        }
    }

    #[test]
    fn parallel_jobs_speak_a_line_once() {
        // `render --jobs` runs the scene in several processes at once; each
        // used to call the service and write the same cache file, which
        // spliced two takes into one WAV.
        let dir = std::env::temp_dir().join(format!("oxframe-tts-par-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let calls = Arc::new(AtomicUsize::new(0));
        let jobs: Vec<_> = (0..4)
            .map(|_| {
                let (dir, calls) = (dir.clone(), calls.clone());
                std::thread::spawn(move || {
                    CTX.with(|slot| {
                        *slot.borrow_mut() = Some(Ctx {
                            dir,
                            voice: "default".into(),
                            provider: Some(Provider(Arc::new(Slow(calls)))),
                        });
                    });
                    resolve("hello there", "default")
                })
            })
            .collect();
        for job in jobs {
            let spoken = job.join().unwrap();
            assert!(spoken.file.is_some(), "every job plays the cached file");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names.len(), 2, "only the entry is left: {names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn at_word_lands_on_that_word() {
        fn body(_view: View) -> BoxFuture {
            Box::pin(async move {
                let line = voice("one two").start();
                line.at_word("two").await;
            })
        }
        let engine = Engine::new(project(body)).unwrap();
        let seconds = engine.total_frames() as f64 / 60.0;
        assert!(
            (seconds - 0.4).abs() < 1.0 / 60.0,
            "two starts at 0.4, scene ended at {seconds}"
        );
        assert_eq!(engine.voices()[0].text, "one two");
    }

    #[test]
    fn a_line_past_the_scene_is_cut_and_two_lines_overlap() {
        fn cut(_view: View) -> BoxFuture {
            Box::pin(async move {
                let _line = voice("one two").start();
            })
        }
        let engine = Engine::new(project(cut)).unwrap();
        assert!(
            lint::voice_lines(&engine.voices())
                .iter()
                .any(|issue| issue.rule == "voice_cut")
        );

        fn both(_view: View) -> BoxFuture {
            Box::pin(async move {
                let _a = voice("one two").start();
                let _b = voice("three four").start();
                oxframe_core::wait_for(0.8).await;
            })
        }
        let engine = Engine::new(project(both)).unwrap();
        let issues = lint::voice_lines(&engine.voices());
        assert!(issues.iter().any(|issue| issue.rule == "voice_overlap"));
        assert!(issues.iter().all(|issue| issue.rule != "voice_cut"));
    }

    #[test]
    fn a_provider_sets_the_word_and_the_cache_is_reused() {
        let dir = std::env::temp_dir().join(format!("oxframe-tts-time-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let calls = Arc::new(AtomicUsize::new(0));

        struct Early(Arc<AtomicUsize>);
        impl TtsProvider for Early {
            fn id(&self) -> &str {
                "early"
            }
            fn synthesize(&self, req: &TtsRequest) -> Result<Synthesis, String> {
                self.0.fetch_add(1, Ordering::SeqCst);
                let words: Vec<TimedWord> = req
                    .text
                    .split_whitespace()
                    .enumerate()
                    .map(|(i, word)| TimedWord {
                        text: word.to_owned(),
                        start: i as f64 * 0.1,
                        end: (i as f64 + 1.0) * 0.1,
                    })
                    .collect();
                Ok(Synthesis {
                    rate: 48_000,
                    samples: vec![0; 9_600],
                    words,
                })
            }
        }

        fn body(_view: View) -> BoxFuture {
            Box::pin(async move {
                voice("one two").start().at_word("two").await;
            })
        }

        let engine = Engine::new(project(body).tts(Early(calls.clone())).tts_dir(&dir)).unwrap();
        let seconds = engine.total_frames() as f64 / 60.0;
        assert!(
            (seconds - 0.1).abs() < 1.0 / 60.0,
            "the provider puts two at 0.1, scene ended at {seconds}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let _again = Engine::new(project(body).tts(Early(calls.clone())).tts_dir(&dir)).unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "the second run reads the cache"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
