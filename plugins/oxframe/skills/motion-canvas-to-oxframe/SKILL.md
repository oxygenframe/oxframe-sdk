---
name: motion-canvas-to-oxframe
description: "Port a Motion Canvas project (TypeScript generators) to oxframe (Rust async): makeScene2D, yield*, all/chain, refs, signals, waitUntil and transitions mapped one to one. Use when migrating from Motion Canvas or Revideo."
---
# From Motion Canvas to oxframe

Uses the `oxframe` skill (workflow, full API cheat sheet); install both with `npx skills add oxygenframe/oxframe-sdk`.

The model is the same: a scene is code that runs over time, nodes have signal
properties, and the editor retimes marked moments. Generators become `async`.

| Motion Canvas | oxframe |
| --- | --- |
| `makeScene2D(function* (view) { … })` | `#[oxframe::scene] async fn name(view: View) { … }` |
| `view.add(<Rect ref={r} width={200} fill="#e13238" />)` | `let r = view.add(Rect::new().width(200.0).fill(color!("#e13238")));` |
| `yield* r().x(300, 1)` | `r.x().to(300.0, 1.0).await` |
| `yield* all(a, b)` | `all((a, b)).await` |
| `yield* chain(a, b)` / `sequence(0.1, …)` | `a.await; b.await;` / `stagger(0.1, items, |x| …)` |
| `yield* waitFor(1)` | `wait_for(1.0).await` |
| `yield* waitUntil('event')` | `wait_until("event").await` (a marker in `meta.json`) |
| `createSignal(0)` | `Signal::new(0.0)`; derived: `Signal::derived(|| …)` |
| `yield r().x(300, 1)` in parallel | `spawn(async move { … })` |
| `<Layout direction="column" gap={20}>` | `Layout::new().direction(Direction::Column).gap(20.0)` |
| `<Txt text="Hi" fontSize={80} />` | `Txt::new().text("Hi").font_size(80.0)` |
| `<Code code={…} />`, `code().code(next, 1)` | `Code::new().code(src)`, `code.code().to(next, 1.0)` |
| `slideTransition(Direction.Left)` | `transition(Transition::slide(Dir::Left)).await` |
| `easeInOutCubic` | `presets::IN_OUT_CUBIC` or `ease!("name")` (editable) |
| `npm run serve` / render | `oxframe dev` / `oxframe render` (server-side, no browser) |

Port scene by scene: keep the structure of `yield*` lines as `.await` lines,
convert refs to the handles `view.add` returns, then `oxframe check`.
