---
name: remotion-to-oxframe
description: "Port a Remotion (React) video to oxframe (Rust): compositions, useCurrentFrame/interpolate, Sequence, spring, Img/Video/Audio and rendering mapped to oxframe scenes and signals. Use when migrating from Remotion or comparing the two."
---
# From Remotion to oxframe

Uses the `oxframe` skill (workflow, full API cheat sheet); install both with `npx skills add oxygenframe/oxframe-sdk`.

Remotion computes every frame from `useCurrentFrame()`. oxframe runs a scene
as an `async fn` where each `.await` takes time, and values are signals that
`.to(target, seconds)` animates. Port the timeline, not the per-frame math.

| Remotion | oxframe |
| --- | --- |
| `<Composition durationInFrames fps width height>` | `Project::new("name").fps(60).size(1920, 1080).scene(main())`; length comes from the scene |
| `useCurrentFrame()` + `interpolate(frame, [0, 30], [0, 1])` | `node.opacity().to(1.0, 0.5).await` |
| `spring()` | an ease: `.to(v, 0.6).ease(presets::OUT_BACK)` |
| `<Sequence from={30}>` | `wait_for(0.5).await` before it, `spawn(async { … })` to run alongside, or another scene |
| `<AbsoluteFill>` | the `View`; `Layout` for flex rows and columns |
| `<Img>`, `<Video>`, `<Audio>` | `Img`, `Video` (feature `video`), `sound("x.wav").play()` / `Audio::track(..)` |
| `staticFile("logo.svg")` | `Svg::new().file("assets/logo.svg")` (or `include_bytes!`) |
| CSS / Tailwind styling | the theme: `Accent`, `Surface`, `.h1()`, `.radius(Lg)` |
| `<TransitionSeries>` | `transition(Transition::fade()).await` at the start of the next scene |
| `npx remotion render` | `oxframe render` (no browser; `--jobs` for parallel segments) |
| Remotion Lambda | CI on a plain runner; `--cpu` needs no GPU |

Steps: list the compositions and their sequences, write one oxframe scene per
composition (or per major sequence), port timings in seconds (frames / fps),
replace styled boxes with components (`TitleCard`, `BulletList`, `LowerThird`,
`EndCard`) where they fit, then `oxframe check` and compare frames with
`oxframe snapshot --at <s>`.

License note for the user: oxframe is MIT/Apache-2.0 with no seat or render fees.
