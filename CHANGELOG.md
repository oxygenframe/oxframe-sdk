# Changelog

All crates of the workspace (`oxframe`, `oxframe-core`, `oxframe-2d`, …) and
`oxframe-cli` are released together under one version. Before 1.0 a breaking
change bumps the minor version (0.1 → 0.2) and a fix bumps the patch
(0.1.0 → 0.1.1). Breaking changes are listed separately with a
"before → after" example.

## 0.12.0 — 2026-10-21

Shaders can read what is already drawn. A lens gets wet over the frame, a
node ripples, and a scene gives way through rising water. All three run
on the CPU, so `check` and a render without a GPU paint them, and the GPU
renderer draws the same picture.

### Added

- `shaders::lens_drops()` puts drops on the lens over everything drawn
  before it, inside its box. Add it last and make it `.fullscreen()`.
  `.wetness()` is a signal from 0 (dry) to 1, so
  `lens.wetness().to(1.0, 0.4)` is a splash. `.seed()` places the drops and `.drop_size()` scales
  them. `.input("scene", Input::frame())` names what it reads;
  `Input::backdrop()` is the same outside a layer.
- `Node::effect` runs a shader on a node's own pixels, children included.
  `shaders::ripple()` sends rings out from the node's center: `.strength()`
  and `.speed()` are signals on the returned `Effect`. The outline stays,
  so use it on text, a gradient, an image or water.
- `Transition::shader` paints a transition with a shader preset.
  `shaders::water_wipe()` raises a wavy waterline over the frame; below it
  is the next scene, rippled near the line. `.wave()` sets the wave height.
- `check` reports `input_cycle` (an error) when a shader would read pixels
  that are not finished: `lens_drops` or an effect inside a group with
  opacity below 1, a blur, a clip, a blend, a shadow or another effect.
  The shader is skipped there. A misspelled input or `lens_drops`
  parameter is `param_unknown` with the right name.
- `inspect --graph` shows effect passes with the shader that runs, and
  `inspect` names the preset of each shader node.
- `Svg::new().width()` and `.height()` take `pct()`, read when the node is
  added. A `Shader` handle converts to `Node`, so `Camera::center_on`
  works on water.

### Fixed

- Renderers kept every image they had drawn. A waves node paints a new
  image each frame, so a long render grew without bound. They now keep
  only the images of the current frame.
- A shader wider than the safe area (water to the edges) is no longer an
  `out_of_frame` warning, and `lens_drops` over text is not `covers_text`.

### Not in this version

`Input::node`, `Input::image` and `.offscreen()` from the roadmap are not
built. A custom WGSL `shade` is still checked, not run. `low_contrast` does
not read shader pixels yet.

### Breaking

- A renderer of your own needs `render_list`, which draws a part of the
  draw list. Before → after:

      // before
      fn render(&mut self, graph: &SceneGraph, o: &RenderOptions) -> Image {
          draw(&graph.draw_list(), o)
      }
      // after
      fn render(&mut self, graph: &SceneGraph, o: &RenderOptions) -> Image {
          self.render_list(graph, &graph.draw_list(), o)
      }
      fn render_list(&mut self, _: &SceneGraph, items: &[DrawItem], o: &RenderOptions) -> Image {
          draw(items, o)
      }

- The draw list has three markers, ReadFrame, BeginEffect and EndEffect.
  A renderer skips them (`Primitive::is_marker`); the engine handles them.
- The pass and inspect node structs have a new `shader` field.

## 0.11.1 — 2026-10-21

### Fixed

- The waves example stopped with "unreachable executed" in the browser
  playground. The timer that reports a slow shader read the clock, and
  the clock panics in WebAssembly. It is now skipped there, so the
  example plays. Native builds, `oxframe dev` and the CLI were not
  affected, so there is no new CLI release.

## 0.11.0 — 2026-10-21

A scene can paint a wave as a node. The preset renders in a check with no
GPU, and the same image is what the GPU draws.

### Added

- `shaders::waves()` is a box in the scene. `.amp()`, `.deep()` and
  `.foam()` are signals, and a theme role such as `Accent` follows the
  theme. `.uniform()` sets a numeric field of your own shader.
- `Shader::source` keeps WGSL in the scene. `Shader::file` reads it on
  every draw, so saving the file updates the picture without a rebuild.
  The text defines `Params` and `shade`. oxframe adds `ShadeIn`. A wrong
  name or type fails the check and lists the fields. WGSL that does not
  compile fails it too, with the line in your source.
- A check paints shader layers at half resolution. An opaque shader over
  text is a warning. A pass over the draft budget is a warning.

## 0.10.0 — 2026-10-21

A scene can wait for one word of a spoken line. Without a provider the
length is a speech-rate estimate, so the playground and CI still run.

