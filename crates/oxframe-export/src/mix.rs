//! Mix a soundtrack and sound effects into one PCM buffer.
//!
//! Decoding uses symphonia, so the samples do not depend on which ffmpeg
//! built the file. ffmpeg only encodes this buffer into the video.

use std::fmt;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Samples per second of every mix. One rate keeps renders comparable.
pub const SAMPLE_RATE: u32 = 48_000;

/// Preview playback is coarser than the render mix.
pub const PREVIEW_RATE: u32 = 16_000;

const DUCK_FLOOR: f32 = 0.25;
const DUCK_ATTACK: f64 = 0.02;
const DUCK_RELEASE: f64 = 0.30;
const CEILING: f32 = 0.89;
const LOOKAHEAD: f64 = 0.005;
const LIMIT_RELEASE: f64 = 0.05;
/// Short fade on the ends of the whole mix, so a cut does not click.
const EDGE: f64 = 0.002;

/// One clip on the output timeline.
#[derive(Debug, Clone)]
pub struct MixClip {
    /// Audio file.
    pub file: std::path::PathBuf,
    /// Start on the output timeline, in seconds. Negative skips into the clip.
    pub start: f64,
    /// Linear gain.
    pub volume: f32,
    /// Fade in, seconds.
    pub fade_in: f64,
    /// Fade out, seconds.
    pub fade_out: f64,
    /// Seconds skipped from the start of the file.
    pub trim_start: f64,
    /// End point in the file. `None` plays to the end.
    pub trim_end: Option<f64>,
    /// While this clip plays, music ducks.
    pub priority: bool,
    /// This clip is the soundtrack, so ducking applies to it.
    pub music: bool,
}

/// What to mix.
#[derive(Debug, Clone)]
pub struct MixRequest {
    /// Length of the output, in seconds.
    pub seconds: f64,
    /// Duck music under priority clips.
    pub duck: bool,
    /// Fade at the start of the whole mix.
    pub master_fade_in: f64,
    /// Fade at the end of the whole mix.
    pub master_fade_out: f64,
    /// Clips, in any order.
    pub clips: Vec<MixClip>,
}

/// Mono PCM at [`SAMPLE_RATE`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mixed {
    /// Samples per second.
    pub rate: u32,
    /// One sample per frame, already limited.
    pub samples: Vec<i16>,
}

/// Why a mix failed.
#[derive(Debug)]
pub enum MixError {
    /// The file could not be read or decoded.
    Read {
        /// Path that failed.
        file: String,
        /// What went wrong.
        message: String,
    },
}

impl fmt::Display for MixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { file, message } => {
                write!(f, "cannot read {file}: {message}")
            }
        }
    }
}

impl std::error::Error for MixError {}

/// Length of an audio file in seconds.
///
/// # Errors
///
/// If the file cannot be decoded.
pub fn probe_duration(path: &Path) -> Result<f64, MixError> {
    let (rate, samples) = decode(path)?;
    Ok(samples.len() as f64 / f64::from(rate.max(1)))
}

/// Mix `req` into mono PCM.
///
/// # Errors
///
/// If a clip's file cannot be decoded.
pub fn mix(req: &MixRequest) -> Result<Mixed, MixError> {
    mix_with_loudness(req, None)
}

