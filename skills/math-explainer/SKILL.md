---
name: math-explainer
description: "Make a math or algorithm explainer video in the style of 3Blue1Brown with oxframe: axes and function plots, a dot riding a curve, vectors and linear maps, matrices step by step. Use for math animation, visual proofs, linear algebra or calculus explainers."
---
# Math explainer

Uses the `oxframe` skill (workflow, full API cheat sheet); install both with `npx skills add oxygenframe/oxframe-sdk`.

```rust
#[oxframe::scene]
async fn derivative(view: View) {
    let axes = view.add(Axes::new().x_range(-3.0..3.0).y_range(-1.5..1.5));
    let f = axes.plot(|x| x.sin());
    f.draw_on().await;
    let x = Signal::new(-3.0);
    axes.point_on(&f, x.clone());
    x.to(3.0, 3.0).await;
    f.to_fn(|x| x.cos(), 1.0).await;          // morph into the derivative
}
```

- Functions: `axes.plot(|x| …)` follows the ranges (`axes.x_range().to(..)` zooms)
  and keeps gaps (tan). `point_on`, `to_fn`, `axes.place(node, x, y)`.
- Linear algebra: `axes.vector(x, y)` (tip is signals), `axes.apply_matrix([[a, b], [c, d]], secs)`
  bends the grid and everything on it; `Matrix::new([[1, 2], [3, 4]])` with
  `row_frame`, `column_frame`, `cell(r, c)` to point at entries.
- Number lines and bars: `NumberLine::new(0.0..10.0)`, `axes.bars([..])`.
- 3D surfaces or solids: `Scene3d` + `View3d` (feature `3d`).
- Formulas: `Math::new("sum_(k=1)^n k = n(n+1)/2")`, Typst math syntax, with
  `features = ["math"]` (native only; the browser playground does not have it).
  Short expressions also work as Unicode in `Txt` (`f′(x) = cos x`, `√`, `x²`).
- Pace like the classics: one idea, one motion, a pause (`wait_for`) to let it land.
