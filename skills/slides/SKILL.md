---
name: slides
description: "Turn oxframe scenes into a presentation: stops on click, speaker notes in a presenter window, slide clickers, and a PDF of every stop. Use for talks, conference slides, or 'make this video a presentation' requests."
---
# Slides from scenes

Uses the `oxframe` skill (workflow, full API cheat sheet); install both with `npx skills add oxygenframe/oxframe-sdk`.

```rust
#[oxframe::scene]
async fn opening(view: View) {
    notes("Introduce yourself.");
    let title = view.add(TitleCard::new("Video as code"));
    title.enter().await;
    wait_for(1.5).await;
    slide_break().await;

    notes("One point per click.");
    let points = view.add(BulletList::new(["Scenes are async Rust", "Every value is a signal"]));
    points.enter().await;
    for _ in 0..2 {
        points.next().await;
        wait_for(1.5).await;
        slide_break().await;
    }
}
```

- `slide_break().await` is a stop; the end of every scene is one too. A render
  plays straight through, so hold with `wait_for` before each break (or `check`
  reports `too_fast`). Do not fade out the last point; the next scene's
  transition takes it away.
- `notes("…")` belong to the part that ends at the next stop.
- `oxframe present` opens the talk full screen: → / space / PageDown next,
  ← / PageUp back, F full screen, N the notes window with a timer.
- `oxframe render --slides talk.pdf` writes one page per stop.
