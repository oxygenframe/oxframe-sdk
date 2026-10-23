//! A linear map on the plane: the basis vectors and the grid follow a
//! shear, then a rotation, while the matrix of the map is shown beside it.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn shear(view: View) {
    let axes = view.add(
        Axes::new()
            .x_range(-4.0..4.0)
            .y_range(-3.0..3.0)
            .width(880.0)
            .height(660.0)
            .x(-180.0)
            .y(30.0),
    );
    let i = axes.vector(1.0, 0.0);
    i.label("i");
    let j = axes.vector(0.0, 1.0).stroke(Success);
    j.label("j");
    let v = axes.vector(2.0, 1.0).stroke(Warning);
    v.label("v");
    let m = view.add(
        Matrix::new([[1, 0], [0, 1]])
            .font_size(56.0)
            .cell_width(56.0)
            .x(640.0)
            .y(30.0),
    );
    wait_for(0.5).await;

    // Shear: x picks up y.
    m.cell(0, 1).text().set("1");
    m.cell(0, 1).fill().set(Accent);
    axes.apply_matrix([[1.0, 1.0], [0.0, 1.0]], 1.5).await;
    wait_for(0.5).await;

    // Then a quarter turn, after the shear: the product shows in the matrix.
    let turn = Mat2([[0.0, -1.0], [1.0, 0.0]]);
    let total = turn.after(&axes.matrix().get());
    axes.apply_matrix(turn, 1.5).await;
    for (r, row) in total.0.iter().enumerate() {
        for (c, value) in row.iter().enumerate() {
            m.cell(r, c).text().set(format!("{value}"));
        }
    }
    wait_for(0.5).await;

    // Back to the identity; the vector's tip is a signal of its own.
    all((axes.matrix().to(Mat2::IDENTITY, 1.2), v.to(-1.0, 2.0, 1.2))).await;
    for (r, row) in [[1, 0], [0, 1]].iter().enumerate() {
        for (c, value) in row.iter().enumerate() {
            m.cell(r, c).text().set(value.to_string());
            m.cell(r, c).fill().set(Text);
        }
    }
    wait_for(0.6).await;
}

pub fn project() -> Project {
    Project::new("linear_map").scene(shear())
}
