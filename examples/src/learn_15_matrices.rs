//! Lesson 15 — Vectors and matrices.
//!
//! A vector is an arrow on `Axes`. `apply_matrix` maps the whole plane
//! through a 2 × 2 matrix: the grid and every vector follow. `Matrix` shows
//! the numbers next to it.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn matrices_lesson(view: View) {
    let axes = view.add(
        Axes::new()
            .x_range(-3.0..3.0)
            .y_range(-3.0..3.0)
            .width(640.0)
            .height(640.0)
            .x(-220.0),
    );

    // An arrow from the origin; its tip is two signals.
    let v = axes.vector(1.0, 2.0);
    v.draw_on().await;
    v.label("v");

    // The matrix, beside the axes.
    let m = view.add(Matrix::new([[2, 0], [0, 1]]).font_size(56.0).x(480.0));
    m.fade_in().await;

    // Stretch x by 2: the grid and the vector follow.
    axes.apply_matrix([[2.0, 0.0], [0.0, 1.0]], 1.5).await;
    wait_for(0.5).await;

    // Move the tip on its own; it is still stretched.
    v.to(-1.0, 1.0, 1.0).await;
    wait_for(0.4).await;
}

// Try: a shear, `axes.apply_matrix([[1.0, 1.0], [0.0, 1.0]], 1.5)`.

pub fn project() -> Project {
    Project::new("matrices_lesson").scene(matrices_lesson())
}
