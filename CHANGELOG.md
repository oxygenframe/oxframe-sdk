# Changelog

All crates of the workspace (`oxframe`, `oxframe-core`, `oxframe-2d`, …) and
`oxframe-cli` are released together under one version. Before 1.0 a breaking
change bumps the minor version (0.1 → 0.2) and a fix bumps the patch
(0.1.0 → 0.1.1). Breaking changes are listed separately with a
"before → after" example.

## Unreleased

### Added

- Components convert to `Node` (`From<X>` and `From<&X>`): `Axes`, `Callout`,
  `CodeExplainer`, `FlowDiagram`, `LowerThird`, `ChapterMarker`, `Matrix`,
  `Table`, `TitleCard`, `EndCard`, `BulletList` go wherever a node does, e.g.
  `caption.next_to(&end_card, Side::Bottom)`.
- `Txt::font_size`, `Txt::weight` and `Code::font_size` take integers too:
  `.font_size(64)`.
- `.scale(..)` on a builder takes one number, a pair or a `Vec2`:
  `.scale(1.5)`, `.scale((2.0, 0.5))`, `.scale(2)`.
- `oxframe check`: `low_contrast` reads the pixels behind text that lies
  over a shader, an image, a video, `Custom`, particles or 3D. The frame is
  drawn without text and the ratio is the median over the text's box; it
  must last 0.5 s to count, so something moving past a label is not
  reported. Text over plain fills is judged as before.
- A custom WGSL `shade` (`Shader::source`, `Shader::file`) is painted, not
  only checked: an interpreter over naga's IR runs it on the CPU, a tile of
  pixels at a time, with the same bytes on every OS (about 0.13 s for a
  1080p frame in a release build). Textures, samplers and derivatives are
  a `shader_error`. The playground does not paint it.
- Inputs of a custom shader: `.input("photo", Input::file("photo.png"))`
  (or `Input::image(include_bytes!(..))`) is read in WGSL with
  `sample_photo(uv)`. With `.input("scene", Input::frame())` the shader
  reads what was drawn before it and is painted over it inside its box,
  e.g. `sample_scene(i.screen / ox.resolution)`. Calling `sample_x` with no
  input `x` is a `shader_error` that says how to add it.

### Fixed

- A vector's label (`axes.vector(..).label("i")`) sits off to the side of
  the tip, so a vector along an axis no longer puts its label on the axis
  line.

### Breaking

- `Signal::set` takes `impl Into<T>`, like `Signal::to`, so
  `text.set("Done")` and `fill.set(Accent)` work. A value converted with a
  bare `.into()` no longer infers its type: drop the `.into()`.
  Before: `m.cell(0, 1).text().set("7".into());` → after:
  `m.cell(0, 1).text().set("7");`
- `Node::scale()` returns `ScaleSignal`, a `Signal<Vec2>` (it derefs to one)
  whose `to` and `set` also take one number for both axes:
  `node.scale().to(1.2, 0.3)`. Code that names the type changes.
  Before: `let s: Signal<Vec2> = node.scale();` → after:
  `let s: Signal<Vec2> = node.scale().signal();` (or `.into()`).
- `lint::Sample` has a `contrast` field (measured contrast per text node).
  Before: `Sample { time, nodes }` → after:
  `Sample { time, nodes, contrast: Vec::new() }`.
- `Input` is no longer `Copy` (it can hold an image); `Input::name` takes
  `&self`. Clone it where it was copied.

## 0.22.0 — 2026-10-23

### Added

- `oxframe check --against` (a directory) compares this check's frames with an
  earlier `oxframe check --frames` output: each frame that changed gets a
  share of changed pixels and `diff/<time>.png`, the frame dimmed with the
  changed pixels in red; `report.json` gets a `comparison` (changed, the
  same, added, removed). Check whether an edit changed only what it meant to.
- `oxframe check` reports where things move: `report.json` gets `motion`,
  each node that travels with its `file:line`, the stretches of time it
  moves visibly (`moves`, its keyframes), where it starts and ends, and the
  length of its path; a child that only rides with its parent is left out.
  `out/check/trajectories/<scene>.png` draws the paths on the scene's last
  frame, numbered as in `motion`.
- The storyboard shows both sides of a hard cut between scenes: the last
  frame of a scene and the first of the next.
- `Image::decode_png`.
- The quickstart example: install, `oxframe init` and `oxframe render` in a
  terminal, then the rendered scene plays. It is the animation in the
  README's quickstart (`scripts/readme-gif.sh` makes the GIF).

### Tested

