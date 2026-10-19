//! Lesson 7 — Timing you can change in the editor.
//!
//! Some values are easier to tune by eye than in code. `wait_until("name")`
//! creates a marker on the timeline, `tune!("name", default)` a slider in the
//! inspector and `ease!("name")` a curve in the curve editor. Changing them
//! does not recompile anything: the values are stored in `<scene>.meta.json`.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn editor(view: View) {
    let size = tune!("size", 160.0);
    let dot = view.add(Circle::new().size(size).x(-400.0).fill(color!("#68abdf")));

    // Drag the "go" marker on the timeline to start the move later or earlier.
    wait_until("go").await;
    dot.x().to(400.0, 1.0).ease(ease!("move")).await;

    // Drag the "fade" marker, too.
    wait_until("fade").await;
    dot.opacity().to(0.0, 0.5).await;
}

// Try (in the editor): drag the markers, move the `size` slider, click the
// `move` ease and bend the curve.

pub fn project() -> Project {
    Project::new("editor").scene(editor())
}