/// [`mix`], then bring the whole mix to `loudness` (integrated LUFS, EBU
/// R128) before the limiter. `None` leaves the levels as they are.
///
/// A function, not a field of [`MixRequest`]: a new field would break
/// crates that build the request with a struct literal.
///
/// # Errors
///
/// If a clip's file cannot be decoded.
pub fn mix_with_loudness(req: &MixRequest, loudness: Option<f64>) -> Result<Mixed, MixError> {
    let rate = SAMPLE_RATE;
    let n = (req.seconds.max(0.0) * f64::from(rate)).round() as usize;
    let mut loaded: Vec<Loaded> = Vec::with_capacity(req.clips.len());
    for clip in &req.clips {
        let pcm = load_clip(clip)?;
        loaded.push(Loaded {
            samples: pcm,
            start: clip.start,
            volume: clip.volume.max(0.0),
            fade_in: clip.fade_in.max(0.0),
            fade_out: clip.fade_out.max(0.0),
            priority: clip.priority,
            music: clip.music,
        });
    }
    let voices: Vec<(f64, f64)> = loaded
        .iter()
        .filter(|c| c.priority)
        .map(|c| {
            let dur = c.samples.len() as f64 / f64::from(rate);
            (c.start, c.start + dur)
        })
        .collect();
    let duck = duck_envelope(n, rate, req.duck, &voices);
    let mut acc = vec![0.0f32; n];
    for clip in &loaded {
        add_clip(&mut acc, clip, rate, if clip.music { &duck } else { &[] });
    }
    apply_master_fade(
        &mut acc,
        rate,
        req.master_fade_in.max(EDGE),
        req.master_fade_out.max(EDGE),
    );
    if let Some(target) = loudness
        && let Some(measured) = integrated_loudness(&acc, rate)
    {
        let gain = 10f64.powf((target - measured) / 20.0) as f32;
        for s in &mut acc {
            *s *= gain;
        }
    }
    let limited = limit(&acc, rate);
    let samples = limited
        .into_iter()
        .map(|s| (s.clamp(-1.0, 1.0) * 32767.0).round() as i16)
        .collect();
    Ok(Mixed { rate, samples })
}

/// Integrated loudness of `mixed` in LUFS (EBU R128 / ITU-R BS.1770), as
/// players and platforms measure the video: the stereo file, both channels
/// the same. `None` for silence.
pub fn loudness(mixed: &Mixed) -> Option<f64> {
    let samples: Vec<f32> = mixed
        .samples
        .iter()
        .map(|s| f32::from(*s) / 32768.0)
        .collect();
    integrated_loudness(&samples, mixed.rate)
}

/// K-weighting (BS.1770 at 48 kHz), 400 ms blocks every 100 ms, an absolute
/// gate at -70 LUFS and a relative one 10 LU below the mean. The mono mix is
/// written as two equal channels, so their powers add (+3 dB).
fn integrated_loudness(samples: &[f32], rate: u32) -> Option<f64> {
    debug_assert_eq!(rate, SAMPLE_RATE, "K-weighting coefficients are for 48 kHz");
    // (b0, b1, b2, a1, a2): the high shelf, then the high pass.
    const STAGES: [[f64; 5]; 2] = [
        [
            1.535_124_859_586_97,
            -2.691_696_189_406_38,
            1.198_392_810_852_85,
            -1.690_659_293_182_41,
            0.732_480_774_215_85,
        ],
        [1.0, -2.0, 1.0, -1.990_047_454_833_98, 0.990_072_250_366_21],
    ];
    let mut weighted: Vec<f64> = samples.iter().map(|s| f64::from(*s)).collect();
    for [b0, b1, b2, a1, a2] in STAGES {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        for v in &mut weighted {
            let x = *v;
            let y = b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
            (x2, x1, y2, y1) = (x1, x, y1, y);
            *v = y;
        }
    }
    let block = (0.4 * f64::from(rate)) as usize;
    let step = (0.1 * f64::from(rate)) as usize;
    if weighted.is_empty() {
        return None;
    }
    let power = |w: &[f64]| 2.0 * w.iter().map(|v| v * v).sum::<f64>() / w.len() as f64;
    let blocks: Vec<f64> = if weighted.len() < block {
        vec![power(&weighted)]
    } else {
        (0..=(weighted.len() - block) / step)
            .map(|i| power(&weighted[i * step..i * step + block]))
            .collect()
    };
    let lufs = |p: f64| -0.691 + 10.0 * p.log10();
    let loud: Vec<f64> = blocks.into_iter().filter(|p| lufs(*p) > -70.0).collect();
    if loud.is_empty() {
        return None;
    }
    let relative = lufs(loud.iter().sum::<f64>() / loud.len() as f64) - 10.0;
    let kept: Vec<f64> = loud.into_iter().filter(|p| lufs(*p) > relative).collect();
    if kept.is_empty() {
        return None;
    }
    Some(lufs(kept.iter().sum::<f64>() / kept.len() as f64))
}

