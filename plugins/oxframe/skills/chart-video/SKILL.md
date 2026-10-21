---
name: chart-video
description: "Animate data as a video: bars that grow, line series, a dot riding a curve, a sorted table, axes that zoom. Use for data stories, benchmark results, bar chart races or turning numbers into a short clip."
---
# Charts and data

Uses the `oxframe` skill (workflow, full API cheat sheet); install both with `npx skills add oxygenframe/oxframe-sdk`.

```rust
#[oxframe::scene]
async fn benchmark(view: View) {
    view.add(Txt::new().text("Frame time, ms").h2().to_edge(Edge::Top));
    let axes = view.add(Axes::new().x_range(-0.5..3.5).y_range(0.0..20.0));
    let bars = axes.bars([0.0, 0.0, 0.0, 0.0]).labels(["vello", "skia", "cairo", "wgpu"]);
    for (i, v) in [4.0, 12.0, 18.0, 3.0].into_iter().enumerate() {
        bars.value(i).to(v, 0.8).await;
    }
    wait_for(1.5).await;
}
```

- Bars: set the x range to `-0.5..n - 0.5`; `bars.value(i)` is a signal,
  `bars.bar(i)` the rectangle (color it to highlight).
- Lines: `axes.line_series([(x, y), …])` (each point is a signal), dots:
  `axes.scatter([…])`. Zoom with `axes.x_range().to(a..b, secs)`.
- Rankings: `Table::new(["Name", "Time"]).row([..])`, then `insert_row`,
  `sort_by_column(c, secs)` and `row(i).background().fade_in()` for the winner.
- There is no pie chart: draw slices with `Custom` and `ctx.arc`.
- Numbers on screen must come from the data the user gave; do not invent values.
