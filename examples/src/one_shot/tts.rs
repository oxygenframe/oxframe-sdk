//! ElevenLabs text-to-speech with word timings, for `Project::tts`.
//! The key comes from `ELEVENLABS_API_KEY` (loaded from `.env` in `main`).

use base64::Engine;
use oxframe::{Synthesis, TimedWord, TtsProvider, TtsRequest};

const RATE: u32 = 24_000;

/// The voice id comes from `Project::speaker`, so it is part of the cache key.
pub struct ElevenLabs;

impl TtsProvider for ElevenLabs {
    fn id(&self) -> &str {
        "elevenlabs"
    }

    fn synthesize(&self, req: &TtsRequest<'_>) -> Result<Synthesis, String> {
        let key = std::env::var("ELEVENLABS_API_KEY")
            .map_err(|_| "set ELEVENLABS_API_KEY in .env".to_string())?;
        let url = format!(
            "https://api.elevenlabs.io/v1/text-to-speech/{}/with-timestamps?output_format=pcm_{RATE}",
            req.voice
        );
        let body: serde_json::Value = ureq::post(&url)
            .set("xi-api-key", &key)
            .send_json(serde_json::json!({
                "text": req.text,
                "model_id": "eleven_multilingual_v2",
                "voice_settings": { "stability": 0.5, "similarity_boost": 0.75 },
            }))
            .map_err(|e| format!("elevenlabs: {e}"))?
            .into_json()
            .map_err(|e| format!("elevenlabs: {e}"))?;

        let audio = body["audio_base64"].as_str().ok_or("elevenlabs: no audio")?;
        let pcm = base64::engine::general_purpose::STANDARD
            .decode(audio)
            .map_err(|e| format!("elevenlabs: {e}"))?;
        let samples = pcm
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();

        let a = &body["alignment"];
        let chars = a["characters"].as_array().ok_or("elevenlabs: no alignment")?;
        let starts = a["character_start_times_seconds"].as_array().ok_or("elevenlabs: no starts")?;
        let ends = a["character_end_times_seconds"].as_array().ok_or("elevenlabs: no ends")?;
        Ok(Synthesis { rate: RATE, samples, words: words(chars, starts, ends) })
    }
}

/// Groups character timings into words split on whitespace.
fn words(
    chars: &[serde_json::Value],
    starts: &[serde_json::Value],
    ends: &[serde_json::Value],
) -> Vec<TimedWord> {
    let mut out = Vec::new();
    let mut current: Option<TimedWord> = None;
    for ((c, s), e) in chars.iter().zip(starts).zip(ends) {
        let c = c.as_str().unwrap_or(" ");
        let (s, e) = (s.as_f64().unwrap_or(0.0), e.as_f64().unwrap_or(0.0));
        if c.trim().is_empty() {
            out.extend(current.take());
            continue;
        }
        match current.as_mut() {
            Some(w) => {
                w.text.push_str(c);
                w.end = e;
            }
            None => current = Some(TimedWord { text: c.to_string(), start: s, end: e }),
        }
    }
    out.extend(current);
    for w in &mut out {
        w.text = w.text.trim_matches(|c: char| !c.is_alphanumeric()).to_string();
    }
    out.retain(|w| !w.text.is_empty());
    out
}