/// Write mono PCM as a stereo 16-bit WAV (both channels identical).
///
/// # Errors
///
/// If the file cannot be created.
pub fn write_wav(path: &Path, mixed: &Mixed) -> Result<(), MixError> {
    let channels: u16 = 2;
    let bits: u16 = 16;
    let block = channels * bits / 8;
    let data_len = (mixed.samples.len() * usize::from(block)) as u32;
    let mut out = File::create(path).map_err(|e| MixError::Read {
        file: path.display().to_string(),
        message: e.to_string(),
    })?;
    let write = |f: &mut File, bytes: &[u8]| {
        f.write_all(bytes).map_err(|e| MixError::Read {
            file: path.display().to_string(),
            message: e.to_string(),
        })
    };
    write(&mut out, b"RIFF")?;
    write(&mut out, &(36 + data_len).to_le_bytes())?;
    write(&mut out, b"WAVEfmt ")?;
    write(&mut out, &16u32.to_le_bytes())?;
    write(&mut out, &1u16.to_le_bytes())?;
    write(&mut out, &channels.to_le_bytes())?;
    write(&mut out, &mixed.rate.to_le_bytes())?;
    write(&mut out, &(mixed.rate * u32::from(block)).to_le_bytes())?;
    write(&mut out, &block.to_le_bytes())?;
    write(&mut out, &bits.to_le_bytes())?;
    write(&mut out, b"data")?;
    write(&mut out, &data_len.to_le_bytes())?;
    for s in &mixed.samples {
        let b = s.to_le_bytes();
        write(&mut out, &b)?;
        write(&mut out, &b)?;
    }
    Ok(())
}

/// Peak envelope in `0..=1`, one value per bucket.
pub fn envelope(samples: &[i16], buckets: usize) -> Vec<f32> {
    if buckets == 0 {
        return Vec::new();
    }
    if samples.is_empty() {
        return vec![0.0; buckets];
    }
    (0..buckets)
        .map(|i| {
            let a = i * samples.len() / buckets;
            let b = ((i + 1) * samples.len() / buckets)
                .max(a + 1)
                .min(samples.len());
            let peak = samples[a..b]
                .iter()
                .map(|s| s.unsigned_abs())
                .max()
                .unwrap_or(0);
            f32::from(peak) / 32768.0
        })
        .collect()
}

/// Average `from` Hz samples down to `to` Hz.
pub fn downsample(samples: &[i16], from: u32, to: u32) -> Vec<i16> {
    if samples.is_empty() || to == 0 || from == 0 || from == to {
        return samples.to_vec();
    }
    let ratio = f64::from(from) / f64::from(to);
    let n = (samples.len() as f64 / ratio).floor() as usize;
    (0..n)
        .map(|i| {
            let a = (i as f64 * ratio).floor() as usize;
            let b = (((i + 1) as f64) * ratio).floor() as usize;
            let b = b.max(a + 1).min(samples.len());
            let sum: i32 = samples[a..b].iter().map(|s| i32::from(*s)).sum();
            (sum / (b - a) as i32) as i16
        })
        .collect()
}

struct Loaded {
    samples: Vec<f32>,
    start: f64,
    volume: f32,
    fade_in: f64,
    fade_out: f64,
    priority: bool,
    music: bool,
}

