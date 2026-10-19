//! Lesson 1 — Hello: add a shape and move it.
//!
//! A scene is an `async fn`. You add shapes to the `view`, then animate
//! their properties. `.await` means "wait until this animation is done".

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn hello(view: View) {
    // Add a circle on the left. `view.add` gives back a handle to animate it.
    let ball = view.add(Circle::new().size(160.0).x(-400.0).fill(color!("#68abdf")));

    // Move it to the right over 1 second.
    ball.x().to(400.0, 1.0).await;

    // When that is done, shrink it to nothing over half a second.
    ball.size().to(0.0, 0.5).await;
}

// Try:
// - change `1.0` to `3.0`: the move takes three seconds;
// - add `ball.fill().to(color!("#e13238"), 1.0).await;` before the shrink.

pub fn project() -> Project {
    Project::new("hello").scene(hello())
}