- CPU frames are byte-for-byte the same on Linux (x86_64 and arm64), macOS
  (Apple Silicon) and Windows: CI renders ten examples on each and compares
  the hashes.

## 0.21.0 — 2026-10-23

### Changed

- Frames are drawn on the CPU (tiny-skia) by default, in `oxframe render`
  and in the preview of `oxframe dev` and `oxframe present`. The CPU is
  faster than the GPU on every scene of the benchmark, and it is the
  byte-for-byte deterministic renderer. `--gpu` opts into vello. `--cpu` is
  still accepted and wins over `--gpu`, so existing commands keep working.
  Before → after: `oxframe render` drew on the GPU and now draws on the
  CPU; `oxframe render --gpu` draws on the GPU as the default used to.

### Added

- `oxframe check` writes a crop for each issue on a node:
  `out/check/issues/<n>-<rule>.png`, the part of the frame around the node
  at full resolution with the node outlined in red (up to 24 per check). In
  `report.json` such an issue has `bounds`, the node's rect at the start of
  the issue in scene units (as in `inspect`), and `crop`, the path relative
  to `out/check/`. A node wholly outside the frame gets the strip along the
  edge it is past.
- `Math::new("e^(i pi) + 1 = 0")`: formulas set by Typst, behind the new
  `math` feature (crate `oxframe-math`). `.size(px)` is the font size, as
  for `Txt`; `.display()` sets the formula on its own line (limits above and
  below, full-size fractions); `.color(..)` and the `color()` signal paint
  every glyph; `paths()` are the glyphs and rules in reading order, for
  `stagger`. The font (New Computer Modern Math) is bundled, so no TeX is
  needed. The feature adds the Typst compiler and a 1.4 MB font, so the
  browser playground does not have it; the `formulas` example runs with
  `cargo run -p oxframe-examples --features math --bin formulas -- preview`.
- docs.rs now documents the `icons`, `3d`, `math` and `unstable-node-api`
  features.
- CODE_OF_CONDUCT.md, SECURITY.md and CONTRIBUTING.md; plugins for Cursor
  and Codex with the same skills and `oxframe mcp` as the Claude Code one;
  a README quickstart from install to the first render.

### Faster

- The CPU renderer keeps a layer's blurred drop shadow across frames while
  the layer does not change, instead of blurring it again on every frame. A
  fading or blurred group keeps its cached pixels too. Output is unchanged.

## 0.20.5 — 2026-10-22

### Added

- The one_shot example: the oxframe promo video, one scene and one
  camera with no cuts, with its voice committed so it renders without a
  key. Its source is the code behind the video on oxframe.dev and X.

## 0.20.4 — 2026-10-22

### Fixed

- `oxframe dev`: after a rebuild while the video plays, the picture and
  the sound go on together from the frame they were on. The editor used to
  start the sound from the beginning over a still picture.

## 0.20.3 — 2026-10-22

### Fixed

- `oxframe check` reported `text_overflow` for text in a row of a `Layout`
  nested in another one: a row that draws nothing itself was placed as if
  there were no layout. `oxframe inspect` now gives every node the box it
  is drawn in.
- `oxframe check` reported `dead_time` while text was written on word by
  word, while a glow changed and while a sea from `shaders::waves()` moved.
  They count as change now; `oxframe inspect` lists how much of a node is
  drawn and its glow among its params.

### Changed

- The `release` example: each section's heading in its color, the changes
  written on word by word with a short halo, and a calm sea along the
  bottom. release-video-action 1.3 makes its videos the same way.

## 0.20.2 — 2026-10-22

### Fixed

- `TitleCard` and `EndCard` wrap a long title or second line inside the
  card, and the card grows to hold it. A long line used to run past the
  card's edges, which release videos with long notes showed.
- Centered and end-aligned text with a `max_width` was drawn beside its own
  box when it was narrower than `max_width` (a single short line, say). It
  now sits inside the box, so placing it and `oxframe check` see where it
  really is.

## 0.20.1 — 2026-10-22

### Added

- Every issue in `report.json` has `docs`: the link to its rule on
  https://oxframe.dev/docs/check-rules, with what the rule reports and how to
  fix it (`Issue::docs`).
- The `showreel` example: scenes from other examples between a title and an
  end card, the reel on oxframe.dev.

### Fixed

- `check` reported `too_fast` at a cut between two scenes when the first
  node of each had the same id: node ids start again in every scene, and
  a move is now only a node added at the same place.

## 0.20.0 — 2026-10-21

Parallel rendering.

### Added

- `render --jobs 4` renders a video in 2-second segments, several at
  once, each in its own process; the segments are joined without
  re-encoding and the soundtrack is added once. The default is one job per
  core, at most eight. A PNG sequence is split the same way.
