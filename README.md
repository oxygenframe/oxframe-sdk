# oxframe-sdk

[![crates.io](https://img.shields.io/crates/v/oxframe.svg)](https://crates.io/crates/oxframe)
[![docs.rs](https://img.shields.io/docsrs/oxframe)](https://docs.rs/oxframe)
[![license](https://img.shields.io/crates/l/oxframe.svg)](https://github.com/oxygenframe/oxframe-sdk#license)
[![playground](https://img.shields.io/badge/try-playground-e13238)](https://oxframe.dev/playground)

[![The oxframe showreel: functions on axes, a lit 3D cube, a linear map and a shader sea, every frame drawn by oxframe](https://oxframe.dev/media/0.20.0/gallery/showreel.jpg)](https://oxframe.dev/)

*The reel on [oxframe.dev](https://oxframe.dev), made by [`examples/src/showreel.rs`](examples/src/showreel.rs).*

> **Status:** oxframe is under very active development. The API is not stable yet and may change from version to version. Pin the exact version in your Cargo.toml and check the [changelog](https://github.com/oxygenframe/oxframe-sdk/blob/main/CHANGELOG.md) before upgrading.

The Rust SDK of [oxframe](https://oxframe.dev).

Procedural animation in Rust, modelled after Motion Canvas: a scene is an
`async fn`, and every `.await` on an animation is a frame boundary.

```rust
use oxframe::prelude::*;

#[oxframe::scene]
async fn intro(view: View) {
    let square = view.add(Rect::new().size(200.0).x(-300.0).fill(color!("#e13238")));
    all((
        square.x().to(300.0, 1.0),
        square.radius().to(100.0, 1.0),
    ))
    .await;
}

fn main() {
    Project::new("intro").scene(intro()).main();
}
```

```sh
cargo run --release -p oxframe-examples --bin intro -- render --out out/intro
```

## With an AI agent

oxframe is made for agents that write the video and check their own
frames (`oxframe check`).

```sh
npx skills add oxygenframe/oxframe-sdk     # Cursor, Copilot, Codex, Claude Code and others
```

In Claude Code, the plugin adds the skill and the `oxframe mcp` server:

```text
/plugin marketplace add oxygenframe/oxframe-sdk
/plugin install oxframe@oxframe
```

The same plugin works in Cursor (`.cursor-plugin/`): the skills and the
`oxframe mcp` server. Until it is listed in the Cursor Marketplace, load it
locally:

```sh
git clone https://github.com/oxygenframe/oxframe-sdk
cp -r oxframe-sdk/plugins/oxframe ~/.cursor/plugins/local/oxframe   # then reload Cursor
```

In Codex:

```sh
codex plugin marketplace add oxygenframe/oxframe-sdk
codex plugin add oxframe@oxframe
```

All of them need the CLI: `cargo install oxframe-cli` (or the
[install script](https://github.com/oxygenframe/oxframe-cli)). Every release
gets a video of its notes from
[release-video-action](https://github.com/oxygenframe/release-video-action),
made with oxframe.

## Crates

| Crate | Purpose | wasm32 |
| --- | --- | --- |
| `oxframe-core` | signals, tweens, `Ease`, `Lerp` (incl. path morphing, gradients), deterministic executor, `all` / `sequence` / `spawn` / `wait_for` / `wait_until`, seeded random | yes |
| `oxframe-2d` | scene graph: `Rect`, `Circle` (arcs, pies), `Polygon`, `Star`, `Path`, `Line` (smooth, arrows), `Txt` (rich spans, glyph morph), `Code` (highlighting, focus), `Img` (PNG/JPEG/SVG), `Layout` (flexbox/grid via taffy), `Grid`, `Group`; gradients, dashes, clip, blend modes, blur, shadow, z-index, anchor | yes |
| `oxframe-text` | text layout (parley) and glyph outlines (skrifa), embedded Inter and JetBrains Mono, syntax highlighter | yes |
| `oxframe-render` | `Renderer` trait; tiny-skia CPU backend; vello GPU backend (native and WebGPU into an OffscreenCanvas) | yes |
| `oxframe-export` | ffmpeg pipe (mp4, webm, ProRes 4444, PNG), audio muxing, waveform | no |
| `oxframe-preview` | protocol session; native WebSocket server | session only |
| `oxframe-protocol` | `Command` / `Event`, `meta.json` schema, TS types via `ts-rs` | yes |
| `oxframe-macros` | `#[oxframe::scene]`, `color!`, `ease!`, `tune!` | — |
| `oxframe` | facade: `prelude`, `Project`, `Engine`, project CLI (`render`, `preview`, `info`), browser API (`wasm` feature) | yes |


## Examples

`cargo run --release -p oxframe-examples --bin <name> -- render|preview|info`
with `intro`, `text`, `morph`, `layout`, `chart`, `particles`, `showcase`,
`effects`, `shapes`, `typography`, `benchmark`.

## Guarantees

- **Deterministic.** The same code and `meta.json` give byte-identical frames:
  fixed `dt = 1/fps`, single-threaded executor, no wall clock, ordered maps only.
- **Exact timing.** A tween that ends between frames hands its exact end time
  to the next one, so chains of non-frame-aligned durations do not drift.
- **`meta.json` is the source of truth** for markers (`wait_until`), named
  eases (`ease!`) and tunables (`tune!`); code only supplies defaults, which are
  recorded on first run. Unknown keys are preserved; output is sorted and
  2-space indented.

## Development

```sh
cargo test --workspace
cargo test --release -p oxframe-examples --test acceptance   # M0 acceptance, seek budget
cargo bench -p oxframe-examples --bench seek
cargo test -p oxframe-protocol --features ts                 # writes crates/oxframe-protocol/bindings
cargo test -p oxframe-render --features gpu --test golden    # CPU and GPU against golden frames
OXFRAME_UPDATE_GOLDEN=1 cargo test -p oxframe-render --test golden   # regenerate references (CPU)
```

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