fn load_clip(clip: &MixClip) -> Result<Vec<f32>, MixError> {
    let (rate, pcm) = decode(&clip.file)?;
    let at_rate = resample(&pcm, rate, SAMPLE_RATE);
    let start = (clip.trim_start.max(0.0) * f64::from(SAMPLE_RATE)).round() as usize;
    let end = clip
        .trim_end
        .map(|t| (t.max(0.0) * f64::from(SAMPLE_RATE)).round() as usize)
        .unwrap_or(at_rate.len())
        .min(at_rate.len());
    if end <= start || start >= at_rate.len() {
        return Ok(Vec::new());
    }
    Ok(at_rate[start..end].to_vec())
}

fn decode(path: &Path) -> Result<(u32, Vec<f32>), MixError> {
    let file = File::open(path).map_err(|e| MixError::Read {
        file: path.display().to_string(),
        message: e.to_string(),
    })?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| MixError::Read {
            file: path.display().to_string(),
            message: e.to_string(),
        })?;
    let mut format = probed.format;
    // A video file's first track is the picture; take the first one that
    // has a sample rate, which only audio tracks have.
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.sample_rate.is_some())
        .ok_or_else(|| MixError::Read {
            file: path.display().to_string(),
            message: "no audio track".into(),
        })?
        .clone();
    let track_id = track.id;
    let rate = track.codec_params.sample_rate.unwrap_or(SAMPLE_RATE);
    let dec_opts = DecoderOptions::default();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &dec_opts)
        .map_err(|e| MixError::Read {
            file: path.display().to_string(),
            message: e.to_string(),
        })?;
    let mut pcm = Vec::new();
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(_)) => continue,
            Err(_) => break,
        };
        let spec = *decoded.spec();
        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buf.copy_interleaved_ref(decoded);
        let channels = spec.channels.count().max(1);
        for frame in buf.samples().chunks(channels) {
            pcm.push(frame.first().copied().unwrap_or(0.0));
        }
    }
    Ok((rate, pcm))
}

fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if input.is_empty() || from == 0 {
        return Vec::new();
    }
    if from == to {
        return input.to_vec();
    }
    let out_len = (input.len() as u64) * u64::from(to) / u64::from(from);
    let mut out = Vec::with_capacity(out_len as usize);
    for i in 0..out_len {
        let pos = i as f64 * f64::from(from) / f64::from(to);
        let j = pos.floor() as usize;
        let frac = (pos - j as f64) as f32;
        let a = input.get(j).copied().unwrap_or(0.0);
        let b = input.get(j + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}

fn duck_envelope(n: usize, rate: u32, enabled: bool, voices: &[(f64, f64)]) -> Vec<f32> {
    let mut env = vec![1.0f32; n];
    if !enabled || voices.is_empty() || n == 0 {
        return env;
    }
    let attack = (DUCK_ATTACK * f64::from(rate)).max(1.0) as f32;
    let release = (DUCK_RELEASE * f64::from(rate)).max(1.0) as f32;
    let mut gain = 1.0f32;
    for (i, slot) in env.iter_mut().enumerate() {
        let t = i as f64 / f64::from(rate);
        let under = voices.iter().any(|(a, b)| t >= *a && t < *b);
        let target = if under { DUCK_FLOOR } else { 1.0 };
        let step = if target < gain { attack } else { release };
        gain += (target - gain) / step;
        *slot = gain;
    }
    env
}

fn add_clip(acc: &mut [f32], clip: &Loaded, rate: u32, duck: &[f32]) {
    let dur = clip.samples.len() as f64 / f64::from(rate);
    let origin = (clip.start * f64::from(rate)).round() as isize;
    for (k, sample) in clip.samples.iter().enumerate() {
        let i = origin + k as isize;
        if i < 0 || i >= acc.len() as isize {
            continue;
        }
        let pos = k as f64 / f64::from(rate);
        let mut g = clip.volume * clip_fade(pos, dur, clip.fade_in, clip.fade_out);
        if !duck.is_empty() {
            g *= duck[i as usize];
        }
        acc[i as usize] += sample * g;
    }
}

fn clip_fade(pos: f64, dur: f64, fade_in: f64, fade_out: f64) -> f32 {
    let mut g = 1.0f32;
    if fade_in > 0.0 && pos < fade_in {
        g *= (pos / fade_in) as f32;
    }
    if fade_out > 0.0 && pos > dur - fade_out {
        g *= ((dur - pos) / fade_out).max(0.0) as f32;
    }
    g
}

fn apply_master_fade(acc: &mut [f32], rate: u32, fade_in: f64, fade_out: f64) {
    let n = acc.len();
    let inn = (fade_in * f64::from(rate)).round().max(1.0) as usize;
    let outn = (fade_out * f64::from(rate)).round().max(1.0) as usize;
    for (i, s) in acc.iter_mut().enumerate() {
        let mut g = 1.0f32;
        if i < inn {
            g *= i as f32 / inn as f32;
        }
        if i + outn >= n {
            g *= (n.saturating_sub(1).saturating_sub(i)) as f32 / outn as f32;
        }
        *s *= g;
    }
}

fn limit(input: &[f32], rate: u32) -> Vec<f32> {
    let n = input.len();
    let lookahead = ((LOOKAHEAD * f64::from(rate)).round() as usize).max(1);
    let release = (LIMIT_RELEASE * f64::from(rate)).max(1.0) as f32;
    let mut out = vec![0.0f32; n];
    let mut gain = 1.0f32;
    for i in 0..n {
        let end = (i + lookahead).min(n);
        let peak = input[i..end].iter().fold(0.0f32, |p, s| p.max(s.abs()));
        let target = if peak > CEILING { CEILING / peak } else { 1.0 };
        if target < gain {
            gain = target;
        } else {
            gain += (target - gain) / release;
        }
        out[i] = (input[i] * gain).clamp(-CEILING, CEILING);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(path: &Path, hz: f32, seconds: f64, amp: f32) {
        let n = (seconds * f64::from(SAMPLE_RATE)).round() as usize;
        let samples: Vec<i16> = (0..n)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                (amp * (t * hz * std::f32::consts::TAU).sin() * 32767.0).round() as i16
            })
            .collect();
        write_wav(
            path,
            &Mixed {
                rate: SAMPLE_RATE,
                samples,
            },
        )
        .unwrap();
    }

    #[test]
    fn a_sine_at_minus_20_dbfs_is_minus_20_lufs() {
        // BS.1770: a 997 Hz sine in both channels reads its peak level.
        let n = 5 * SAMPLE_RATE as usize;
        let samples: Vec<i16> = (0..n)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                (0.1 * (t * 997.0 * std::f32::consts::TAU).sin() * 32767.0).round() as i16
            })
            .collect();
        let l = loudness(&Mixed {
            rate: SAMPLE_RATE,
            samples,
        })
        .unwrap();
        assert!((l + 20.0).abs() < 0.1, "{l} LUFS");
        let silent = Mixed {
            rate: SAMPLE_RATE,
            samples: vec![0; n],
        };
        assert_eq!(loudness(&silent), None);
    }

    #[test]
    fn a_quiet_mix_is_brought_to_the_target_loudness() {
        let dir = std::env::temp_dir().join(format!("oxframe-loud-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let quiet = dir.join("quiet.wav");
        tone(&quiet, 440.0, 4.0, 0.03);
        let req = MixRequest {
            seconds: 4.0,
            duck: false,
            master_fade_in: 0.0,
            master_fade_out: 0.0,
            clips: vec![clip(&quiet, 0.0, false, false)],
        };
        let before = loudness(&mix(&req).unwrap()).unwrap();
        let after = loudness(&mix_with_loudness(&req, Some(-14.0)).unwrap()).unwrap();
        assert!(before < -25.0, "{before}");
        assert!((after + 14.0).abs() < 0.2, "{after} LUFS");
        let _ = std::fs::remove_dir_all(dir);
    }

    fn clip(file: &Path, start: f64, priority: bool, music: bool) -> MixClip {
        MixClip {
            file: file.into(),
            start,
            volume: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
            trim_start: 0.0,
            trim_end: None,
            priority,
            music,
        }
    }

    #[test]
    fn a_mix_is_the_same_bytes_twice_and_never_clips() {
        let dir = std::env::temp_dir().join("oxframe-mix-det");
        let _ = std::fs::create_dir_all(&dir);
        let a = dir.join("a.wav");
        let b = dir.join("b.wav");
        tone(&a, 440.0, 0.4, 1.0);
        tone(&b, 660.0, 0.4, 1.0);
        let req = MixRequest {
            seconds: 0.4,
            duck: false,
            master_fade_in: 0.0,
            master_fade_out: 0.0,
            clips: vec![clip(&a, 0.0, false, false), clip(&b, 0.0, false, false)],
        };
        let once = mix(&req).unwrap();
        let twice = mix(&req).unwrap();
        assert_eq!(once.samples, twice.samples);
        let ceiling = (CEILING * 32767.0).round() as i16;
        assert!(once.samples.iter().all(|s| s.abs() <= ceiling));
        assert_eq!(once.samples.first().copied(), Some(0));
        assert_eq!(once.samples.last().copied(), Some(0));
    }

    #[test]
    fn priority_ducks_the_music_without_a_click() {
        let dir = std::env::temp_dir().join("oxframe-mix-duck");
        let _ = std::fs::create_dir_all(&dir);
        let music = dir.join("music.wav");
        let voice = dir.join("voice.wav");
        tone(&music, 220.0, 0.8, 0.5);
        tone(&voice, 880.0, 0.2, 0.0);
        let ducked = mix(&MixRequest {
            seconds: 0.8,
            duck: true,
            master_fade_in: 0.0,
            master_fade_out: 0.0,
            clips: vec![
                clip(&music, 0.0, false, true),
                clip(&voice, 0.3, true, false),
            ],
        })
        .unwrap();
        let plain = mix(&MixRequest {
            seconds: 0.8,
            duck: false,
            master_fade_in: 0.0,
            master_fade_out: 0.0,
            clips: vec![clip(&music, 0.0, false, true)],
        })
        .unwrap();
        let at = |samples: &[i16], t: f64| {
            let i = (t * f64::from(SAMPLE_RATE)) as usize;
            samples[i].unsigned_abs()
        };
        assert!(at(&ducked.samples, 0.4) < at(&plain.samples, 0.4));
        let window = |samples: &[i16], t: f64| {
            let i = (t * f64::from(SAMPLE_RATE)) as usize;
            samples[i..i + SAMPLE_RATE as usize / 20]
                .iter()
                .map(|s| u32::from(s.unsigned_abs()))
                .max()
                .unwrap_or(0)
        };
        assert!(window(&ducked.samples, 0.35) * 2 < window(&ducked.samples, 0.1));
    }

    #[test]
    fn a_range_shifts_the_clip() {
        let dir = std::env::temp_dir().join("oxframe-mix-range");
        let _ = std::fs::create_dir_all(&dir);
        let click = dir.join("click.wav");
        tone(&click, 1000.0, 0.05, 0.5);
        let mixed = mix(&MixRequest {
            seconds: 0.5,
            duck: false,
            master_fade_in: 0.0,
            master_fade_out: 0.0,
            clips: vec![clip(&click, 0.2, false, false)],
        })
        .unwrap();
        let onset = mixed
            .samples
            .iter()
            .position(|s| s.unsigned_abs() > 1000)
            .expect("the clip is audible");
        let at = onset as f64 / f64::from(SAMPLE_RATE);
        assert!((at - 0.2).abs() < 0.01, "onset at {at}");
    }
}