- The file does not depend on the number of jobs: `--jobs 1` and
  `--jobs 8` write the same bytes. A 7-second 3D scene renders 2.7 times
  faster on 8 cores.
- `oxframe_export::concat` joins segments encoded with the same settings.

### Changed

- A video longer than two seconds now has a key frame every two seconds
  (where the segments meet), so files are a little larger than before.

## 0.19.0 — 2026-10-21

3D scenes inside 2D ones, behind the `3d` feature.

### Added

- `Scene3d` is a world of meshes: `Mesh::cube()`, `Mesh::sphere()`,
  `Mesh::plane()`, `Mesh::cylinder()`, `Mesh::group()` and models from a
  binary glTF (`Mesh::glb_file`). `.color()`, `.metallic()`,
  `.roughness()`, `.emissive()` and `.opacity()` set the material.
- `scene.add(mesh)` returns an `Object3d` whose position, rotation,
  scale, color and opacity are signals, so `.to()` animates them;
  `object.add(mesh)` makes a child that moves with its parent.
- `scene.camera()` (position, target, field of view) and
  `scene.light(Light::directional(..))` or `Light::point(..)` are signals
  too; with no light, a default sunlight shades each face differently.
- `View3d::new(&scene).size(640.0, 400.0)` shows the scene as a 2D node:
  position, opacity, effects, the 2D camera and layout work as for any
  node. `view.projector()` maps a 3D point into the view, so a 2D label
  follows a corner of a model.
- The renderer is a software rasterizer with a depth buffer and 2 × 2
  supersampling: the same scene gives the same pixels in a render, in
  `check`, in CI without a GPU and in the browser playground.
- `DrawCtx::image` draws a picture from a node of your own.
- The `cube_3d` example.

## 0.18.0 — 2026-10-21

Presentations from the same scenes.

### Added

- `slide_break().await` marks a stop. A render plays straight through it;
  `oxframe present` (CLI 0.18.0) pauses there until the next key press.
  The last frame of every scene is a stop too.
- `notes("…")` adds speaker notes for the part that ends at the next stop.
- `render --slides talk.pdf` writes a PDF with the frame of every stop,
  one per page, without ffmpeg.
- The timeline sent to the editor lists the stops.
- The `slides` example.

### Fixed

- Rendering with `--subtitles` from `oxframe render` now works: the CLI
  passes the flag to the project (CLI 0.18.0).

## 0.17.0 — 2026-10-21

Matrices, vectors and tables.

### Added

- `Matrix::new([[1, 2], [3, 4]])` draws values in square brackets. Each
  cell is a `Txt` from `cell(r, c)`, `row(r)` or `column(c)`;
  `row_frame`, `column_frame` and `cell_frame` give hidden outlines to
  fade in. `.cell_width(px)` leaves room for longer values.
- `axes.vector(x, y)` draws an arrow from the origin whose tip is two
  signals; `v.to(x, y, seconds)` moves it and `.label("v")` follows it.
- `axes.apply_matrix(m, seconds)` maps the plane through a 2 × 2 matrix:
  a transformed grid is drawn over the original, and plots, series,
  vectors and placed nodes move with it. `axes.matrix()` is the `Mat2`
  signal; `Mat2::after` multiplies and `Mat2::det` gives the determinant.
- `Table::new` takes the column titles and `.row(..)` adds rows.
  `insert_row`, `push_row` and `remove_row` slide the other rows into
  place, `sort_by_column` and `sort_by` move every row to its sorted
  place, and `table.row(i)` is a `TableRow` with a background to
  highlight it.
- Guide lesson 15, and the `matrix_multiply`, `linear_map` and
  `table_sort` examples.

## 0.16.0 — 2026-10-21

Icons from Lucide, Tabler and Simple Icons, behind the `icons` feature.

### Added

- `icon!("lucide:server")` adds an icon by set and name: `lucide:`,
  `tabler:` (with `-filled` variants) and `simple:` for brand logos. Only
  the icons a project names are put into its binary. A name that does not
  exist fails to compile and lists the closest names.
- `.size()`, `.color()` and `.stroke_width()` on the builder; the `Icon`
  handle has `color()` and `stroke_width()` as signals, `Icon::paths` for
  `draw_on`, and `Icon::style` (`IconStyle::Stroke` or `IconStyle::Fill`).
  The color defaults to the theme's text color.
- `Icon::from_svg` makes the same node from a 24 by 24 SVG of your own.
- `FlowDiagram` puts an icon at the start of a card with
  `.icon(name, icon!(..))`; `flow.icon(name)` returns its handle.
