---
name: oxframe-check
description: "Read and fix `oxframe check` reports: storyboard.png and report.json with named rules (out_of_frame, text_overflow, too_fast, low_contrast, dead_time…). Use after any change to an oxframe scene, or when check exits 1."
---
# Fixing `oxframe check`

Uses the `oxframe` skill (workflow, full API cheat sheet); install both with `npx skills add oxygenframe/oxframe-sdk`.

`oxframe check` type-checks the project, runs every scene and writes
`out/check/storyboard.png` and `out/check/report.json`. It exits 1 on errors;
warnings do not fail it. `oxframe --json check` prints the report on stdout.

Each issue has `rule`, `severity`, a time range, a node, `file:line` and (from
oxframe 0.20.1) `docs`, a link to the rule's section of
https://oxframe.dev/docs/check-rules with a before and after picture. An issue
tied to one node also has `bounds`, its pixel rect `[x0, y0, x1, y1]` at the
start of the range, and (from oxframe 0.21) `crop`, a full-resolution PNG of
that part of the frame with the node outlined in red, relative to
`out/check/` (for example `issues/01-out_of_frame.png`). Open the crop to see
the problem up close instead of finding it on the storyboard. Fix
errors first, then warnings, then run `check` again until it exits 0. Look at the
storyboard before reporting: a clean report is necessary, not sufficient.

| Rule | What it means | Usual fix |
| --- | --- | --- |
| `out_of_frame` | A node leaves the frame or the safe area | Smaller size, `pct()`, `to_edge(Edge::…)`, or move the camera |
| `text_overflow` | Text wider than its box or the frame | Shorter text, `.caption()`/`.body()` instead of a heading, a wider box |
| `text_overlap` | Text on top of text | `next_to`, a `Layout`, or fade one out first |
| `covers_text` | A shader drawn over text | Add the shader before the text, or move the text out of its box |
| `low_contrast` | Text too close to its background | A theme role: `.fill(Text)` on `Surface`, `OnAccent` on `Accent` |
| `too_fast` | Text not on screen long enough to read, or a large move too quick | `wait_for(…)` after it appears, a longer tween |
| `dead_time` | Nothing changes for too long | Shorten the wait or animate something |
| `voice_cut` / `voice_overlap` | A spoken line runs past the scene, or two lines overlap | `line.await` before the scene ends; start the next line after |
| `shader_error`, `param_unknown`, `shader_slow`, `input_cycle` | Shader problems | See the Shaders section of the cheat sheet |
| `nondeterministic` | Two paints of one frame differ | Draw only from signals and `ctx.time()`; randomness from `ctx.rng(seed)` |
| `video_unavailable` | A clip cannot be opened | Check the path and the `video` feature; install ffmpeg |

`oxframe inspect --at <seconds>` prints the node tree (kind, name, bounds,
text) when the picture is unclear; `--graph` adds the render passes.
`oxframe snapshot --at <seconds>` writes one full-size frame. Thresholds live in
`oxframe.toml` under `[check]`; change them only when the user asks.
