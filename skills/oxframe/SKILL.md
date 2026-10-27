---
name: oxframe
description: "Make videos from code with oxframe: explainers, code walkthroughs, charts, matrices, 3D, slides and release videos as Rust scenes, checked with `oxframe check` (storyboard and report) and rendered to MP4 without a browser. Use when the user asks for an animation, a motion graphic, a video from code, or changes to an oxframe project."
---
<!-- oxframe-agents 0.24.1 -->
# oxframe scenes

The project is on oxframe 0.24.2: docs https://oxframe.dev/docs/0.24,
model text https://oxframe.dev/docs/0.24/llms.txt
(and https://oxframe.dev/docs/0.24/llms-full.txt),
API https://docs.rs/oxframe/0.24.2/oxframe/.
`oxframe mcp` exposes check, notes, note_take, note_done, storyboard, inspect, render_frame, render_graph, list_components, docs_search and render. `oxframe --json check` prints one JSON object.

## No project yet

When the user wants a video and there is no oxframe project (no `oxframe.toml`):

1. Install the CLI: `curl -fsSL https://github.com/oxygenframe/oxframe-cli/releases/latest/download/install.sh | sh`
   (or `cargo install oxframe-cli`). Rendering needs ffmpeg.
2. `oxframe init my-video` creates the project: `src/main.rs` with one scene,
   `oxframe.toml`, and this skill with project notes in `AGENTS.md`.
3. `cd my-video`, write the scenes, then follow the workflow below. `oxframe render`
   writes `out/my-video.mp4`.

Rust is needed on the machine, but the user does not need to know it: the
scenes are short async functions, and `oxframe check` tells you what is wrong.

## Workflow

1. Read `src/main.rs` and the scene's `src/<scene>.meta.json` (markers/tunables already tuned by a human — keep them).
2. Make the change. Prefer small, readable steps: one `.await` per beat of the story, `all((..))` for things that happen together.
3. `oxframe check`. Fix compile errors first (see AGENTS.md, "Common compile errors"). Then open `out/check/storyboard.png` and `out/check/report.json`. Each issue names a rule, a time range, a node and `file:line`. An issue on a node also has `crop`, a close-up PNG under `out/check/issues/` with the node outlined: open it. Most issues carry `fix`, the change with the numbers worked out (`move` by `dx`/`dy` px, `shrink`, a text `color`, `widen` by px or `font_scale`, `hold` or `slow` in seconds): start from it, and keep the design in mind (moving a title 140 px may call for moving its neighbours too).
   Issues with rule `review_note` come first: a person marked something in the `oxframe dev` editor and wrote what to change (`note_text`, with the time, the node, `file:line` and a `crop`). Take it first (`oxframe notes take <id>`, MCP `note_take`: the editor shows the person that you are on it), do what the note asks, even when nothing else is wrong there, then close it: `oxframe notes done <id> --reply "what you changed"` (MCP: `note_done`). The reply shows under the note in the editor. If a note is unclear or asks for something you cannot do, ask in the reply of `take` and leave it open. While the person reviews in `oxframe dev`, keep `oxframe notes wait` running in the background: it returns with the next new note.
4. Fix and run `oxframe check` again until it exits 0. Warnings do not fail the command; errors do.
5. `oxframe inspect --at <t>` when a frame looks wrong and you need the node tree (kind, name, bounds, text). Add `--graph` to see render passes. `oxframe snapshot --at <t>` for one still (960×540; `--size` for another).
6. Only then report. Mention the storyboard and which issues you fixed.

Never run `oxframe dev` (interactive server). Never hand-edit `meta.json` unless asked.

When the user asks for a link to the video, `oxframe publish --title "…"` renders it and prints a public oxframe.dev link (30 days, up to 30 s: `--range 0..30`). Never publish unasked: anyone with the link can watch.

## API

The full cheat sheet is in [reference.md](reference.md): nodes and their
properties, flow functions, easing, signals, text, layout, effects, meta helpers.

## Recipes

Reach for the right node before writing the scene:

- **Explaining code** (what an expression does, why a line matters, a bug fix):
  a `Code` node is the tool, not plain `Txt`. Show the source, then walk
  attention through it with `.focus(first, last)` or `.focus_lines(a, b, secs)`
  and `.dim(0.3)` on the rest; to explain parts of one expression, focus each
  column range in turn. Morph to the next version with `.code().to(next, secs)`,
  or use `.insert`/`.remove`/`.replace`. For a line-by-line walkthrough with a
  note per step, `CodeExplainer::new(src).step(line, line, "note")`.
- **Vertical (9:16) or square short:** `Project::new(..).size(1080, 1920)` for
  portrait, `(1080, 1080)` for square — or `Project::aspects([Aspect::Portrait])`
  and `oxframe render --aspect 9:16`. `view.is_portrait()` adapts the layout.
- **Charts and math:** `Axes`/`NumberLine` with `.plot(..)`, `bars`, `scatter`;
  `Matrix`, `axes.vector(..)`. A title, bullets or an end card: the components
  (`TitleCard`, `BulletList`, `EndCard`), not hand-placed `Txt`.
