---
name: pr-to-video
description: "Animate a pull request or a code change as a short video: the old code morphs into the new one token by token, with the changed lines in focus. Use for PR summaries, code review videos, or 'show what changed' requests."
---
# A code change as a video

Uses the `oxframe` skill (workflow, full API cheat sheet); install both with `npx skills add oxygenframe/oxframe-sdk`.

Get the change with `git diff` or `gh pr diff <n>`. Show each changed file
(or the one that matters) as a `Code` node, then animate it to the new
version: `.code().to(..)` diffs tokens, so kept code stays and changed code
moves, appears or fades.

```rust
const BEFORE: &str = "fn total(items: &[Item]) -> u32 {\n    items.iter().map(|i| i.price).sum()\n}";
const AFTER: &str = "fn total(items: &[Item]) -> u32 {\n    items.iter().filter(|i| i.in_stock).map(|i| i.price).sum()\n}";

#[oxframe::scene]
async fn change(view: View) {
    view.add(Txt::new().text("Skip items out of stock").h2().to_edge(Edge::Top));
    let code = view.add(Code::new().code(BEFORE).language("rust"));
    wait_for(1.0).await;
    code.focus_lines(2, 2, 0.4).await;
    code.code().to(AFTER, 1.2).flash().await;
    wait_for(1.5).await;
}
```

- `.insert(line, col, text, secs)`, `.remove(((l, c), (l, c)), secs)` and
  `.replace(range, text, secs)` edit in place; lines are 1-based, columns 0-based.
- One file per scene, about 20 lines on screen; cut the rest from the
  snippet. Long files: `.focus(first, last)` and `.dim(0.3)`.
- Use `CodeExplainer::new(src).step(first, last, "what it does")` when the
  video should walk through the new code instead of the change.