- The `architecture` example.

### Fixed

- Creating GPU renderers on several threads at once could crash inside the
  Vulkan driver. Renderers now share one Vulkan, Metal or DX12 instance;
  a GL-only machine still gets one per renderer.

## 0.15.0 — 2026-10-21

Video clips in a scene, behind the `video` feature.

### Added

- `Video::new().file("assets/demo.mp4")` shows a clip as a node, sized
  with `.width(pct(70))` like an image, with `.radius(24.0)` corners,
  effects and the camera. `demo.play()`, `demo.pause()` and
  `demo.seek(2.0)` control it; `demo.playback_rate()` and `demo.time()`
  are signals, so `demo.playback_rate().to(0.25, 0.5)` slows it down.
- The frame shown is a function of the scene's time, so a render and a
  seek show the same frame. Frames are decoded with ffmpeg and cached;
  WebM (VP9) and ProRes 4444 keep their alpha.
- The clip's sound goes to the mixer while it plays at rate 1;
  `.muted(true)` leaves it out.
- `check` reports `video_unavailable` when a clip cannot be opened.
- The `video` example.

### Fixed

- The mixer read the first track of a file; for a video that is the
  picture. It now takes the first audio track.

## 0.14.0 — 2026-10-21

Nodes of your own, behind the `unstable-node-api` feature.

### Added

- `#[derive(Node)]` on a struct that implements `Draw` makes it a node.
  Fields marked `#[signal]` get a setter on the builder (`Gauge::new()`)
  and a signal on the handle (`GaugeHandle`), so `gauge.value().to(0.8, 1.0)`
  animates them. Other fields are set once.
- `Draw::draw` paints with the same `DrawCtx` as `Custom`, and
  `Draw::size` gives the box that layout, `Camera::fit` and `check` use.
  The node gets position, opacity, effects, flex properties and theme
  colors like any built-in one, and `check` names it by its type.
- `DrawCtx::arc` strokes or fills an arc, for gauges and rings.
- `oxframe init --template node my-node` makes a crate with a node, a demo
  scene and a reference test.
- The `gauge` example.

## 0.13.0 — 2026-10-21

Charts: axes, number lines, function plots and bars, all following
animated ranges.

### Added

- `Axes::new()` is a plotting box with a grid, axes, ticks and labels.
  `.x_range(-5.0..5.0)` and `.y_range(-2.0..2.0)` set the data ranges,
  `.width()` and `.height()` the box, `.no_grid()` drops the grid.
  `axes.x_range()` is a signal of `Interval`, so
  `axes.x_range().to(-10.0..10.0, 1.0)` zooms; ticks stay on round steps
  and each label keeps its value while the range slides.
- `axes.plot(|x| x.sin())` draws a function as a `Plot`. It is sampled
  again every frame, finer where it bends, broken at jumps (no line across
  the gaps of `tan`) and cut at the box. `.stroke(Accent)` and `.width(4.0)`
  style it, `.draw_on()` draws it, and `plot.to_fn(|x| x.cos(), 1.0)`
  morphs it into another function.
- `axes.point_on(&plot, x)` keeps a dot on the curve at a signal `x`.
  `axes.c2p(x, y)` gives the scene point of a data point, and
  `axes.place(node, x, y)` keeps a node there.
- `axes.bars([3.0, 7.5])` draws bars with their values above them;
  `bars.value(0).to(9.0, 1.0)` grows one, `.labels(["a", "b"])` names them.
  `axes.line_series(..)` and `axes.scatter(..)` draw data points.
- `NumberLine::new(0.0..10.0)` is a single axis with the same methods.
- Guide lesson 14 (charts) and the `charts` example.

### Fixed

- A long `BulletList` row ran off both edges of the frame. Rows now wrap
  inside 70 % of the frame width, start right of the mark, and take the
  height they need. Short lists look as before.
- `check` pointed at the component's own source for nodes a component
  adds. It now names the line of your `add` call.

## 0.12.1 — 2026-10-21

### Added

- `oxframe inspect` lists each shader node's numeric parameters (`amp`,
  `wetness`, …) and the strength and speed of an effect, under `params`.
- Guide lessons 11 to 13 as examples: scenes and transitions, sound and
  voice, shaders and effects.
- The `ocean` example: the target scene of hybrid rendering.

### Fixed

- `check` reported `dead_time` while a shader parameter or an effect was
  animating, such as a growing wave or a wetting lens. Those changes now
  count as motion.

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
  A renderer skips them (Primitive::is_marker on docs.rs); the engine
  handles them.
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
