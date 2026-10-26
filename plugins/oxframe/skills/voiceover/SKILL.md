---
name: voiceover
description: "Add narration to an oxframe video and sync the animation to spoken words: voice lines, waiting for a word, ducking music, subtitles, a text-to-speech provider. Use for narrated explainers, voiceovers or captions."
---
# Voiceover

Uses the `oxframe` skill (workflow, full API cheat sheet); install both with `npx skills add oxygenframe/oxframe-sdk`.

```rust
#[oxframe::scene]
async fn intro(view: View) {
    let square = view.add(Rect::new().size(200.0).fill(Accent).opacity(0.0));
    let line = voice("First we draw a square, then we turn it").start();
    line.at_word("square").await;
    square.fade_in().await;
    line.at_word("turn").await;
    square.rotation().to(90.0, 1.0).await;
    line.await;                       // the scene must not end before the line
}
```

- `voice(text).start()` speaks now; `.at_word("word")` waits for that word;
  awaiting the line waits for its end. `check` reports `voice_cut` and
  `voice_overlap`.
- oxframe does not call a speech service. Implement `TtsProvider` (`id`,
  `synthesize` → `Synthesis { rate, samples, words }`) and pass it with
  `Project::tts(..)`; read the API key from the environment. Without a provider
  or key, timing is estimated and there is no audio.
- The first render caches audio in `tts/`; commit that folder so CI renders
  the real voice without a key.
- A recorded voice-over: `voice("…").recording("vo/intro.wav")` plays the file
  instead of a provider. `oxframe transcribe vo/intro.wav` (local whisper.cpp)
  writes the word times, so `.at_word()` and captions follow the recording.
- Music: `Project::audio(Audio::track("assets/music.mp3").duck_under_voice())`.
- Subtitles: `oxframe render --subtitles out.srt` (`.vtt` by extension).