### Added

- `Project::tts` speaks through a `TtsProvider`. The key stays in the
  provider. A failed call falls back to the estimate and the scene still
  runs. `voice("one two").start()` starts the line. `.at_word()` waits
  until that word. Awaiting the line waits until the end. A line is
  `.priority()`, so it ducks a soundtrack built with `.duck_under_voice()`.
- The recording is cached under `tts/` next to the scene. Commit that
  folder to render in CI without a key. Without a provider, oxframe reads
  a committed cache and otherwise estimates the length.
- `oxframe render --subtitles out.srt` writes the lines. A path ending in
  `.vtt` writes WebVTT. `--no-audio` still writes them, and `--range`
  shifts the cues into that part of the video.

## 0.9.0 — 2026-10-21

Sound effects mix with the soundtrack into one track. ffmpeg only encodes
that mix, so the samples do not depend on the ffmpeg version.

### Added

- `Audio::track` builds the soundtrack. `Project::audio("music.mp3")` still
  works. `.volume()`, `.fade_in()`, `.fade_out()` and `.master_fade()` shape
  it. `.duck_under_voice()` lowers it under a clip marked `.priority()`.
- `sound("click.wav")` starts at the current frame. `.play()` does not wait.
  Awaiting it waits until the clip ends. `.trim()` plays a slice of the file,
  which is also how long the wait is.
- The editor draws each effect on the timeline and plays the mix while you
  play or seek. `--no-audio` leaves effects out. `--range` keeps only the
  effects inside that part of the video.

## 0.8.0 — 2026-10-21

A frame's draw list is a pass graph, and a layer whose vectors did not
change is reused (ROADMAP task 15, stage 2). No new scene API and no shaders.

### Added

- Render passes in topological order: one vector pass when nothing opens a
  layer, then a composite pass for a group opacity, a blend, a blur, a shadow
  or a clip. Contents run before the composite that uses them.
- A static layer is cached as the pixmap from before `composite`, so the next
  frame skips redrawing that layer. The root vector pass is not hashed, and a
  layer that moved or changed is drawn again. CPU bytes stay the reference.
- `inspect --graph` adds a `passes` array (`kind`, `node`, `name`, `items`,
  `depth`, `opacity`, `blend`, `filtered`). Without the flag the JSON is
  unchanged.

## 0.7.0 — 2026-10-21

A node can draw its own geometry, and a spray keeps a seeded simulation
(ROADMAP task 15, stage 1). No shaders yet.

### Added

- `Custom::new(|ctx, t| { ... })`. `ctx.line`, `ctx.circle`, `ctx.rect` and
  `ctx.path` take `.fill` or `.stroke`. `ctx.rng(seed)` does not advance
  `random()`, and the same seed draws the same shapes after a seek.
- `Particles::new().seed(7).gravity(900.0).toward_camera()` and
  `burst(origin, count).await`. Integration uses a fixed 1/120 s step and a
  checkpoint every 30 steps, so seeking back replays at most that many steps
  and lands on the same dots. `toward_camera` only scales the dots.
- `oxframe check` reports `nondeterministic` when painting the same frame
  twice does not match.
- Example `spray`: rain from `Custom`, then a burst into the camera.

## 0.6.0 — 2026-10-21

Scenes can blend into each other (ROADMAP task 5).

### Added

- `transition(Transition::slide(Dir::Left)).await` at the start of a scene.
  Also `fade`, `wipe`, `zoom_in`, `zoom_out`, `.duration`, `.ease`, `.overlap`,
  and `Transition::custom`. The length is stored in `meta.json` and can be
  dragged on the timeline. `render --scene` leaves the incoming blend out.
  `check` samples it and `inspect` reports kind and progress.
- Example `transitions`: a title card slides into an end card.
- The browser presents a transition as one canvas frame. Both layers are drawn first, then blended.

## 0.5.0 — 2026-10-20

Code edits itself (ROADMAP task 2). `Txt` still types its text out.
SVG files become ordinary nodes (ROADMAP task 3).

### Added

- `code.code().to(next, seconds)` diffs the two sources. Tokens that stay
  slide to their new places, deleted tokens fade and collapse, new tokens
  fade in. Lines below the edit shift. Highlighting stays on the six
  languages and is taken from each side, not from a half-typed string.
- `insert(line, col, text, seconds)`, `remove(((line, col), (line, col)), seconds)`
  and `replace(range, text, seconds)`. Lines are 1-based, columns are
  0-based characters, and the range is half-open. `.flash()` on that tween
  lights the inserted text for a moment.
- A focus on lines that move rides with them and lands on their new numbers
  when the tween ends.
