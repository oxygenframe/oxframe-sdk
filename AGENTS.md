# Notes for coding agents

## Releasing

All crates and `oxframe-cli` share one version (see CHANGELOG.md).

1. `./scripts/release-check.sh` must pass. It runs this repository's CI and
   then the CLI (its project template, end to end) and the site (examples,
   docs, the compile service's sandbox template, e2e) against this
   checkout, and builds the container images on it with podman (or
   docker): the compile service's image is run with no network and must
   build a real scene. It also checks that the marketing pages quote only
   the timings in `oxframe-web/apps/site/src/lib/measurements.json`. It
   requires the current site docs to mention every public API this
   version's changelog names in backticks (`scripts/check-docs.py`), and a
   live player on the Going further pages that show a picture. Update those
   docs before tagging. It also renders this version's changelog as an MP4
   (`target/release-check/release.mp4`) through the sibling
   `release-video-action` checkout, the CLI, and this SDK. CI alone cannot
   catch an API change that breaks the other two repositories or the images.
2. Bump `[workspace.package] version` and the internal dependency versions,
   add the CHANGELOG.md section (breaking changes with "before → after").
3. Commit, tag `vX.Y.Z`, push the tag: the release workflow publishes to
   crates.io. Wait until the crates are there.
4. CLI: bump its version and `SDK_VERSION`, publish, tag.
5. Site: archive the previous docs (`bun scripts/archive-docs.ts <old
   version>` in oxframe-web/apps/site), bump `apps/site/src/lib/sdk.ts`,
   `bun run sync-examples`, push. The site and compile.oxframe.dev
   redeploy.

## Voiced scenes

`voice("…").start()` speaks at the current frame and returns a handle.
`line.at_word("word").await` waits until that word; `line.await` waits
until the end. Await the line, or `oxframe check` reports that it runs
past the scene. A line is `.priority()`, so it ducks a soundtrack built
with `.duck_under_voice()`.

oxframe does not call a speech service. Implement `TtsProvider` and
pass it to `Project::tts`. `synthesize` receives `TtsRequest` (`text`,
`voice`) and returns `Synthesis`: `rate`, mono `i16` `samples`, and
`words` (`TimedWord` with `text`, `start`, `end` in seconds). Read the
API key from the environment inside the provider. `id()` is part of the
cache key.

```rust
struct ElevenLabs;
impl TtsProvider for ElevenLabs {
    fn id(&self) -> &str { "elevenlabs" }
    fn synthesize(&self, req: &TtsRequest) -> Result<Synthesis, String> {
        let key = std::env::var("ELEVENLABS_API_KEY").map_err(|_| "set ELEVENLABS_API_KEY".into())?;
        let (rate, samples, words) = speak(&key, req.voice, req.text)?;
        Ok(Synthesis { rate, samples, words })
    }
}
Project::new("explainer").tts(ElevenLabs).speaker("Rachel").scene(intro())
```

The first render writes `tts/{hash}.json` and `{hash}.wav`. Commit that
folder so CI renders the real audio without a key. A missing key or a
failed call falls back to a speech-rate estimate and the scene still
runs, with no speech audio. An estimate is not cached. With no provider,
a committed cache is still found by the line text. Wasm and the
playground always estimate and never open files.

`oxframe render --subtitles out.srt` writes the lines (`.vtt` by
extension). `--no-audio` still writes them.
