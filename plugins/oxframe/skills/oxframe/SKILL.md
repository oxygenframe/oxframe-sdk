---
name: oxframe
description: "Make videos from code with oxframe: explainers, code walkthroughs, charts, matrices, 3D, slides and release videos as Rust scenes, checked with `oxframe check` (storyboard and report) and rendered to MP4 without a browser. Use when the user asks for an animation, a motion graphic, a video from code, or changes to an oxframe project."
---
<!-- oxframe-agents 0.20.2 -->
# oxframe scenes

The project is on oxframe 0.20.2: docs https://oxframe.dev/docs/0.20,
model text https://oxframe.dev/docs/0.20/llms.txt
(and https://oxframe.dev/docs/0.20/llms-full.txt),
API https://docs.rs/oxframe/0.20.2/oxframe/.
`oxframe mcp` exposes check, inspect, render_graph, render_frame and docs_search. `oxframe --json check` prints one JSON object.

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
3. `oxframe check`. Fix compile errors first (see AGENTS.md, "Common compile errors"). Then open `out/check/storyboard.png` and `out/check/report.json`. Each issue names a rule, a time range, a node and `file:line`.
4. Fix and run `oxframe check` again until it exits 0. Warnings do not fail the command; errors do.
5. `oxframe inspect --at <t>` when a frame looks wrong and you need the node tree (kind, name, bounds, text). Add `--graph` to see render passes. `oxframe snapshot --at <t>` for one full-size still.
6. Only then report. Mention the storyboard and which issues you fixed.

Never run `oxframe dev` (interactive server). Never hand-edit `meta.json` unless asked.

## API

The full cheat sheet is in [reference.md](reference.md): nodes and their
properties, flow functions, easing, signals, text, layout, effects, meta helpers.
