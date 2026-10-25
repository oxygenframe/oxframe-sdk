<!-- oxframe-agents 0.23.6 -->
# oxframe API cheat sheet

`use oxframe::prelude::*;` brings everything below into scope.

## Scenes and project

```rust
#[oxframe::scene]
async fn intro(view: View) { /* … */ }          // view: the root; view.size() -> (w, h)

fn main() {
    Project::new("name").size(1920, 1080).fps(60).seed(7)
        .background(color!("#141414")).audio("assets/voice.mp3")
        .scene(intro()).scene(outro())             // scenes play in order
        .main();
}
```

## Sound

```rust
.audio(Audio::track("assets/music.mp3").duck_under_voice().volume(0.8).master_fade(0.2, 0.4))
// a path still works: .audio("assets/music.mp3")

sound("assets/whoosh.wav").volume(0.6).fade_in(0.05).play();       // starts now, does not wait
sound("assets/click.wav").trim(0.0, 0.2).priority().await;         // waits out the slice
```

`.play()` records the clip at the current frame and continues. Awaiting waits until the clip ends. `.trim(start, end)` is the length of that wait and does not need to open the file. `.priority()` ducks a soundtrack built with `.duck_under_voice()`. Effects are clips on the timeline and play during playback and seeking. `oxframe render` mixes them with the soundtrack before ffmpeg encodes; `--no-audio` drops both, and `--range` keeps only the part inside the range. Dragging the waveform changes the soundtrack offset in `meta.json`. `Project::new(..).loudness(-14.0)` brings the whole mix to -14 LUFS (YouTube; `-16.0` for podcasts); the render prints the loudness it got.

## Voice

```rust
let line = voice("First draw a square, then rotate it").start();
line.at_word("square").await;          // waits until that word
square.draw_on().await;
line.await;                             // waits until the line ends
```

`voice("one two").start()` speaks now and returns a handle. `.at_word()` matches the word with surrounding punctuation stripped and panics if it is missing, listing the words. Awaiting the line waits until the end. A line is `.priority()`, so it ducks a soundtrack built with `.duck_under_voice()`. Await the line: one that runs past the scene is an error from `oxframe check`, and two overlapping lines are a warning.

oxframe does not call a speech service. Implement `TtsProvider` and pass it to `Project::tts`. `synthesize` gets `TtsRequest` (`text`, `voice`) and returns `Synthesis`: `rate`, mono `i16` `samples`, and `words` (`TimedWord`: `text`, `start`, `end` in seconds from the start of the samples). `.at_word()` waits on those times. Read the API key from the environment inside the provider.

```rust
use oxframe::{Synthesis, TtsProvider, TtsRequest};

struct ElevenLabs;

impl TtsProvider for ElevenLabs {
    fn id(&self) -> &str { "elevenlabs" }   // part of the cache key

    fn synthesize(&self, req: &TtsRequest) -> Result<Synthesis, String> {
        let key = std::env::var("ELEVENLABS_API_KEY")
            .map_err(|_| "set ELEVENLABS_API_KEY".to_string())?;
        let (rate, samples, words) = speak(&key, req.voice, req.text)?;
        Ok(Synthesis { rate, samples, words })
    }
}

Project::new("explainer").tts(ElevenLabs).speaker("Rachel")
    .audio(Audio::track("assets/music.mp3").duck_under_voice())
    .scene(intro())
```

The first render writes `tts/{hash}.wav` and `tts/{hash}.json`. The next render plays the file. Commit that folder so CI has the audio without a key. A missing key or a failed call falls back to a speech-rate estimate (2.5 words per second, at least 0.3s) and the scene still runs, with no speech audio. An estimate is not cached. With no provider, a committed cache is still found by the line text. The playground always estimates.