- Example `code_steps`: a function written in a few edits.
- `Svg` loads a file (`.src(include_bytes!(..))` or `.file(path)`) and
  expands it into groups, paths and images. `.width` / `.height` keep the
  aspect ratio. `svg.find("id")` is the element with that id, `svg.paths()`
  is every contour in paint order, and those paths morph through `data`
  like a path written by hand. Filters, masks and clip paths are skipped
  with a warning.
- Example `svg`: a logo drawn on path by path, then one part recolored.

### Breaking

- `code.code()` is no longer a `Signal<String>`. `.get()`, `.set()` and
  `.bind()` still work. `.to()` is a diff, not a typewriter:

  ```rust
  // before — `code()` was a Signal<String>, and `.to()` typed the text
  let src: Signal<String> = code.code();
  src.to(next, 1.0).await;

  // after — `.to()` slides tokens; `.get()` / `.set()` / `.bind()` remain
  code.code().to(next, 1.0).await;
  code.insert(2, 0, "    let y = x * 2;\n", 0.6).flash().await;
  ```

## 0.4.0 — 2026-10-20

### Added

- `Camera` (ROADMAP task 1): a node that zooms, pans and turns the view of
  everything added to it. Signals `x`/`y` (the point it looks at), `zoom`,
  `rotation`; `center_on(node, s)`, `fit(node, padding, s)` (measures the
  node when called, so a growing layout fits at its current size) and
  `reset(s)`. Follow a moving node with `cam.x().bind(..)`. Nodes added to
  the view stay on screen (titles, captions, a HUD).
- Zoomed content is drawn at the final scale on both renderers: text and
  outlines stay sharp at ×10, blurs and shadows scale with the zoom
  (golden frames `camera_zoom`, `camera_rotate_blur`; GPU within ΔE 0.04).
- `oxframe check` does not report `out_of_frame` for nodes seen through a
  camera (framing a detail is the point); `inspect` marks them `in_camera`.
- Example `camera`: a tour of a system diagram.

### Changed

- `Signal::to` takes anything that converts into the value: theme tokens
  (`rect.fill().to(Accent, 0.3)`), integers for `f64`, `&str` for text.

### Breaking

- An explicit `.into()` in `.to(..)` is now ambiguous; drop it:

  ```rust
  // before
  title.text().to("Hello".into(), 1.0).await;
  // after
  title.text().to("Hello", 1.0).await;
  ```

## 0.3.0 — 2026-10-20

Scene code without Rust plumbing (ROADMAP task 4): no `Box::pin`, no
`BoxFuture` in a typical scene.

### Added

- `stagger(delay, items, |x| animation)`: the same animation on many items,
  each starting `delay` seconds after the previous one.
- `all(..)` takes a `Vec`, an array or an iterator of animations of one kind
  as well as a tuple: `all(cards.iter().map(|c| c.fade_in()))`.
- `any(..)`: completes when the first animation does (returns its index);
  the others stop where they are.
- `repeat(n, || animation)` and `loop_while(|| condition, || animation)`.
- Node helpers, timed by the theme's motion tokens: `pop_in()` (scale from
  zero with overshoot), `slide_in(Side::Left)` (slides in and fades in),
  `draw_on()` (outline drawn from start to end), next to the existing
  `fade_in()` / `fade_out()`.
- `Tween::duration(s)` changes a helper's length: `title.fade_in().duration(0.3)`.
- Clearer compile errors: `all(5)`, `all(a, b)` and a `stagger` closure that
  ends in `;` explain how to fix them.

### Breaking

- `sequence(delay, futures)` takes any `Vec`, array or iterator of
  animations instead of `Vec<BoxFuture>`. Code that built the list with
  `.collect()` no longer infers its type; use `stagger` instead:

  ```rust
  // before
  sequence(0.15, cards.iter()
      .map(|c| -> oxframe::BoxFuture { Box::pin(c.opacity().to(1.0, 0.5)) })
      .collect()).await;
  // after
  stagger(0.15, &cards, |c| c.opacity().to(1.0, 0.5)).await;
  ```

  Several steps per item: return an `async move { … }` block from the
  closure, no boxing.

- `all_vec(v)` is deprecated: `all(v)` takes the `Vec` directly.
- `CodeExplainer::next()` returns `impl Future` instead of `BoxFuture`
  (awaiting it is unchanged).

## 0.2.6 — 2026-10-20

The first published release after 0.2.1: it also contains everything listed
under 0.2.2–0.2.5 below, which were not published separately.

No crate changes. Released together with oxframe-cli 0.2.6 (`oxframe mcp`,
`--json`, and hints on the compile errors agents hit most often).

The scene API and the editor protocol are unchanged.

## 0.2.5 — 2026-10-20

### Added

