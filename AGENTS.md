# Notes for coding agents

## Releasing

All crates and `oxframe-cli` share one version (see CHANGELOG.md).

Cadence: about one release a week. Merged work collects under
`## Unreleased` in CHANGELOG.md; do not tag a release per change. A
breaking change (a removed or renamed API, a changed default) waits for
the next minor version (0.21 → 0.22) and is listed under **Changed** or
**Breaking** with a before → after example; additions and fixes may go
out as a patch. An urgent fix (a crash, a wrong render, a broken install)
ships on its own as a patch release. The policy for users is on
https://oxframe.dev/docs/versions.

1. `./scripts/release-check.sh` must pass. It runs this repository's CI and
   then the CLI (its project template, end to end) and the site (examples,
   docs, the compile service's sandbox template, e2e) against this
   checkout, and builds the container images on it with podman (or
   docker): the compile service's image is run with no network and must
   build a real scene. It also checks that the marketing pages quote only
   the timings in `oxframe-web/apps/site/src/lib/measurements.json`. It
   requires the current site docs to mention every public API this
   version's changelog names in backticks (`scripts/check-docs.py`), and a
   live player on the Going further pages that show a picture, and that
   every Rust snippet on the site compiles on this SDK
   (`oxframe-web/apps/site/scripts/check-snippets.ts`). Update those docs
   before tagging. It also renders this version's changelog as an MP4
   (`target/release-check/release.mp4`) through the sibling
   `release-video-action` checkout, the CLI, and this SDK. CI alone cannot
   catch an API change that breaks the other two repositories or the images.
2. Bump `[workspace.package] version` and the internal dependency versions,
   add the CHANGELOG.md section (breaking changes with "before → after").
   Run `python3 scripts/build-skill.py` (the skill at `skills/oxframe/`,
   installed with `npx skills add oxygenframe/oxframe-sdk`); release-check
   fails while it is out of date.
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

## Video

With `oxframe = { ..., features = ["video"] }` and ffmpeg installed:

```rust
let demo = view.add(Video::new().file("assets/demo.mp4").width(pct(70)).radius(24.0));
demo.play();                               // returns at once
wait_for(2.0).await;
demo.playback_rate().to(0.25, 0.5).await;  // slow motion
demo.pause();
```

`time()` is a signal (seconds into the clip); `seek(t)` jumps. The frame is
a function of scene time, so renders are deterministic. Sound plays only at
rate 1; `.muted(true)` drops it. A missing file is a dark box and a
`video_unavailable` error in `check`. The playground cannot play videos.

## 3D

With `oxframe = { ..., features = ["3d"] }`:

```rust
let world = Scene3d::new();
let cube = world.add(Mesh::cube().color(Accent).roughness(0.35));
let frame = view.add(View3d::new(&world).size(760.0, 520.0));
cube.rotation().to(Vec3::new(0.0, 360.0, 0.0), 6.0).await;   // degrees
world.camera().position().to(Vec3::new(2.2, 1.8, 3.0), 2.0).await;
let to_view = frame.projector();                              // for bindings
label.x().bind(move || to_view(Vec3::new(0.0, 0.5, 0.0)).x);
```

Y is up; a cube is 1 unit. `oxframe-3d` depends only on the core
(`scripts/check-layering.sh`). Rendering is on the CPU, deterministic;
there are no shadows yet.

## Presentations

```rust
title.enter().await;
slide_break().await;                 // `oxframe present` waits here
notes("Each point on the next click.");
for _ in 0..3 { points.next().await; wait_for(1.5).await; slide_break().await; }
```

Hold with `wait_for` before each break: the video plays through the stops,
and `check` reports `too_fast` text otherwise.

A render plays through the stops. Every scene ends with a stop; a break at
the very end of a scene is the same stop, so do not exit on the last
point (the next scene's transition takes it away). Notes belong to the part
that ends at the next stop. `oxframe render --slides talk.pdf` writes one
page per stop.

## Matrices, vectors, tables

```rust
let a = view.add(Matrix::new([[1, 2], [3, 4]]).font_size(64.0));
a.row_frame(0).fade_in().await;          // also column_frame, cell_frame
a.cell(0, 1).text().set("7");
let v = axes.vector(2.0, 1.0);           // tip: v.x(), v.y(), v.to(x, y, s)
axes.apply_matrix([[1.0, 1.0], [0.0, 1.0]], 1.5).await;
let t = view.add(Table::new(["Name", "Time"]).row(["a", "3 ms"]));
t.insert_row(0, ["b", "1 ms"]).await;
t.sort_by_column(1, 0.8).await;
```

`apply_matrix` composes with what was applied before; `axes.matrix()` is
the `Mat2` signal. Ticks and labels stay put; placed nodes move but keep
their shape.

## Icons

With `oxframe = { ..., features = ["icons"] }`:

```rust
let db = view.add(icon!("lucide:database").size(64.0).color(Accent));
db.color().to(Danger, 0.3).await;
let flow = view.add(FlowDiagram::new().edge("api", "db").icon("db", icon!("lucide:database")));
```

Sets: `lucide:`, `tabler:` (`-filled` for fills), `simple:` (brand logos).
The data lives in `crates/oxframe-icons/data` (built by
`scripts/build-icons.py`); the proc macro copies only the named icons into
the binary. A wrong name is a compile error with the closest names.

## Nodes of your own

With `oxframe = { ..., features = ["unstable-node-api"] }`:

```rust
#[derive(Node)]
pub struct Gauge {
    #[signal] value: f64,
    #[signal] size: f64,
}
impl Draw for Gauge {
    fn draw(&self, ctx: &mut DrawCtx) {
        ctx.arc((0.0, 0.0), self.size / 2.0, 135.0, 270.0 * self.value).stroke(Accent, 16.0);
    }
    fn size(&self) -> (f64, f64) { (self.size, self.size) }
}
let g = view.add(Gauge::new().size(200.0)); // GaugeHandle
g.value().to(0.8, 1.0).await;
```

`draw` must depend only on the fields and `ctx.time()`. `size` is the box
for layout and `check`. Reach for this when no built-in node fits and the
shape is drawn, not composed: a composite of existing nodes is a function
that adds them to a `Group`. `oxframe init --template node my-node` makes a
crate for one.

## Charts

```rust
let axes = view.add(Axes::new().x_range(-5.0..5.0).y_range(-2.0..2.0));
let wave = axes.plot(|x| x.sin()).stroke(Accent);
wave.draw_on().await;
let x = Signal::new(-5.0);
axes.point_on(&wave, x.clone());
x.to(5.0, 2.0).await;
wave.to_fn(|x| (2.0 * x).sin(), 1.0).await;
axes.x_range().to(-10.0..10.0, 1.0).await;
```

A plot is a function of x, sampled every frame: it follows the ranges, and
`tan` keeps its gaps. Bars: `axes.bars([0.0, 0.0]).labels(["a", "b"])`, then
`bars.value(0).to(3.2, 1.0)`; set the x range to `-0.5..n - 0.5`.
`axes.place(node, x, y)` keeps any node at a data point. `NumberLine::new(0.0..10.0)`
is a single axis. There is no pie chart: draw slices with `Custom`.

## Shaders

Start with `shaders::waves()`. `.amp()`, `.speed()`, `.deep()` and
`.foam()` are signals, so `.to()` animates them. `.deep(Accent)` follows
the theme. The node is a box: `.width(pct(100))`, `.height()`,
`.to_edge(Edge::Bottom)`. It has no children; put a title on the view,
above the water.

`Shader::source` and `Shader::file` take WGSL that defines `struct Params`,
the uniform `p`, and `fn shade`. oxframe adds `ox` (`time`, `frame`,
`resolution`, `size`, `seed`, `view`, `progress`) and `ShadeIn` (`uv`,
`local`, `world`, `screen`). A name or type that is not in `Params` fails
`check`. Saving the file updates the next draw without a rebuild. The
waves preset is what gets painted. A custom `shade` is type-checked on
every draw.

Preset catalog, the first stop before any WGSL:

| You want | Use |
| --- | --- |
| A sea or a moving water background | `shaders::waves()` |
| Spray, sparks, rain | `Particles` (`.burst()`) or `Custom` with `ctx.rng(seed)` |
| A wet camera lens | `shaders::lens_drops()`, last, `.fullscreen()` |
| Rings on a node | `node.effect(shaders::ripple())` |
| A scene change through water | `Transition::shader(shaders::water_wipe())` |
| Anything else | `Custom`; custom WGSL is checked, not run |

`examples/src/ocean.rs` uses all of them in the target scene of hybrid
rendering. `oxframe mcp` has `render_graph`, the passes at one time.

Shaders that read drawn pixels:

- `shaders::lens_drops()` wets the lens over what was drawn before it.
  Add it last: `view.add(shaders::lens_drops().wetness(0.0).fullscreen())`,
  then `lens.wetness().to(1.0, 0.4).await` for a splash. `.seed()` moves
  the drops, `.drop_size()` scales them.
- `node.effect(shaders::ripple())` rings a node from its center. It returns
  an `Effect`; `.strength()` and `.speed()` are signals. Colors move, the
  outline stays, so a flat fill shows nothing.
- `transition(Transition::shader(shaders::water_wipe())).await` raises
  water into the next scene. Add the next scene's background before it.

Do not put `lens_drops` or an effect inside a group with opacity below 1,
a blur, a clip, a blend, a shadow or another effect: `check` reports
`input_cycle` and skips the shader. Put the effect on that group instead.
Events go through signals (`wetness`, `strength`) at the time they happen,
not through `ox.time`.