A voice-over recorded by a person: `voice("…").recording("vo/intro.wav")` plays the file instead of calling the provider, and the line lasts as long as it. `oxframe transcribe vo/intro.wav` (needs whisper.cpp's `whisper-cli`; runs locally) writes `vo/intro.words.json` with the word times, so `.at_word()` and `.caption()` follow the recording; without that file the words are spread over its length. Write the line's text as spoken, since captions and subtitles show it.

`oxframe render --subtitles out.srt` writes the lines. A path ending in `.vtt` writes WebVTT. `--no-audio` still writes them, and `--range` shifts the cues.

## Shaders

```rust
let sea = view.add(shaders::waves().amp(0.05).deep(Accent).foam(Surface)
    .width(pct(100)).height(pct(40)).to_edge(Edge::Bottom));
sea.amp().to(0.12, 1.2).await;
```

`shaders::waves()` paints a crest. `.amp()`, `.speed()`, `.deep()` and `.foam()` are signals. `.deep(Accent)` follows the theme. The node is a box and has no children.

`Shader::source(wgsl)` and `Shader::file(path)` take WGSL that defines `struct Params`, `@group(1) @binding(0) var<uniform> p: Params`, and `fn shade(i: ShadeIn) -> vec4<f32>`. oxframe adds `ox` (`time`, `frame`, `resolution`, `size`, `seed`, `view`, `progress`) and `ShadeIn` (`uv`, `local`, `world`, `screen`). `.uniform("name", 0.3)` sets a number; a color uses `.uniform_color`. A name or type that is not in `Params` is `param_unknown`. WGSL that does not compile is `shader_error`, with the line in your source. `Shader::file` is read on every draw, so saving the file updates the scene without a rebuild.

The waves preset is rasterized and then drawn by whichever backend is running, including a check with no GPU. A custom `shade` runs on the CPU the same way (about 0.13 s for a 1080p frame in a release build, about 0.3 s in the browser playground), and the playground paints it too. It cannot use textures or derivatives (`shader_error`). Read an image with `.input("photo", Input::file("photo.png"))` and `sample_photo(uv)` in WGSL (straight-alpha sRGB, `uv` 0..1, clamped). With `.input("scene", Input::frame())`, `sample_scene(i.screen / ox.resolution)` reads what was drawn before the shader, and the shader is painted over it inside its box. A node added with `.offscreen()` is drawn only for a shader: `.input("logo", Input::node(logo))` and `sample_logo(uv)`, `uv` across the node's box; `check` skips it. A shader that takes longer than the draft budget is `shader_slow`. An opaque shader painted over text is `covers_text`.

### Shaders that read pixels

```rust
let lens = view.add(shaders::lens_drops().wetness(0.0).seed(7).fullscreen());
lens.wetness().to(1.0, 0.4).await;

let rings = sea.effect(shaders::ripple().strength(0.0));
rings.strength().to(1.0, 0.4).await;

transition(Transition::shader(shaders::water_wipe().wave(0.04))).await;
```

- `shaders::lens_drops()`: drops over everything drawn before this node, inside its box. Add it last and make it `.fullscreen()`. `.wetness()` 0..1 is a signal (default 0.6), `.seed()` places the drops, `.drop_size()` scales them. `.input("scene", Input::frame())` names the input; another name is `param_unknown`.
- `node.effect(shaders::ripple())` returns an `Effect`. `.strength()` (default 1) and `.speed()` (default 6) are signals. Colors move outward from the node's center, the outline stays. One effect per node; a second call replaces it.
- `Transition::shader(shaders::water_wipe())`: a wavy waterline rises; below it is the next scene, rippled near the line. `.wave()` sets the wave height (fraction of the frame, default 0.04). Nodes the next scene adds before `transition` are under the water.
- A shader inside a group with opacity below 1, a blur, a clip, a blend, a shadow or another effect reads what that group drew before it; the group is then faded, blurred or clipped as a whole.
- All three run on the CPU, so `check` and `render --cpu` paint them; the GPU renderer gives the same picture. `inspect --graph` lists them as `effect` passes.

## Video (feature `video`, needs ffmpeg)

- `Video::new().file(path)` + `.width(px | pct())` / `.height()` (other side keeps the clip's aspect), `.radius(r)`, `.muted(true)`, `.playback_rate(r)`, `.start_at(seconds)`.
- Handle: `play()` (returns at once, plays alongside the scene), `pause()`, `seek(seconds)`, `time() -> Signal<f64>` (seconds into the clip; animate it to scrub), `playback_rate() -> Signal<f64>`, `duration()`, `is_playing()`.
- Deterministic: the frame comes from scene time. MP4 H.264, WebM VP9 and ProRes 4444 (alpha kept), GIF for a README or a pull request (720 wide, 15 fps, no sound).
- Sound joins the mix only while playing at rate 1. A file that cannot be opened is a dark box plus `video_unavailable` in `check`. The playground always shows the box.

## 3D (feature `3d`)

- `Scene3d::new()`: world; `.add(Mesh) -> Object3d`, `.light(Light) -> Light3d`, `.camera() -> Camera3d` (`position()`, `target()`, `fov()` signals), `.ambient()`, `.background()` (transparent default), `.world_point(&obj, Vec3)`.
- `Mesh::cube() | sphere() | plane() | cylinder() | group() | glb_file(path) | glb(bytes)`; builder `.position(v)`, `.rotation(deg Vec3)`, `.scale(k)`, `.scale3(v)`, `.color(paint)`, `.metallic(0..1)`, `.roughness(0..1)`, `.emissive(c)`, `.opacity(a)`.
- `Object3d` signals: `position()`, `rotation()` (degrees XYZ, past 360 = more turns), `orientation()` (Quat), `scale()`, `color()`, `metallic()`, `roughness()`, `emissive()`, `opacity()`; `.add(Mesh)` for children.
- `Light::directional(dir)`, `Light::point(pos)`, `.color()`, `.intensity()`. Default sunlight when none is added.
- `View3d::new(&scene).size(w, h)`, `.samples(1..4)` (default 2), `.resolution(k)`; handle `width()`, `height()`, `project(Vec3)`, `projector()` (use in `bind`). `Vec3::new(x, y, z)`, Y up.

## Presentations

- `slide_break().await`: stop for `oxframe present`; no effect on render/check — hold with `wait_for` before it so the video version is readable. Last frame of each scene is also a stop; a break at a scene's end merges with it.
- `notes("…")`: speaker notes for the part ending at the next stop; several calls join with newlines.
- `oxframe present` (keys: →/space/PageDown next, ←/PageUp back, Home, F full screen, N notes window). `oxframe render --slides out.pdf`: one page per stop.

## Matrices, vectors, tables

- `Matrix::new(rows)` (numbers, `&str`, anything `Display`) → handle (Clone, derefs to `Node`): `cell(r, c) -> Txt`, `row(r)`, `column(c) -> Vec<Txt>`, `rows()`, `columns()`, `brackets() -> [Path; 2]`, `row_frame(r)`, `column_frame(c)`, `cell_frame(r, c) -> Rect` (hidden; `.fade_in()` / `.opacity().to(..)`). Builder: `.font_size(px)`, `.cell_width(px)` (room for later, longer values), `.column_gap`, `.row_gap`. Change a value: `m.cell(1, 0).text().set("7")`.
- `axes.vector(x, y) -> Vector` (derefs to `Line`): `x()`, `y()` signals, `to(x, y, secs)`, `.stroke(paint)`, `.label("v") -> Txt`.
- `axes.apply_matrix([[a, b], [c, d]], secs) -> Tween` composes with the current matrix; `axes.matrix() -> Signal<Mat2>` (`Mat2::IDENTITY`, `.after(&m)`, `.det()`). Grid deforms; plots, series, vectors, `axes.place`d nodes move; ticks/labels don't.
- `Table::new(header).row(cells)…` builder: `.widths([px..])`, `.font_size(px)`, `.row_height(px)`. Handle: `row(i) -> TableRow` (`cells()`, `cell(c)`, `background()` hidden Rect, `values()`), `cell(r, c)`, `len()`, `header()`. Await: `insert_row(at, cells)`, `push_row(cells)`, `remove_row(i)`, `sort_by_column(c, secs)` (leading numbers compare numerically), `sort_by(|a, b| .., secs)`.

## Icons (feature `icons`)

- `icon!("set:name")` → `IconBuilder`; sets `lucide:` (outlines), `tabler:` (outlines; `-filled` names are fills), `simple:` (brand logos, fills). Names as on lucide.dev, tabler.io/icons, simpleicons.org. Unknown name = compile error with the closest names. Only named icons go into the binary.
- `.size(px)` (default 48, square), `.color(Ink | Color)` (default theme `Text`), `.stroke_width(w)` on the 24-unit grid (default 2).
- Handle `Icon` (Clone, derefs to `Node`): `color() -> Signal<Color>`, `stroke_width() -> Signal<f64>`, `paths() -> Vec<Path>` (each can `draw_on()`), `style()` (`IconStyle::Stroke | Fill`).
- `FlowDiagram::new().edge(a, b).icon(a, icon!(..))` puts the icon at the start of that card; `flow.icon(a) -> Option<Icon>`.
- `Icon::from_svg(bytes, IconStyle::Stroke)` for a 24×24 SVG of your own.

## Formulas (feature `math`)

- Turn it on in Cargo.toml: `oxframe = { version = "…", features = ["math"] }`. Native only: the browser playground does not have it.
- `Math::new("e^(i pi) + 1 = 0")` → `MathBuilder`; the string is Typst math syntax without the `$`: `x^2`, `x_i`, `sqrt(x)`, `(a + b) / c`, `sum_(k=1)^n`, `integral_0^1`, `alpha pi`, `vec(1, 2)`, `mat(1, 2; 3, 4)`, `"text"` in quotes. A bad formula panics when added, with Typst's message.
- `.size(px)` font size (default as `Txt`), `.color(Ink | Color)` (default theme `Text`), `.display()` own-line style (limits above/below, full-size fractions).
- Handle `Math` (Clone, derefs to `Node`): `color() -> Signal<Color>`, `paths() -> Vec<Path>` glyphs and rules in reading order, `formula()`. Glyphs are fills: write on with `stagger(0.08, m.paths(), |g| g.pop_in())` or `fade_in`, not `draw_on`.

## Nodes of your own (feature `unstable-node-api`)

- `#[derive(Node)]` on a struct that implements `Draw` (`draw(&self, ctx: &mut DrawCtx)`, optional `size(&self) -> (f64, f64)`, default 100×100).
- `#[signal]` fields: builder setter `Gauge::new().value(0.3)`, handle `GaugeHandle::value() -> Signal<f64>`. Other fields: builder setter only. All fields need `Default + Clone`.
- `DrawCtx`: `line`, `circle`, `rect`, `arc(center, radius, start_deg, sweep_deg)`, `path`; then `.fill(paint)` or `.stroke(paint, width)`. Origin is the box center, y down. `ctx.time()`, `ctx.rng(seed)`.
- The node gets x/y/rotation/scale/opacity, `to_edge`, effects, flex; `check` names it by its type. No children: compose with a `Group` instead.
- `oxframe init --template node my-node` → crate with `src/lib.rs`, `examples/demo.rs`, `tests/reference.rs`.

## Charts

```rust
let axes = view.add(Axes::new().x_range(-5.0..5.0).y_range(-2.0..2.0).width(pct(80)).height(pct(60)));
let wave = axes.plot(|x| x.sin()).stroke(Accent).width(4.0);
wave.draw_on().await;
let x = Signal::new(-5.0);
let dot = axes.point_on(&wave, x.clone());      // a Circle on the curve
x.to(5.0, 2.0).await;
wave.to_fn(|x| (2.0 * x).sin(), 1.0).await;      // every x moves to the new value
axes.x_range().to(-10.0..10.0, 1.0).await;        // Signal<Interval>; ranges animate
```

- A plot is sampled every frame (finer where it bends), breaks at jumps (`tan` has no vertical bridges) and is cut at the box. `Plot` derefs to `Path`.
- `axes.c2p(x, y)` is the scene point of data `(x, y)`; `axes.place(node, x, y)` keeps a node there while the ranges move.
- `axes.bars([3.2, 7.5])` → `Bars`: values above the bars, `.labels(["rustc", "gcc"])` under them, `bars.value(i)` is a signal. With bars the y axis is at the left edge; use x range `-0.5..n - 0.5`.
- `axes.line_series([(0.0, 1.0), (1.0, 3.0)])` → a polyline (`series.point(i)` is a signal); `axes.scatter(points)` → dots.
- `NumberLine::new(0.0..10.0)` is an `Axes` without a y axis (`place`, `c2p` work).
- `.no_grid()` hides the grid. Labels and lines take theme colors. Pie charts are not built in: draw slices with `Custom`.

## Transitions

At the start of any scene except the first:

```rust
transition(Transition::slide(Dir::Left)).await;          // theme duration and enter easing
transition(Transition::fade().duration(0.6).ease(presets::LINEAR)).await;
transition(Transition::wipe(Dir::Right).overlap()).await; // previous scene keeps playing
transition(Transition::zoom_in()).await;
transition(Transition::zoom_out()).await;
transition(Transition::custom(|t| Blend {                 // t is eased progress, 0..=1
    from: LayerPose::new().opacity(1.0 - t),
    to: LayerPose::new().opacity(t).offset(1.0 - t, 0.0),
    incoming_on_top: true,
    mix: false,
})).await;
```

`Dir` is `Left`, `Right`, `Up`, `Down`. The call waits, so nodes added before it show during the blend and nodes added after show when it ends. The first scene ignores it. Length is stored in `meta.json` under `transition.seconds` and the timeline can drag it. `oxframe render --scene name` draws that scene without the blend into it. `oxframe check` puts a blend frame on the storyboard. `oxframe inspect` adds `"transition": { "kind", "dir", "progress", "overlap" }` while one is running.

## Nodes (builder → `view.add(..)` / `parent.add(..)` → Copy handle)

| Node | Builder | Handle signals |
|---|---|---|
| `Rect` | `.width .height .size .radius` | `width height radius` |
| `Circle` | `.size` (diameter) `.start_angle .end_angle` (deg, from 12 o'clock) `.pie(bool)` | `size start_angle end_angle` |
| `Polygon` | `.sides .size` | `sides size` |
| `Star` | `.points .size .inner` (0..1) | `points size inner` |
| `Path` | `.data(BezPath) .svg("M0 0 L10 0 Z")` | `data` (morphs between any paths) |
| `Line` | `.points([(x, y), ..]) .smooth .closed .start_arrow .end_arrow .arrow_size` | `points arrow_size` |
| `Txt` | `.text .rich([span("a").bold().italic().color(c)]) .font_size .weight .family(SANS/MONO) .align .max_width` | `text spans font_size weight`; `.morph_to("new", secs)` |
| `Code` | `.code(src) .language("rust"/"js"/"python"/"json"/"toml"/"shell") .theme(CodeTheme::dark()) .focus(first, last) .dim(0.3)` | `font_size focus dim`; `.code().to(next, secs)` diffs tokens (not a typewriter); `.insert(line, col, text, secs)`, `.remove(((l, c), (l, c)), secs)`, `.replace(range, text, secs)`, `.flash()` on that tween; `.focus_lines(a, b, secs)` — lines are 1-based, columns 0-based |
| `Img` | `.src(include_bytes!("../assets/a.png")) .file("assets/a.png") .width .height` (PNG/JPEG/SVG) | `width height` |
| `Svg` | `.src(include_bytes!("../assets/logo.svg")) .file("assets/logo.svg") .width .height` (aspect kept if only one is set) | `width height`; `.find("id")` the element with that id; `.paths()` every contour in paint order. Filters, masks and clip paths are skipped with a warning |
| `Layout` | `.row() .column() .wrap(bool) .grid(cols) .gap .padding .align(LayoutAlign::..) .justify(Justify::..) .width .height .radius` | `gap padding width height radius` |
| `Grid` | `.width .height .spacing` | `width height spacing` |
| `Group` | — | — |

## Theme

`Project::new("name").theme(Theme::dark().accent(color!("#68abdf")))` — also `Theme::light()`, `midnight`, `paper`, `vivid`. Colors: `Background Surface Text Muted Accent OnAccent Success Warning Danger` (`.fill(Accent)`, `.stroke(Muted)`). Type: `.display() .h1() .h2() .body() .caption() .code()`. Space and radius: `Xs Sm Md Lg Xl` (`.padding(Md)`, `.gap(Sm)`, `.margin(Lg)`, `.radius(Lg)`). Shadow: `.elevation(Md)`. Motion: `.to(v, Normal)` (`Fast`/`Slow` too); a tween takes tokens directly, `.fill().to(Accent, Normal)`. `node.fade_in()` / `fade_out()` use the theme's normal duration. `[theme]` in `oxframe.toml` (`name`, `accent`) overrides the compiled theme. Text with no fill uses `Text`.

Every node: `.name("title")` (readable in check/inspect; otherwise the `add` call site), `x y rotation scale opacity fill stroke line_width start end`
(`start/end` trim outlines 0..1), `fill_gradient(Gradient) stroke_gradient dash(vec) dash_offset
line_cap line_join blur shadow(color, blur, (dx, dy)) glow(color, blur) clip(bool) blend(BlendMode)
z_index anchor((ax, ay))`, and inside a `Layout`: `grow shrink basis margin align_self`.
Builders take values; handles return `Signal`s of the same names. Children: `.child(builder)` on a
builder, or `handle.add(builder)` for a handle. `node.remove()`, `node.reparent(other)`.

## Blocks (`oxframe add`)

Ready-made pieces copied into the project, each checked to pass `oxframe check`. Each is `pub async fn` that builds its nodes in a `Group`, plays them in and returns the group (fade it out or move it after). Prefer a block over drawing the same thing by hand; edit the copied file freely.

- `blocks::logo_reveal(view, name, tagline)` — mark and name centered as a pair, tagline under.
- `blocks::stat_counter(view, value, label)` — a big number counting up, label under.
- `blocks::progress_bar(view, label, fraction)` — bar fills to `fraction` (0–1) with the percentage counting.
- `blocks::quote_card(view, quote, author)` — a card with the quote and the author.
- `blocks::before_after(view, before, after)` — two cards, "After" slides in.
- `blocks::timeline(view, &[(date, text), ..])` — a line, then each date and text in turn.
- `blocks::release_notes(view, version, &[change, ..])` — the version, then each change with a mark.
- `blocks::terminal(view, command, &[output, ..])` — a terminal window: the command is typed, then the output lines appear.
- `blocks::code_change(view, language, before, after)` — code that diffs into its new version.
- `blocks::bar_chart(view, title, &[(label, value), ..])` — axes and a title, bars grow in turn.
- `blocks::kinetic_title(view, text)` — the words pop in one after another.
- `blocks::call_to_action(view, headline, link)` — a closing line and a link in a pill that pulses.
- `blocks::feature_grid(view, &[(title, text), ..])` — up to four cards in a row slide up in turn.
- `blocks::checklist(view, title, &[item, ..])` — open boxes, then each item is ticked.
- `blocks::comparison(view, ours, theirs, &[(feature, ours_has, theirs_has), ..])` — a table, a row at a time, "Yes" green and "No" red.
- `blocks::chat_messages(view, &[(mine, text), ..])` — bubbles pop in, yours on the right in the accent color.
- `blocks::pull_request(view, number, title, added, removed)` — a PR card, the line counts count up, then "Merged".
- `blocks::countdown(view, from, done)` — a number per second in a filling ring, then `done`.

`pop_in()` grows a node to its scale at the time of the call: add the node at full scale and call `pop_in()` right away; a node added with `.scale(0.0)` stays invisible.

## Components

`view.add(TitleCard::new("Binary search").subtitle("O(log n)"))`, `ChapterMarker::new("1  Search")`, `BulletList::new(["Take the middle", "Compare"])`, `CodeExplainer::new(src).step(1, 1, "the middle")`, `FlowDiagram::new().edge("Client", "API").down()`, `Callout::new(node).text("here")`, `LowerThird::new("Ada").role("walks the half")`, `EndCard::new("Done").note("thanks")`. All of them: `enter().await`, `next().await`, `exit().await`. They read the theme and the safe area.

## Camera

Zoom, pan and turn the view of everything added to the camera; nodes added
to `view` stay on screen (titles, captions).

```rust
let cam = view.add(Camera::new());
let diagram = cam.add(Group::new());            // build the world inside it
cam.fit(diagram, 120.0, 0.0).await;             // show all of it (padding px, seconds)
cam.center_on(node, 1.0).await;                 // bring a node to the middle
cam.zoom().to(2.5, 1.0).await;                  // x(), y() (point looked at), zoom(), rotation()
cam.x().bind(move || ball.x().get());           // follow a moving node
cam.reset(1.2).await;                           // back to x = y = 0, zoom 1, rotation 0
```

`oxframe check` does not report `out_of_frame` for nodes inside a camera.

## Position

`node.next_to(other, Side::Right)` (also `Left`, `Top`, `Bottom`) sits outside the neighbor with the theme's `Md` gap, and follows it every frame. `align_to(other, Edge::Top)`, `to_edge(Edge::Bottom)` (pins to the safe area, not the frame edge), `center_in(other)`. The same methods exist on builders: `.h1().to_edge(Edge::Top)`. `.width(pct(80))` / `.height(pct(50))` is a fraction of the frame, or of the parent when the parent has a fixed size. `view.safe_area()` is the scene-space rectangle; `view.is_portrait()` is true when the frame is taller than wide. `Project::aspects([Aspect::Landscape, Aspect::Portrait, Aspect::Square])` — `oxframe check` lints each one, `oxframe render --aspect 9:16` or `--aspect all` exports them.

## Animation

```rust
s.x().to(300.0, 1.0).await;                                  // seconds
s.x().to(300.0, 1.0).ease(presets::OUT_BACK).await;           // presets::*, oxframe::bounce_out, oxframe::elastic_out, |t| t * t
s.fill().to(color!("#68abdf"), 0.5).await;                   // colors blend in OKLab
s.scale().to(2.0, 0.5).await;                                // one number, or (x, y)
all((a.x().to(1.0, 1.0), b.y().to(2.0, 0.5))).await;          // together (tuple ≤ 12)
all(cards.iter().map(|c| c.fade_in())).await;                 // together, any count
stagger(0.1, &cards, |c| c.pop_in()).await;                   // staggered starts
stagger(0.1, &cards, |c| async move { c.pop_in().await; c.fade_out().await; }).await;
let first = any((a.x().to(1.0, 2.0), wait_for(0.5))).await;   // first to finish (index)
repeat(3, || dot.scale().to(1.2, 0.3)).await;
spawn(loop_while(|| true, move || dot.rotation().to(dot.rotation().get() + 360.0, 2.0)));  // background loop
wait_for(0.5).await;  next_frame().await;
let h = spawn(async move { /* … */ }); h.await;              // background task
```

Helpers on every node, timed by the theme (`Fast`/`Normal`/`Slow`), with
`.duration(s)` to override:

```rust
title.fade_in().await;  title.fade_out().duration(0.3).await;
badge.pop_in().await;                     // scale from 0 with overshoot
card.slide_in(Side::Left).await;          // slides in from the left, fading in
path.draw_on().await;                     // outline drawn start → end
```

## Signals

```rust
let x = s.x();          // Signal<f64>, cheap to clone (shared)
x.get(); x.set(10.0);
t.text().bind(move || format!("{:.0}", x.get()));   // derived, recomputed lazily
let free = Signal::new(0.0f64);                     // your own signal, animatable with .to()
```

## Editor-tunable values (stored in `<scene>.meta.json`)

```rust
wait_until("beat").await;                // timeline marker (first run records the current time)
let n = tune!("count", 7.0);             // slider, f64
x.to(1.0, 1.0).ease(ease!("slide"));     // curve editor; ease!("slide", presets::OUT_CUBIC) for a default
```

## Randomness

`random()` in `[0, 1)`, `random_range(a, b)`; seeded by `Project::seed`. Same output every run.

## Rendering

- `oxframe render` runs 2-second segments in parallel (`--jobs n`, default cores ≤ 8); output bytes do not depend on n.

`oxframe check [--frames] [--against <dir>]` — storyboard and `report.json` in `out/check/`. `--frames` also writes full-size PNGs. `--against out/before` (after `oxframe check --frames --out out/before`) lists the frames an edit changed, with `diff/` pictures, so you can confirm it changed only what you meant. Each issue on a node gets `bounds` and a `crop`: `out/check/issues/<n>-<rule>.png`, that part of the frame at full resolution with the node outlined. Most issues also have `fix`: `{"action": "move", "dx": -140, "dy": 0}` (pixels, y down), `shrink` (`width`, `height`), `color` (`#rrggbb` and the `contrast` it reaches), `widen` (`by` px or `font_scale`), `hold` or `slow` (`seconds`). Thresholds live in `oxframe.toml` `[check]`. Errors exit 1.
`oxframe inspect --at <seconds> [--scene s] [--graph]` — JSON scene tree at that time. `--graph` adds the render passes.
`oxframe render [--out out.mp4|.webm|.mov|.gif|dir] [--format mp4|webm|prores|png|gif] [--scene s] [--range 1..3] [--size 1280x720] [--fps 15] [--cpu]`
`oxframe snapshot --at <seconds> [--at …] [--scene s] [--size WxH] [--out file.png]`