- Crate `oxframe-components`, re-exported from the prelude. `TitleCard`,
  `ChapterMarker`, `BulletList`, `CodeExplainer`, `FlowDiagram`, `Callout`,
  `LowerThird` and `EndCard` are added like any other node. Each has
  `enter`, `next` and `exit`. They take color and type from the theme.
  `LowerThird` and `ChapterMarker` pin to the safe area. `FlowDiagram` lays
  its own nodes out in layers (`.down()` stacks them).
- `Node::from_id`, so another crate can implement `NodeBuilder`.

The scene API and the editor protocol are unchanged.

## 0.2.4 — 2026-10-20

### Added

- Relative placement. `.next_to(other, Side::Bottom)`, `.align_to(other, Edge::Top)`,
  `.to_edge(Edge::Top)` and `.center_in(other)` are recomputed every frame, so a
  node follows a neighbor that moves or resizes. The gap in `next_to` is the
  theme's `Md` step. `to_edge` pins to the safe area; a portrait frame leaves
  room for a vertical video's status bar and buttons.
- `.width(pct(80))` and `.height(pct(50))`: a fraction of the frame, or of the
  parent when that parent has a fixed size.
- `view.safe_area()` and `view.is_portrait()`.
- `Project::aspects([Landscape, Portrait, Square])`. `check` lints every
  declared format. `render --aspect 9:16` exports one; `--aspect all` exports
  each, with the shape in the file name.

The scene API and the editor protocol are unchanged. A project that does not
declare aspects renders and checks at its own size, as before.

## 0.2.3 — 2026-10-20

### Added

- Themes and design tokens. `Project::theme(Theme::dark().accent(...))` (also
  `light`, `midnight`, `paper`, `vivid`) supplies colors, type roles
  (`.display()`, `.h1()`, `.body()`, `.caption()`, `.code()`), spacing
  (`.padding(Md)`), radii, shadows (`.elevation(Md)`) and motion
  (`Fast`/`Normal`/`Slow`, `fade_in`). A token is accepted wherever the
  concrete value is. Text with no fill uses the theme's text color.
  `.accent(color)` keeps a 4.5:1 contrast. `[theme]` in `oxframe.toml`
  overrides the compiled theme.
- `RELEASE.md`: before a release tag, publish the crates, then move
  oxframe-web's playground pin to that version. The docs version list already
  follows crates.io; the compile image cannot.

The scene API and the editor protocol are unchanged. Existing explicit colors
are unchanged. The default theme is the previous dark background and white text.

## 0.2.2 — 2026-10-20

### Added

- `check` and `inspect` subcommands on the project binary (what `oxframe check`
  and `oxframe inspect` run). `check` samples the scene on the CPU, writes a
  storyboard and a JSON report, and fails when a node stays outside the frame,
  text overlaps or overflows, or text contrast is below WCAG 4.5:1. Text that
  flashes too briefly and a long still with no soundtrack are reported too.
- `.name("title")` on every node builder. `oxframe check` and `oxframe inspect`
  use it; without a name the node is labeled by the `add` call site.

## 0.2.1 — 2026-10-20

### Fixed

- The browser engine (`oxframe` with the `wasm` feature, used by the
  playground) did not compile in 0.2.0. CI now builds it.

## 0.2.0 — 2026-10-20

### Changed

- **Crate layering.** `oxframe-core` now owns `Image` (RGBA pixels) and the new
  `FrameSource` trait — "give me frame *n*" — through which the editor
  (`oxframe-preview`), `render`/`snapshot` and the exporter get frames
  without knowing what draws them. `oxframe-preview` and `oxframe-export` no
  longer depend on the 2D crate or the renderers; CI checks this
  (`scripts/check-layering.sh`). Scene code is unchanged and renders
  byte-identically.
- The crate docs and README carry a development-status notice.

### Breaking (only for code using the internal crates directly)

- `oxframe_preview::PreviewEngine` now extends `oxframe_core::FrameSource`:
  `fps`, `size`, `total_frames` and `render` moved to that trait.

  ```rust
  // before
  impl PreviewEngine for MyEngine { fn fps(&self) -> u32 { 60 } /* … */ }
  // after
  impl FrameSource for MyEngine { fn fps(&self) -> u32 { 60 } /* … */ }
  impl PreviewEngine for MyEngine { /* the editor-specific methods */ }
  ```

- `oxframe_render::Image` is a re-export of `oxframe_core::Image`;
  `Image::encode_png` still exists.

## 0.1.0 — 2026-10-19

First release: scenes as coroutines, signals and tweens, 2D nodes with
flexbox layout, text and code, effects, CPU (tiny-skia) and GPU (vello)
renderers, the editor protocol, video export through ffmpeg and the
in-browser (wasm) engine.
