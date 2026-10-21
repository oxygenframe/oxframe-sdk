---
name: code-walkthrough
description: "Explain a piece of code step by step in a video: highlight lines, show a note for each step, then morph the code into its next version. Use for tutorials, onboarding videos, or explaining an algorithm's code."
---
# Code walkthrough

Uses the `oxframe` skill (workflow, full API cheat sheet); install both with `npx skills add oxygenframe/oxframe-sdk`.

```rust
const SRC: &str = "fn search(xs: &[i32], x: i32) -> Option<usize> {\n    let (mut lo, mut hi) = (0, xs.len());\n    while lo < hi {\n        let mid = (lo + hi) / 2;\n        if xs[mid] < x { lo = mid + 1 } else { hi = mid }\n    }\n    (lo < xs.len() && xs[lo] == x).then_some(lo)\n}";

#[oxframe::scene]
async fn walkthrough(view: View) {
    let card = view.add(TitleCard::new("Binary search").subtitle("in eight lines"));
    card.enter().await;
    card.next().await;
    wait_for(1.0).await;
    card.exit().await;
    let explain = view.add(
        CodeExplainer::new(SRC)
            .step(2, 2, "Search the whole slice")
            .step(4, 4, "Look at the middle")
            .step(5, 5, "Drop the half that cannot hold x")
            .step(7, 7, "Found, or not"),
    );
    explain.enter().await;
    for _ in 0..4 {
        explain.next().await;
        wait_for(2.0).await;
    }
}
```

- One idea per step, a short note (under 8 words), 2 s or more per step so
  `check` does not report `too_fast`.
- For a free layout use a `Code` node with `.focus_lines(a, b, secs)` and a
  `Callout::new(node).text("…")`, or morph versions with `.code().to(next, secs)`.
- Pair it with a diagram (`FlowDiagram`) or a chart when the code does
  something visual.
