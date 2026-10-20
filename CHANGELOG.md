# Changelog

All crates of the workspace (`oxframe`, `oxframe-core`, `oxframe-2d`, …) and
`oxframe-cli` are released together under one version. Before 1.0 a breaking
change bumps the minor version (0.1 → 0.2) and a fix bumps the patch
(0.1.0 → 0.1.1). Breaking changes are listed separately with a
"before → after" example.

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
