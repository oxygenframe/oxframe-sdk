---
name: manim-to-oxframe
description: "Port a Manim (Python) scene to oxframe (Rust): construct, self.play with Create/FadeIn/Transform, Axes and plots, ValueTracker, NumberPlane and apply_matrix, Matrix, 3D. Use when migrating from Manim CE or ManimGL or making 3Blue1Brown-style videos without Python."
---
# From Manim to oxframe

Uses the `oxframe` skill (workflow, full API cheat sheet); install both with `npx skills add oxygenframe/oxframe-sdk`.

| Manim | oxframe |
| --- | --- |
| `class S(Scene): def construct(self):` | `#[oxframe::scene] async fn s(view: View)` |
| `self.play(Create(circle))` | `let c = view.add(Circle::new().size(200.0)); c.draw_on().await;` |
| `self.play(FadeIn(m))` / `FadeOut` | `m.fade_in().await` / `m.fade_out().await` |
| `self.play(a.animate.shift(RIGHT))` | `a.x().to(a.x().get() + 100.0, 1.0).await` |
| `self.play(A, B)` | `all((a, b)).await` |
| `self.wait(1)` | `wait_for(1.0).await` |
| `Transform(text_a, text_b)` | `txt.morph_to("new text", 1.0).await` |
| `Text("Hi")` | `Txt::new().text("Hi").h1()` |
| `MathTex(r"\frac{a}{b}")` | not yet: Unicode in `Txt` (`a/b`, `√x`, `x²`) |
| `Axes(x_range=[-3, 3])`, `axes.plot(lambda x: …)` | `Axes::new().x_range(-3.0..3.0)`, `axes.plot(|x| …)` |
| `ValueTracker(0)`, `always_redraw` | `Signal::new(0.0)`, `.bind(move || …)` or `axes.point_on(&plot, x)` |
| `NumberPlane()`, `apply_matrix(m)` | `Axes::new()`, `axes.apply_matrix([[a, b], [c, d]], secs)` |
| `Vector([1, 2])` | `axes.vector(1.0, 2.0)` |
| `Matrix([[1, 2], [3, 4]])` | `Matrix::new([[1, 2], [3, 4]])` (`row_frame`, `cell`) |
| `ThreeDScene` | `Scene3d` + `View3d` (feature `3d`) |
| `self.camera.frame.animate.scale(0.5)` | `Camera` node: `camera.zoom().to(2.0, 1.0)` |
| `manim -pqh scene.py S` | `oxframe render` (`oxframe dev` for a live editor) |

No Cairo, Pango or LaTeX to install, and the browser playground at
oxframe.dev runs the same scenes. Keep Manim's pacing: one animation per
`.await`, short waits between ideas, then `oxframe check`.
